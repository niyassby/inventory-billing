import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useDebounced } from "./use-debounced";

/** Page sizes the table offers. */
export const PAGE_SIZES = [25, 50, 100, 200] as const;

const RESERVED = ["page", "size", "sort", "dir"] as const;

export interface TableState {
  page: number;
  size: number;
  /** The column key the server sorts by; null means the list's own default order. */
  sort: string | null;
  descending: boolean;
  filters: Record<string, string>;
}

export interface TableDefaults {
  size: number;
  sort: string | null;
  descending: boolean;
  filters: Record<string, string>;
}

function positiveInt(text: string | null, fallback: number): number {
  const n = Number(text);
  return Number.isInteger(n) && n > 0 ? n : fallback;
}

/** Reads the table state out of a query string; anything missing or invalid falls back. */
export function readTableState(params: URLSearchParams, defaults: TableDefaults): TableState {
  const size = positiveInt(params.get("size"), defaults.size);
  const sort = params.get("sort");
  const dir = params.get("dir");
  const filters: Record<string, string> = {};
  for (const [name, fallback] of Object.entries(defaults.filters)) {
    filters[name] = params.get(name) ?? fallback;
  }
  return {
    page: positiveInt(params.get("page"), 1),
    size: (PAGE_SIZES as readonly number[]).includes(size) ? size : defaults.size,
    sort: sort ? sort : defaults.sort,
    descending: sort ? dir === "desc" : defaults.descending,
    filters,
  };
}

/** The shortest query string that reproduces `state`: values equal to a default are left out. */
export function writeTableState(state: TableState, defaults: TableDefaults): URLSearchParams {
  const out = new URLSearchParams();
  if (state.page > 1) out.set("page", String(state.page));
  if (state.size !== defaults.size) out.set("size", String(state.size));
  if (state.sort !== defaults.sort || state.descending !== defaults.descending) {
    if (state.sort) {
      out.set("sort", state.sort);
      out.set("dir", state.descending ? "desc" : "asc");
    }
  }
  for (const [name, fallback] of Object.entries(defaults.filters)) {
    const value = state.filters[name] ?? fallback;
    if (value !== fallback) out.set(name, value);
  }
  return out;
}

/**
 * What a click on a column header does: a new column sorts ascending, the same column again flips
 * to descending, and a third click goes back to the list's default order.
 */
export function nextSort(
  current: { sort: string | null; descending: boolean },
  key: string,
  defaults: Pick<TableDefaults, "sort" | "descending">,
): { sort: string | null; descending: boolean } {
  if (current.sort !== key) return { sort: key, descending: false };
  if (!current.descending) return { sort: key, descending: true };
  return { sort: defaults.sort, descending: defaults.descending };
}

function savedKey(key: string): string {
  return `table:${key}`;
}

function hasTableParams(params: URLSearchParams, defaults: TableDefaults): boolean {
  return [...RESERVED, ...Object.keys(defaults.filters)].some((name) => params.has(name));
}

export interface UseTableState {
  state: TableState;
  setPage: (page: number) => void;
  setSize: (size: number) => void;
  toggleSort: (key: string) => void;
  setFilter: (name: string, value: string) => void;
  /** What to spread into a list command's input. */
  input: { page: number; pageSize: number; sort?: string; descending?: boolean };
  /** For `DataTable`'s `sort` prop. */
  sortProps: { key: string | null; descending: boolean; onChange: (key: string) => void };
}

/**
 * The state of one list: page, page size, sort and filters. It lives in the URL, so reload, the
 * back button and a shared link all show the same view, and the last view of each list is
 * remembered for the session when you return to it from the menu. Changing a filter, the sort or
 * the page size goes back to page 1.
 */
export function useTableState(
  key: string,
  options: {
    size?: number;
    sort?: string | null;
    descending?: boolean;
    filters?: Record<string, string>;
  } = {},
): UseTableState {
  const defaults = useMemo<TableDefaults>(
    () => ({
      size: options.size ?? 50,
      sort: options.sort ?? null,
      descending: options.descending ?? false,
      filters: options.filters ?? {},
    }),
    // The defaults are written once per screen as literals; their content never changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [JSON.stringify(options)],
  );
  const [params, setParams] = useSearchParams();

  // Coming back to a list from the menu: restore what was on screen last time.
  const [restored] = useState(() => {
    if (hasTableParams(params, defaults)) return null;
    try {
      return sessionStorage.getItem(savedKey(key));
    } catch {
      return null;
    }
  });
  useEffect(() => {
    if (restored) setParams(new URLSearchParams(restored), { replace: true });
  }, [restored, setParams]);

  const state = useMemo(
    () =>
      readTableState(
        restored && !hasTableParams(params, defaults) ? new URLSearchParams(restored) : params,
        defaults,
      ),
    [params, defaults, restored],
  );

  const commit = useCallback(
    (next: TableState) => {
      const written = writeTableState(next, defaults);
      setParams(written, { replace: true });
      try {
        sessionStorage.setItem(savedKey(key), written.toString());
      } catch {
        // Not remembered in private mode; the URL still carries the state.
      }
    },
    [defaults, key, setParams],
  );

  return {
    state,
    setPage: (page) => commit({ ...state, page: Math.max(1, page) }),
    setSize: (size) => commit({ ...state, size, page: 1 }),
    toggleSort: (column) => commit({ ...state, ...nextSort(state, column, defaults), page: 1 }),
    setFilter: (name, value) =>
      commit({ ...state, page: 1, filters: { ...state.filters, [name]: value } }),
    input: {
      page: state.page,
      pageSize: state.size,
      ...(state.sort ? { sort: state.sort, descending: state.descending } : {}),
    },
    sortProps: {
      key: state.sort,
      descending: state.descending,
      onChange: (column) => commit({ ...state, ...nextSort(state, column, defaults), page: 1 }),
    },
  };
}

/**
 * A search box that types freely and updates the list's `name` filter once typing pauses, so each
 * key does not reload the table. Returns the text to show and its setter.
 */
export function useSearchFilter(
  table: UseTableState,
  name = "q",
  delay = 250,
): [string, (text: string) => void] {
  const stored = table.state.filters[name] ?? "";
  const [text, setText] = useState(stored);
  const debounced = useDebounced(text.trim(), delay);
  useEffect(() => {
    if (debounced !== stored) table.setFilter(name, debounced);
    // Only a pause in typing should write to the URL.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debounced]);
  // The URL changed from outside (back button): show it.
  useEffect(() => {
    setText(stored);
  }, [stored]);
  return [text, setText];
}
