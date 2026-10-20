import { useEffect, useRef, useState } from "react";
import type { Api } from "@inventory/shared";
import { Printer } from "lucide-react";
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
import { MoneyInput, QtyInput, RateInput } from "@/components/app/inputs";
import { OfflineBadge } from "@/components/app/badges";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showDateTime } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { documentHtml, defaultPaper, printHtml } from "@/features/sales/print";
import type { Paper } from "@/features/sales/print";
import type { CartLine } from "./cart";

/** Amount-or-percent discount editor. */
export function DiscountEditor({
  value,
  onChange,
  idPrefix,
}: {
  value: Api.Discount | null;
  onChange: (d: Api.Discount | null) => void;
  idPrefix: string;
}) {
  const kind = value?.kind ?? "percent";
  return (
    <div className="flex items-end gap-2">
      <div
        className="flex rounded-xl border border-border p-0.5"
        role="radiogroup"
        aria-label="Discount type"
      >
        {(["percent", "amount"] as const).map((k) => (
          <button
            key={k}
            type="button"
            role="radio"
            aria-checked={kind === k}
            className={`rounded-lg px-3 py-1 text-sm ${kind === k ? "bg-muted font-medium" : ""}`}
            onClick={() =>
              onChange(
                k === "percent" ? { kind: "percent", rate: 0 } : { kind: "amount", amount: 0 },
              )
            }
          >
            {k === "percent" ? "%" : "Amount"}
          </button>
        ))}
      </div>
      {kind === "percent" ? (
        <RateInput
          id={`${idPrefix}-rate`}
          aria-label="Discount percent"
          value={value?.kind === "percent" ? value.rate : null}
          onValueChange={(rate) => onChange(rate ? { kind: "percent", rate } : null)}
        />
      ) : (
        <MoneyInput
          id={`${idPrefix}-amount`}
          aria-label="Discount amount"
          value={value?.kind === "amount" ? value.amount : null}
          onValueChange={(amount) => onChange(amount ? { kind: "amount", amount } : null)}
        />
      )}
    </div>
  );
}

/** F4: quantity, price and discount of one line. */
export function LineDialog({
  line,
  onClose,
  onSave,
}: {
  line: CartLine | null;
  onClose: () => void;
  onSave: (change: {
    qty: number;
    unitPrice: number | null;
    discount: Api.Discount | null;
  }) => void;
}) {
  const [qty, setQty] = useState<number | null>(null);
  const [price, setPrice] = useState<number | null>(null);
  const [discount, setDiscount] = useState<Api.Discount | null>(null);
  const qtyRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (line) {
      setQty(line.qty);
      setPrice(line.unitPrice ?? line.item.salePrice);
      setDiscount(line.discount);
      setTimeout(() => qtyRef.current?.focus(), 30);
    }
  }, [line]);
  return (
    <Dialog open={line !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (qty !== null) onSave({ qty, unitPrice: price, discount });
          }}
        >
          <DialogHeader>
            <DialogTitle>{line?.item.name}</DialogTitle>
            <DialogDescription>
              {line?.item.sku} · {line?.item.unitCode}
            </DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-2 gap-3">
            <Field>
              <FieldLabel htmlFor="line-qty">Quantity</FieldLabel>
              <QtyInput id="line-qty" ref={qtyRef} value={qty} onValueChange={setQty} />
            </Field>
            <Field>
              <FieldLabel htmlFor="line-price">Price</FieldLabel>
              <MoneyInput id="line-price" value={price} onValueChange={setPrice} />
            </Field>
          </div>
          <Field>
            <FieldLabel>Discount on this item</FieldLabel>
            <DiscountEditor idPrefix="line-discount" value={discount} onChange={setDiscount} />
          </Field>
          <p className="text-xs text-muted-foreground">
            A changed price or a discount above your limit asks for a manager's approval.
          </p>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={qty === null}>
              Apply
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** F7: discount on the whole bill. */
export function BillDiscountDialog({
  open,
  value,
  onClose,
  onSave,
}: {
  open: boolean;
  value: Api.Discount | null;
  onClose: () => void;
  onSave: (d: Api.Discount | null) => void;
}) {
  const [discount, setDiscount] = useState<Api.Discount | null>(value);
  useEffect(() => {
    if (open) setDiscount(value);
  }, [open, value]);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            onSave(discount);
          }}
        >
          <DialogHeader>
            <DialogTitle>Bill discount</DialogTitle>
            <DialogDescription>Applied before tax, shared across the items.</DialogDescription>
          </DialogHeader>
          <DiscountEditor idPrefix="bill-discount" value={discount} onChange={setDiscount} />
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onSave(null)}>
              Remove discount
            </Button>
            <Button type="submit">Apply</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** F9: bills parked on any counter. */
export function HeldBillsDialog({
  open,
  onClose,
  onRecall,
}: {
  open: boolean;
  onClose: () => void;
  onRecall: (bill: Api.HeldBill) => void;
}) {
  const held = useCommand("sales_held_list", {}, { enabled: open });
  const recall = useAction("sales_recall", {
    invalidates: ["sales_held_list"],
    onSuccess: (bill) => onRecall(bill),
  });
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Held bills</DialogTitle>
          <DialogDescription>Recalling a bill takes it off this list.</DialogDescription>
        </DialogHeader>
        <div className="flex max-h-96 flex-col gap-1 overflow-auto">
          {held.data?.length === 0 && (
            <p className="text-sm text-muted-foreground">No held bills.</p>
          )}
          {held.data?.map((b) => (
            <button
              key={b.id}
              type="button"
              className="flex items-center justify-between rounded-xl border border-border/60 px-3 py-2 text-left text-sm hover:bg-muted focus-visible:bg-muted"
              disabled={recall.isPending}
              onClick={() => recall.mutate({ input: { id: b.id } })}
            >
              <span className="font-medium">{b.label}</span>
              <span className="text-xs text-muted-foreground">
                {b.createdByName} · {showDateTime(b.createdAt)}
              </span>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** After a sale: print in the chosen size, then the next customer. */
export function ReceiptDialog({
  invoice,
  onClose,
}: {
  invoice: Api.InvoiceDetail | null;
  onClose: () => void;
}) {
  const settings = usePublicSettings();
  const money = useMoney();
  const [paper, setPaper] = useState<Paper>(defaultPaper(settings.data));
  const change = invoice?.payments.reduce((n, p) => n + p.changeGiven, 0) ?? 0;
  const print = () => invoice && printHtml(documentHtml(invoice, settings.data, paper));
  return (
    <Dialog open={invoice !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            Sale complete {invoice?.origin === "offline" && <OfflineBadge />}
          </DialogTitle>
          <DialogDescription>
            {invoice?.docNo} · {money(invoice?.total ?? 0)}
          </DialogDescription>
        </DialogHeader>
        {change > 0 && (
          <div className="rounded-2xl bg-primary/10 p-4 text-center">
            <p className="text-sm">Give change</p>
            <p className="text-3xl font-semibold tabular-nums">{money(change)}</p>
          </div>
        )}
        {invoice?.warnings && invoice.warnings.length > 0 && (
          <p className="text-sm text-warning">
            Stock went below zero for {invoice.warnings.map((w) => w.name).join(", ")}.
          </p>
        )}
        <div className="flex gap-1" role="radiogroup" aria-label="Paper">
          {(
            [
              ["thermal_80", "80 mm"],
              ["thermal_58", "58 mm"],
              ["a4", "A4"],
            ] as const
          ).map(([p, text]) => (
            <Button
              key={p}
              type="button"
              size="sm"
              variant={paper === p ? "default" : "outline"}
              role="radio"
              aria-checked={paper === p}
              onClick={() => setPaper(p)}
            >
              {text}
            </Button>
          ))}
        </div>
        <DialogFooter>
          <Button variant="outline" data-print-receipt="true" onClick={print}>
            <Printer /> Print (P)
          </Button>
          <Button autoFocus onClick={onClose}>
            Next customer (Enter)
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Open the till with a float, or close it with the counted cash. */
export function TillDialog({
  open,
  session,
  onClose,
}: {
  open: boolean;
  session: Api.RegisterSession | null | undefined;
  onClose: () => void;
}) {
  const money = useMoney();
  const [amount, setAmount] = useState<number | null>(0);
  const [closed, setClosed] = useState<Api.RegisterSession | null>(null);
  useEffect(() => {
    if (open) {
      setAmount(session ? null : 0);
      setClosed(null);
    }
  }, [open, session]);
  const openTill = useAction("register_open", {
    invalidates: ["register_current"],
    success: "Till opened",
    onSuccess: onClose,
  });
  const closeTill = useAction("register_close", {
    invalidates: ["register_current"],
    onSuccess: (s) => setClosed(s),
  });
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (amount === null) return;
            if (session)
              closeTill.mutate({ input: { sessionId: session.id, countedCash: amount } });
            else openTill.mutate({ input: { openingFloat: amount } });
          }}
        >
          <DialogHeader>
            <DialogTitle>{session ? "Close the till" : "Open the till"}</DialogTitle>
            <DialogDescription>
              {session
                ? `Opened ${showDateTime(session.openedAt)} by ${session.openedByName ?? "—"}. Count the cash in the drawer.`
                : "Count the change in the drawer before the first sale."}
            </DialogDescription>
          </DialogHeader>
          {closed ? (
            <dl className="grid grid-cols-2 gap-2 rounded-2xl bg-muted/50 p-4 text-sm">
              <dt>Expected cash</dt>
              <dd className="text-right tabular-nums">{money(closed.expectedCash)}</dd>
              <dt>Counted</dt>
              <dd className="text-right tabular-nums">{money(closed.countedCash ?? 0)}</dd>
              <dt className="font-semibold">Difference</dt>
              <dd className="text-right font-semibold tabular-nums">
                {money(closed.difference ?? 0)}
              </dd>
            </dl>
          ) : (
            <Field>
              <FieldLabel htmlFor="till-amount">
                {session ? "Cash counted" : "Opening cash"}
              </FieldLabel>
              <MoneyInput id="till-amount" autoFocus value={amount} onValueChange={setAmount} />
            </Field>
          )}
          <DialogFooter>
            {closed ? (
              <Button type="button" onClick={onClose}>
                Done
              </Button>
            ) : (
              <>
                <Button type="button" variant="outline" onClick={onClose}>
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={amount === null || openTill.isPending || closeTill.isPending}
                >
                  {session ? "Close till" : "Open till"}
                </Button>
              </>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
