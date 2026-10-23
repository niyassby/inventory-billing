import { useState } from "react";
import type { Api } from "@inventory/shared";
import { Scale } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { QueryView, Section } from "@/components/app/page";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showQty } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { useCan } from "@/stores/session";

const NAMES: Record<string, string> = {
  weighted_average: "Weighted average cost",
  fifo: "FIFO (first in, first out)",
};

/** Settings → Inventory: the valuation method, changed only through a previewed, audited switch. */
export function ValuationSection() {
  const canSwitch = useCan("inventory.valuation");
  const settings = usePublicSettings();
  const [open, setOpen] = useState(false);
  const current = settings.data?.inventory.valuationMethod || "weighted_average";
  return (
    <Section
      title="Stock valuation"
      actions={
        canSwitch && (
          <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
            <Scale /> Change method…
          </Button>
        )
      }
    >
      <p className="text-sm">
        <Badge variant="secondary">{NAMES[current] ?? current}</Badge>{" "}
        <span className="text-muted-foreground">
          {current === "fifo"
            ? "Sales take the cost of the oldest stock first; each receipt keeps its own cost."
            : "Sales take the average cost of the stock on hand; each receipt updates the average."}
        </span>
      </p>
      {open && <SwitchDialog onClose={() => setOpen(false)} />}
    </Section>
  );
}

function SwitchDialog({ onClose }: { onClose: () => void }) {
  const money = useMoney();
  const preview = useCommand("inventory_valuation_preview", {}, { staleTime: 0 });
  const [reason, setReason] = useState("");
  const run = useAction("inventory_valuation_switch", {
    invalidates: ["settings_public", "settings_get", "inventory_valuation_preview", "reports_run"],
    success: (r) =>
      r.method === "fifo"
        ? `Switched to FIFO: ${r.layersCreated} opening layers at book value`
        : `Switched to weighted average: ${r.itemsRepriced} products repriced`,
    onSuccess: onClose,
  });
  const submit = (p: Api.ValuationPreview) =>
    run.mutate({
      input: { target: p.target, expectedTotalValue: p.totalValue, reason: reason.trim() },
    });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-3xl">
        <QueryView query={preview}>
          {(p) => (
            <>
              <DialogHeader>
                <DialogTitle>Switch to {NAMES[p.target] ?? p.target}?</DialogTitle>
                <DialogDescription>
                  {p.target === "fifo"
                    ? "Each item's stock becomes one opening layer at its current book value; later receipts form new layers. The books don't change."
                    : "Each item's cost becomes its stock value divided by its quantity. The books don't change."}
                </DialogDescription>
              </DialogHeader>
              {p.problems.length > 0 && (
                <Alert variant="danger">
                  <AlertTitle>Fix these first</AlertTitle>
                  <AlertDescription>
                    <ul className="list-disc pl-4">
                      {p.problems.map((m) => (
                        <li key={m}>{m}</li>
                      ))}
                    </ul>
                  </AlertDescription>
                </Alert>
              )}
              <div className="max-h-72 overflow-y-auto rounded-lg border border-border">
                <table className="w-full text-sm">
                  <thead className="sticky top-0 bg-card text-left text-xs text-muted-foreground">
                    <tr>
                      <th className="px-2 py-1.5 font-medium">Item</th>
                      <th className="font-medium">Store</th>
                      <th className="font-medium">Batch</th>
                      <th className="text-right font-medium">Qty</th>
                      <th className="text-right font-medium">Unit cost</th>
                      <th className="pr-2 text-right font-medium">Value</th>
                    </tr>
                  </thead>
                  <tbody>
                    {p.rows.slice(0, 500).map((r, i) => (
                      <tr key={`${r.variantId}-${i}`} className="border-t border-border/50">
                        <td className="px-2 py-1">{r.name}</td>
                        <td>{r.warehouseName}</td>
                        <td>{r.batchNo ?? ""}</td>
                        <td className="text-right tabular-nums">{showQty(r.qty)}</td>
                        <td className="text-right tabular-nums">{money(r.unitCost)}</td>
                        <td className="pr-2 text-right tabular-nums">{money(r.value)}</td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr className="border-t border-border font-medium">
                      <td className="px-2 py-1.5" colSpan={5}>
                        Stock value (unchanged)
                      </td>
                      <td className="pr-2 text-right tabular-nums">{money(p.totalValue)}</td>
                    </tr>
                  </tfoot>
                </table>
              </div>
              <Field>
                <FieldLabel htmlFor="valuation-reason">Reason (kept in the audit log)</FieldLabel>
                <Input
                  id="valuation-reason"
                  value={reason}
                  maxLength={300}
                  onChange={(e) => setReason(e.target.value)}
                />
              </Field>
              <DialogFooter>
                <Button variant="ghost" onClick={onClose}>
                  Cancel
                </Button>
                <Button
                  disabled={p.problems.length > 0 || reason.trim() === "" || run.isPending}
                  onClick={() => submit(p)}
                >
                  {run.isPending ? "Switching…" : "Switch now"}
                </Button>
              </DialogFooter>
            </>
          )}
        </QueryView>
      </DialogContent>
    </Dialog>
  );
}
