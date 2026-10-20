import type { Api } from "@inventory/shared";

export interface InvoiceEvent {
  key: string;
  kind: "issued" | "payment" | "payment_voided" | "return" | "voided";
  /** ISO date or date-time. */
  at: string;
  /** Payment or return amount; the issue event carries the invoice total. */
  amount?: number;
  /** Document number, payment mode or reason. */
  text?: string;
}

/**
 * The life of an invoice as a list of events, oldest first. Same-day events keep a sensible order
 * (issued, then payments, then returns, then void) because the backend dates them by business day.
 */
export function invoiceEvents(
  invoice: Api.InvoiceDetail,
  returns: Pick<Api.ReturnListItem, "id" | "docNo" | "businessDate" | "total">[],
): InvoiceEvent[] {
  const events: InvoiceEvent[] = [
    {
      key: "issued",
      kind: "issued",
      at: invoice.finalizedAt ?? invoice.businessDate,
      amount: invoice.total,
      text: invoice.createdByName ?? undefined,
    },
  ];
  for (const p of invoice.payments) {
    events.push({
      key: `pay-${p.id}`,
      kind: p.status === "voided" ? "payment_voided" : "payment",
      at: p.businessDate,
      amount: p.amount,
      text: `${p.docNo ? `${p.docNo} · ` : ""}${p.mode}`,
    });
  }
  for (const r of returns) {
    events.push({
      key: `ret-${r.id}`,
      kind: "return",
      at: r.businessDate,
      amount: r.total,
      text: r.docNo,
    });
  }
  if (invoice.status === "voided" && invoice.voidedAt) {
    events.push({
      key: "voided",
      kind: "voided",
      at: invoice.voidedAt,
      text: invoice.voidReason ?? undefined,
    });
  }
  const rank = { issued: 0, payment: 1, payment_voided: 1, return: 2, voided: 3 } as const;
  return events
    .map((e, i) => ({ e, i }))
    .sort(
      (a, b) =>
        a.e.at.slice(0, 10).localeCompare(b.e.at.slice(0, 10)) ||
        rank[a.e.kind] - rank[b.e.kind] ||
        a.i - b.i,
    )
    .map(({ e }) => e);
}
