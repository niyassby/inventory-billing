import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { invoiceEvents } from "./timeline";

function invoice(extra: Partial<Api.InvoiceDetail> = {}): Api.InvoiceDetail {
  return {
    id: "i1",
    docNo: "INV/1",
    status: "finalized",
    total: 29_200,
    businessDate: "2026-10-20",
    finalizedAt: "2026-10-20T09:30:00Z",
    createdByName: "Asha",
    payments: [],
    ...extra,
  } as Api.InvoiceDetail;
}

describe("invoiceEvents", () => {
  it("starts with the invoice being issued", () => {
    const events = invoiceEvents(invoice(), []);
    expect(events.map((e) => e.kind)).toEqual(["issued"]);
    expect(events[0]?.amount).toBe(29_200);
    expect(events[0]?.text).toBe("Asha");
  });

  it("orders payments and returns by day, then by what comes first on a day", () => {
    const events = invoiceEvents(
      invoice({
        payments: [
          {
            id: "p2",
            docNo: "RCT/2",
            mode: "upi",
            amount: 5000,
            businessDate: "2026-10-22",
            status: "finalized",
            changeGiven: 0,
          },
          {
            id: "p1",
            docNo: "RCT/1",
            mode: "cash",
            amount: 14_600,
            businessDate: "2026-10-20",
            status: "finalized",
            changeGiven: 0,
          },
        ],
      }),
      [{ id: "r1", docNo: "SRN/1", businessDate: "2026-10-20", total: 2000 }],
    );
    expect(events.map((e) => e.key)).toEqual(["issued", "pay-p1", "ret-r1", "pay-p2"]);
  });

  it("marks refunded payments and a void with its reason", () => {
    const events = invoiceEvents(
      invoice({
        status: "voided",
        voidedAt: "2026-10-21T10:00:00Z",
        voidReason: "Wrong customer",
        payments: [
          {
            id: "p1",
            docNo: "",
            mode: "cash",
            amount: 100,
            businessDate: "2026-10-20",
            status: "voided",
            changeGiven: 0,
          },
        ],
      }),
      [],
    );
    expect(events.map((e) => e.kind)).toEqual(["issued", "payment_voided", "voided"]);
    expect(events.at(-1)?.text).toBe("Wrong customer");
  });
});
