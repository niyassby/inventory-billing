import { useEffect, useRef, useState } from "react";
import type { Api } from "@inventory/shared";
import { Banknote, CreditCard, Landmark, Plus, Smartphone, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { MoneyInput } from "@/components/app/inputs";
import { useMoney } from "@/lib/business";
import { label } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Tender } from "./cart";
import { settle } from "./cart";

const MODES: { mode: Tender["mode"]; icon: typeof Banknote; key: string }[] = [
  { mode: "cash", icon: Banknote, key: "1" },
  { mode: "card", icon: CreditCard, key: "2" },
  { mode: "upi", icon: Smartphone, key: "3" },
  { mode: "bank_transfer", icon: Landmark, key: "4" },
];

interface PaymentDialogProps {
  open: boolean;
  total: number;
  /** A customer is chosen: the rest may stay on credit. */
  creditAllowed: boolean;
  busy: boolean;
  onClose: () => void;
  onPay: (payments: Api.PaymentInput[]) => void;
}

/**
 * Takes the money. Cash defaults to the bill total; the cashier types what was handed over and
 * sees the change. Alt+1…4 choose the mode, Enter completes.
 */
export function PaymentDialog({
  open,
  total,
  creditAllowed,
  busy,
  onClose,
  onPay,
}: PaymentDialogProps) {
  const money = useMoney();
  const [tenders, setTenders] = useState<Tender[]>([{ mode: "cash", amount: total }]);
  const firstAmount = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (open) {
      setTenders([{ mode: "cash", amount: total }]);
      setTimeout(() => firstAmount.current?.focus(), 30);
    }
  }, [open, total]);
  const s = settle(total, tenders);
  const canPay = !busy && total >= 0 && (s.due === 0 || creditAllowed);
  const setTender = (i: number, t: Partial<Tender>) =>
    setTenders((list) => list.map((x, j) => (j === i ? { ...x, ...t } : x)));

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <form
          className="flex flex-col gap-5"
          onSubmit={(e) => {
            e.preventDefault();
            if (canPay) onPay(s.payments);
          }}
          onKeyDown={(e) => {
            const m = MODES.find((x) => x.key === e.key);
            if (e.altKey && m) {
              e.preventDefault();
              setTender(tenders.length - 1, { mode: m.mode });
            }
          }}
        >
          <DialogHeader>
            <DialogTitle>Payment</DialogTitle>
            <DialogDescription className="text-3xl font-semibold text-foreground tabular-nums">
              {money(total)}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            {tenders.map((t, i) => (
              <div key={i} className="flex items-center gap-2">
                <div className="flex gap-1" role="radiogroup" aria-label="Payment mode">
                  {MODES.map(({ mode, icon: Icon, key }) => (
                    <button
                      key={mode}
                      type="button"
                      role="radio"
                      aria-checked={t.mode === mode}
                      title={`${label(mode)} (Alt+${key})`}
                      onClick={() => setTender(i, { mode })}
                      className={cn(
                        "flex size-9 items-center justify-center rounded-xl border border-border",
                        t.mode === mode && "border-primary bg-primary text-primary-foreground",
                      )}
                    >
                      <Icon className="size-4" aria-hidden />
                      <span className="sr-only">{label(mode)}</span>
                    </button>
                  ))}
                </div>
                <MoneyInput
                  ref={i === 0 ? firstAmount : undefined}
                  aria-label={`${label(t.mode)} amount`}
                  className="h-10 flex-1 text-lg"
                  value={t.amount}
                  onValueChange={(v) => setTender(i, { amount: v ?? 0 })}
                />
                {tenders.length > 1 && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    aria-label="Remove payment"
                    onClick={() => setTenders((list) => list.filter((_, j) => j !== i))}
                  >
                    <Trash2 />
                  </Button>
                )}
              </div>
            ))}
            {s.due > 0 && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="w-fit"
                onClick={() => setTenders((list) => [...list, { mode: "card", amount: s.due }])}
              >
                <Plus /> Split: add another payment
              </Button>
            )}
          </div>
          <dl className="grid grid-cols-2 gap-2 rounded-2xl bg-muted/50 p-4 text-sm">
            <dt>Paid</dt>
            <dd className="text-right tabular-nums">{money(s.paid)}</dd>
            {s.change > 0 && (
              <>
                <dt className="text-lg font-semibold">Change</dt>
                <dd className="text-right text-lg font-semibold tabular-nums">{money(s.change)}</dd>
              </>
            )}
            {s.due > 0 && (
              <>
                <dt className={creditAllowed ? "" : "text-destructive"}>
                  {creditAllowed ? "On credit" : "Still to pay"}
                </dt>
                <dd className={cn("text-right tabular-nums", !creditAllowed && "text-destructive")}>
                  {money(s.due)}
                </dd>
              </>
            )}
          </dl>
          {s.due > 0 && !creditAllowed && (
            <p className="text-sm text-muted-foreground">
              Choose a customer to leave part of the bill on credit.
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Back (Esc)
            </Button>
            <Button type="submit" size="lg" disabled={!canPay}>
              {busy ? "Saving…" : "Complete sale (Enter)"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
