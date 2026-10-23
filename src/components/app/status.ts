/** Which colour a status means. Kept apart from the badge component so it can be tested. */
export type StatusTone = "success" | "warning" | "danger" | "info" | "neutral";

export const STATUS_TONES: Record<string, StatusTone> = {
  // Payment: what is still owed is a warning, never green.
  paid: "success",
  partial: "warning",
  unpaid: "warning",
  // Documents
  finalized: "neutral",
  draft: "neutral",
  open: "info",
  accepted: "success",
  converted: "success",
  expired: "neutral",
  rejected: "danger",
  voided: "danger",
  void: "danger",
  cancelled: "danger",
  // Journals and cheques
  submitted: "info",
  approved: "info",
  posted: "success",
  reversed: "neutral",
  pending: "warning",
  cleared: "success",
  bounced: "danger",
  // Bank statement lines
  matched: "success",
  unmatched: "warning",
  ignored: "neutral",
  reopened: "warning",
  closed: "neutral",
  // Stock documents
  counting: "info",
  received: "success",
  partially_received: "warning",
};

export type StockTone = "ok" | "low" | "out";

/** Out of stock at zero or below; low at or under the reorder level (when one is set). */
export function stockTone(stock: number, reorderLevel: number): StockTone {
  if (stock <= 0) return "out";
  if (reorderLevel > 0 && stock <= reorderLevel) return "low";
  return "ok";
}

export type BalanceTone = "owed" | "advance" | "settled";

/** Positive = still owed to you (or by you); negative = paid ahead; zero = settled. */
export function balanceTone(owed: number): BalanceTone {
  if (owed > 0) return "owed";
  if (owed < 0) return "advance";
  return "settled";
}
