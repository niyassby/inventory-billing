import { useEffect, useMemo } from "react";
import { create } from "zustand";
import { useSession } from "@/stores/session";
import {
  DEFAULT_BINDINGS,
  mergeSaved,
  overridesOf,
  validateBinding,
  type BindingProblem,
  type Bindings,
  type ShortcutAction,
} from "./shortcuts";

/**
 * Each person's own shortcuts. They are kept per PC and per user (browser storage keyed by the
 * user's id): the hub has no place for personal preferences yet, so on another PC the shortcuts
 * start from the defaults again (docs/ASSUMPTIONS.md A-92).
 */

const KEY = (userId: string) => `pos-shortcuts:${userId}`;

function load(userId: string): Bindings {
  try {
    const text = localStorage.getItem(KEY(userId));
    return mergeSaved(text ? (JSON.parse(text) as unknown) : null);
  } catch {
    return { ...DEFAULT_BINDINGS };
  }
}

function save(userId: string, bindings: Bindings): void {
  try {
    const changed = overridesOf(bindings);
    if (Object.keys(changed).length === 0) localStorage.removeItem(KEY(userId));
    else localStorage.setItem(KEY(userId), JSON.stringify(changed));
  } catch {
    // Not remembered (private mode); the change still applies until the window closes.
  }
}

interface ShortcutState {
  byUser: Record<string, Bindings>;
  ensure: (userId: string) => Bindings;
  /** Returns the problem when the key is not allowed; otherwise saves it. */
  set: (userId: string, action: ShortcutAction, binding: string) => BindingProblem | null;
  reset: (userId: string, action: ShortcutAction) => void;
  resetAll: (userId: string) => void;
}

export const useShortcutStore = create<ShortcutState>((set, get) => ({
  byUser: {},
  ensure: (userId) => {
    const existing = get().byUser[userId];
    if (existing) return existing;
    const loaded = load(userId);
    set((s) => ({ byUser: { ...s.byUser, [userId]: loaded } }));
    return loaded;
  },
  set: (userId, action, binding) => {
    const current = get().ensure(userId);
    const problem = validateBinding(binding, action, current);
    if (problem) return problem;
    const next = { ...current, [action]: binding };
    save(userId, next);
    set((s) => ({ byUser: { ...s.byUser, [userId]: next } }));
    return null;
  },
  reset: (userId, action) => {
    const current = get().ensure(userId);
    const next = { ...current, [action]: DEFAULT_BINDINGS[action] };
    // A default may be taken by another action the person re-bound: free that one too.
    for (const [id, value] of Object.entries(next) as [ShortcutAction, string][]) {
      if (id !== action && value === DEFAULT_BINDINGS[action]) next[id] = DEFAULT_BINDINGS[id];
    }
    save(userId, next);
    set((s) => ({ byUser: { ...s.byUser, [userId]: next } }));
  },
  resetAll: (userId) => {
    save(userId, DEFAULT_BINDINGS);
    set((s) => ({ byUser: { ...s.byUser, [userId]: { ...DEFAULT_BINDINGS } } }));
  },
}));

/** The signed-in user's shortcuts and the functions to change them. */
export function useShortcuts() {
  const userId = useSession((s) => s.user?.id ?? "");
  const stored = useShortcutStore((s) => s.byUser[userId]);
  const ensure = useShortcutStore((s) => s.ensure);
  // Until the store has them, read them without writing to the store during a render.
  const initial = useMemo(() => (userId ? load(userId) : { ...DEFAULT_BINDINGS }), [userId]);
  useEffect(() => {
    if (userId) ensure(userId);
  }, [userId, ensure]);
  const { set, reset, resetAll } = useShortcutStore.getState();
  return {
    bindings: stored ?? initial,
    set: (action: ShortcutAction, binding: string) => set(userId, action, binding),
    reset: (action: ShortcutAction) => reset(userId, action),
    resetAll: () => resetAll(userId),
  };
}
