import { useRef } from "react";
import type { ReactNode } from "react";
import type { Api } from "@inventory/shared";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { showQty } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { PickerHandle } from "./pickers";
import { ProductPicker } from "./pickers";
import { QtyInput } from "./inputs";

export interface ItemLine {
  key: string;
  item: Api.SearchItem;
  /** Thousandths of the line's unit; null while being typed. */
  qty: number | null;
}

export interface LineColumn<L extends ItemLine> {
  header: string;
  align?: "right";
  className?: string;
  cell: (line: L, update: (change: Partial<L>) => void) => ReactNode;
}

interface ItemLinesProps<L extends ItemLine> {
  lines: L[];
  onChange: (lines: L[]) => void;
  /** A new line for a picked item. */
  makeLine: (item: Api.SearchItem) => L;
  columns?: LineColumn<L>[];
  qtyLabel?: string;
  /** Adjustments: allow "−3" for stock going out. */
  allowNegative?: boolean;
  warehouseId?: string | null;
  showStock?: boolean;
  disabled?: boolean;
  emptyText?: string;
}

let counter = 0;
export function lineKey(): string {
  counter += 1;
  return `l${counter}`;
}

/** Items of a document: search or scan to add, then quantities and per-line fields. */
export function ItemLines<L extends ItemLine>({
  lines,
  onChange,
  makeLine,
  columns = [],
  qtyLabel = "Qty",
  allowNegative,
  warehouseId,
  showStock,
  disabled,
  emptyText = "Search or scan items to add them.",
}: ItemLinesProps<L>) {
  const picker = useRef<PickerHandle>(null);
  const update = (key: string, change: Partial<L>) =>
    onChange(lines.map((l) => (l.key === key ? { ...l, ...change } : l)));
  return (
    <div className="flex flex-col gap-3">
      {!disabled && (
        <ProductPicker
          ref={picker}
          warehouseId={warehouseId ?? null}
          onSelect={(item) => {
            onChange([...lines, makeLine(item)]);
          }}
        />
      )}
      {lines.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border bg-card/50 p-6 text-center text-sm text-muted-foreground">
          {emptyText}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-border bg-card shadow-xs">
          <table className="w-full text-table">
            <thead className="bg-muted/60 text-xs text-muted-foreground">
              <tr>
                <th className="px-3 py-2 text-left font-medium">Item</th>
                {showStock && <th className="px-3 py-2 text-right font-medium">In stock</th>}
                <th className="w-32 px-3 py-2 text-right font-medium">{qtyLabel}</th>
                {columns.map((c) => (
                  <th
                    key={c.header}
                    className={cn(
                      "px-3 py-2 font-medium",
                      c.align === "right" ? "text-right" : "text-left",
                      c.className,
                    )}
                  >
                    {c.header}
                  </th>
                ))}
                <th className="w-10" />
              </tr>
            </thead>
            <tbody>
              {lines.map((l) => (
                <tr key={l.key} className="border-t border-border/60 align-middle">
                  <td className="px-3 py-1.5">
                    <p className="font-medium">{l.item.name}</p>
                    <p className="text-caption">
                      {[l.item.sku, l.item.unitCode].filter(Boolean).join(" · ")}
                    </p>
                  </td>
                  {showStock && (
                    <td className="px-3 py-1.5 text-right num">{showQty(l.item.stock)}</td>
                  )}
                  <td className="px-3 py-1.5">
                    <QtyInput
                      aria-label={`${qtyLabel} of ${l.item.name}`}
                      disabled={disabled}
                      value={l.qty}
                      onValueChange={(qty) => update(l.key, { qty } as Partial<L>)}
                      {...(allowNegative ? {} : { min: 0 })}
                    />
                  </td>
                  {columns.map((c) => (
                    <td
                      key={c.header}
                      className={cn(
                        "px-3 py-1.5",
                        c.align === "right" && "text-right tabular-nums",
                        c.className,
                      )}
                    >
                      {c.cell(l, (change) => update(l.key, change))}
                    </td>
                  ))}
                  <td className="px-1">
                    {!disabled && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Remove ${l.item.name}`}
                        onClick={() => onChange(lines.filter((x) => x.key !== l.key))}
                      >
                        <X />
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** A warehouse (store) chooser fed by `stock_warehouses`. */
export { WarehouseSelect } from "./warehouse-select";
