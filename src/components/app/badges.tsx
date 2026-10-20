import { Badge } from "@/components/ui/badge";
import { label } from "@/lib/format";
import { balanceTone, stockTone, type StockTone, STATUS_TONES, type StatusTone } from "./status";

const VARIANT: Record<StatusTone, "success" | "warning" | "destructive" | "info" | "secondary"> = {
  success: "success",
  warning: "warning",
  danger: "destructive",
  info: "info",
  neutral: "secondary",
};

/** Document or payment status as a small coloured chip ("Paid", "Voided", …). */
export function StatusBadge({ status }: { status: string }) {
  return <Badge variant={VARIANT[STATUS_TONES[status] ?? "neutral"]}>{label(status)}</Badge>;
}

const STOCK_LABEL: Record<Exclude<StockTone, "ok">, string> = {
  out: "Out of stock",
  low: "Low stock",
};

/**
 * Only the two states worth a second look get a badge: a column full of "In stock" is noise.
 * The number stays next to it, so colour is never the only signal.
 */
export function StockBadge({ stock, reorderLevel }: { stock: number; reorderLevel: number }) {
  const tone = stockTone(stock, reorderLevel);
  if (tone === "ok") return null;
  return <Badge variant={tone === "out" ? "destructive" : "warning"}>{STOCK_LABEL[tone]}</Badge>;
}

export function OfflineBadge() {
  return (
    <Badge variant="info" title="Made on a counter while the hub was offline">
      Offline sale
    </Badge>
  );
}

/** What a customer or supplier owes, as a chip: amber while open, green when paid ahead. */
export function BalanceChip({
  owed,
  owedLabel,
  money,
}: {
  owed: number;
  /** "They owe" for a customer, "You owe" for a supplier. */
  owedLabel: string;
  money: (minor: number) => string;
}) {
  const tone = balanceTone(owed);
  if (tone === "settled") return <Badge variant="secondary">Settled</Badge>;
  if (tone === "advance") return <Badge variant="success">Paid ahead {money(-owed)}</Badge>;
  return (
    <Badge variant="warning">
      {owedLabel} {money(owed)}
    </Badge>
  );
}
