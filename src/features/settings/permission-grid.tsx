import type { Api } from "@inventory/shared";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import { permissionMatrix } from "./permissions";

/**
 * One row per area (Sales, Products …) with that area's actions as chips you tick. The area's own
 * checkbox ticks or clears the whole row. Only the actions that exist are shown, so the dialog
 * never needs a wide, mostly empty grid.
 */
export function PermissionGrid({
  permissions,
  selected,
  onChange,
}: {
  permissions: Api.PermissionInfo[];
  selected: Set<string>;
  onChange: (next: Set<string>) => void;
}) {
  const matrix = permissionMatrix(permissions);
  const toggle = (codes: string[], on: boolean) => {
    const next = new Set(selected);
    for (const c of codes) {
      if (on) next.add(c);
      else next.delete(c);
    }
    onChange(next);
  };
  return (
    <div className="max-h-96 overflow-auto rounded-lg border border-border">
      <ul className="divide-y divide-border/60">
        {matrix.rows.map((row) => {
          const cells = matrix.actions.flatMap((a) => {
            const cell = row.cells[a];
            return cell ? [{ action: a, cell }] : [];
          });
          const codes = cells.map((c) => c.cell.code);
          const all = codes.every((c) => selected.has(c));
          return (
            <li key={row.group} className="flex items-start gap-4 px-3 py-2.5">
              <label className="flex w-40 shrink-0 items-center gap-2 pt-1 text-sm font-medium">
                <Checkbox
                  checked={all}
                  onCheckedChange={(on) => toggle(codes, on)}
                  aria-label={`All ${row.title} permissions`}
                />
                {row.title}
              </label>
              <div className="flex flex-wrap gap-1.5">
                {cells.map(({ action, cell }) => {
                  const on = selected.has(cell.code);
                  return (
                    <label
                      key={cell.code}
                      title={cell.description}
                      className={cn(
                        "flex cursor-pointer items-center gap-1.5 rounded-md border px-2 py-1 text-xs transition-colors",
                        on
                          ? "border-primary/40 bg-primary-soft text-primary"
                          : "border-border text-muted-foreground hover:bg-muted",
                      )}
                    >
                      <Checkbox
                        checked={on}
                        onCheckedChange={(next) => toggle([cell.code], next)}
                        aria-label={cell.description}
                      />
                      <span className="capitalize">{action.replace(/_/g, " ")}</span>
                    </label>
                  );
                })}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
