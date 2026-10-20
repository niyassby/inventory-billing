import type { Api } from "@inventory/shared";

const PREFERRED = ["view", "create", "manage", "receive", "pay", "return", "void", "export"];

export interface MatrixRow {
  /** "sales", "products" … */
  group: string;
  /** What the group is called on screen ("Sales"). */
  title: string;
  cells: Record<string, Api.PermissionInfo | undefined>;
}

export interface PermissionMatrix {
  actions: string[];
  rows: MatrixRow[];
}

function titleOf(group: string): string {
  const text = group.replace(/[_-]+/g, " ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * Permission codes look like `sales.create`. The role editor shows them as a grid: one row per area
 * (sales, products, …) and one column per action (view, create, manage …), which reads far
 * better than a flat list of 40 checkboxes.
 */
export function permissionMatrix(permissions: Api.PermissionInfo[]): PermissionMatrix {
  const rows = new Map<string, MatrixRow>();
  const seen = new Set<string>();
  for (const p of permissions) {
    const dot = p.code.indexOf(".");
    const group = dot < 0 ? p.code : p.code.slice(0, dot);
    const action = dot < 0 ? "use" : p.code.slice(dot + 1);
    seen.add(action);
    const row = rows.get(group) ?? { group, title: titleOf(group), cells: {} };
    row.cells[action] = p;
    rows.set(group, row);
  }
  const rank = (a: string) => {
    const i = PREFERRED.indexOf(a);
    return i < 0 ? PREFERRED.length : i;
  };
  const actions = [...seen].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
  return {
    actions,
    rows: [...rows.values()].sort((a, b) => a.title.localeCompare(b.title)),
  };
}
