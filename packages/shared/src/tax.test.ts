import { describe, expect, it } from "vitest";
import { money } from "./money";
import { parseQty, units } from "./qty";
import type { LineInput } from "./tax";
import {
  TaxError,
  gstComponents,
  percentToRate,
  priceDocument,
  rateToPercent,
  settleTender,
} from "./tax";

const GST18 = gstComponents(180000, false);

function line(price: number, count: number, extra: Partial<LineInput> = {}): LineInput {
  return { unitPrice: money(price), qty: units(count), taxes: GST18, ...extra };
}

describe("rates", () => {
  it("converts percentages to ppm and back", () => {
    expect(percentToRate(18)).toBe(180000);
    expect(percentToRate(0.25)).toBe(2500);
    expect(rateToPercent(2500)).toBe(0.25);
    expect(() => percentToRate(0.12345)).toThrow(TaxError);
    expect(() => percentToRate(Number.NaN)).toThrow(TaxError);
  });

  it("splits GST into CGST + SGST, or IGST across states", () => {
    expect(gstComponents(180000, false)).toEqual([
      { code: "CGST", rate: 90000 },
      { code: "SGST", rate: 90000 },
    ]);
    expect(gstComponents(2500, false)).toEqual([
      { code: "CGST", rate: 1250 },
      { code: "SGST", rate: 1250 },
    ]);
    expect(gstComponents(180000, true)).toEqual([{ code: "IGST", rate: 180000 }]);
    expect(gstComponents(0, false)).toEqual([]);
    expect(() => gstComponents(1, false)).toThrow(TaxError);
    expect(() => gstComponents(-1, true)).toThrow(TaxError);
  });
});

describe("priceDocument: exclusive pricing", () => {
  it("computes CGST and SGST per line", () => {
    const r = priceDocument({ lines: [line(10000, 2)], pricesIncludeTax: false });
    expect(r.gross).toBe(20000);
    expect(r.taxable).toBe(20000);
    expect(r.lines[0]?.taxes.map((t) => t.amount)).toEqual([1800, 1800]);
    expect(r.tax).toBe(3600);
    expect(r.total).toBe(23600);
    expect(r.roundOff).toBe(0);
  });

  it("applies line discounts before tax", () => {
    const r = priceDocument({
      lines: [line(10000, 2, { discount: { kind: "percent", rate: 100000 } })],
      pricesIncludeTax: false,
    });
    expect(r.lineDiscount).toBe(2000);
    expect(r.taxable).toBe(18000);
    expect(r.tax).toBe(3240);
    expect(r.total).toBe(21240);
  });

  it("spreads a bill discount pro-rata and reduces the taxable value", () => {
    const r = priceDocument({
      lines: [line(10000, 1), line(5000, 1)],
      pricesIncludeTax: false,
      billDiscount: { kind: "amount", amount: money(1000) },
    });
    expect(r.lines.map((l) => l.billDiscount)).toEqual([667, 333]);
    expect(r.billDiscount).toBe(1000);
    expect(r.taxable).toBe(14000);
    // 9333 × 9% = 839.97 → 840 (×2), 4667 × 9% = 420.03 → 420 (×2)
    expect(r.tax).toBe(2520);
    expect(r.total).toBe(16520);
  });

  it("supports a percentage bill discount", () => {
    const r = priceDocument({
      lines: [line(10000, 1)],
      pricesIncludeTax: false,
      billDiscount: { kind: "percent", rate: 50000 },
    });
    expect(r.billDiscount).toBe(500);
    expect(r.taxable).toBe(9500);
  });

  it("handles untaxed lines and mixed rates in the tax summary", () => {
    const r = priceDocument({
      lines: [
        line(10000, 1),
        line(10000, 1, { taxes: gstComponents(50000, false) }),
        line(10000, 1),
        line(500, 1, { taxes: [] }),
      ],
      pricesIncludeTax: false,
    });
    expect(r.taxSummary).toEqual([
      { code: "CGST", rate: 25000, amount: 250 },
      { code: "CGST", rate: 90000, amount: 1800 },
      { code: "SGST", rate: 25000, amount: 250 },
      { code: "SGST", rate: 90000, amount: 1800 },
    ]);
    expect(r.lines[3]?.tax).toBe(0);
    expect(r.lines[3]?.total).toBe(500);
  });

  it("prices fractional quantities", () => {
    const r = priceDocument({
      lines: [{ unitPrice: money(4550), qty: parseQty("1.255"), taxes: [] }],
      pricesIncludeTax: false,
    });
    expect(r.total).toBe(5710);
  });
});

describe("priceDocument: inclusive pricing", () => {
  it("backs tax out of an inclusive price", () => {
    const r = priceDocument({
      lines: [line(11800, 1, { taxes: gstComponents(180000, true) })],
      pricesIncludeTax: true,
    });
    expect(r.taxable).toBe(10000);
    expect(r.tax).toBe(1800);
    expect(r.total).toBe(11800);
  });

  it("keeps components summing to the tax portion", () => {
    const r = priceDocument({ lines: [line(10000, 1)], pricesIncludeTax: true });
    // 100 / 1.18 = 84.7457… → 84.75 taxable, 15.25 tax split 7.63 + 7.62
    expect(r.taxable).toBe(8475);
    expect(r.lines[0]?.taxes.map((t) => t.amount)).toEqual([763, 762]);
    expect(r.total).toBe(10000);
  });

  it("leaves untaxed inclusive lines alone", () => {
    const r = priceDocument({ lines: [line(999, 1, { taxes: [] })], pricesIncludeTax: true });
    expect(r.taxable).toBe(999);
    expect(r.total).toBe(999);
  });
});

describe("priceDocument: round-off and validation", () => {
  it("rounds the grand total and reports the round-off", () => {
    const up = priceDocument({
      lines: [line(1050, 1, { taxes: [] })],
      pricesIncludeTax: false,
      roundTotalTo: money(100),
    });
    expect(up.total).toBe(1100);
    expect(up.roundOff).toBe(50);

    const down = priceDocument({
      lines: [line(1049, 1, { taxes: [] })],
      pricesIncludeTax: false,
      roundTotalTo: money(100),
    });
    expect(down.total).toBe(1000);
    expect(down.roundOff).toBe(-49);
    expect(down.totalBeforeRounding).toBe(1049);
  });

  it("returns zeros for an empty document", () => {
    const r = priceDocument({ lines: [], pricesIncludeTax: false });
    expect(r.total).toBe(0);
    expect(r.lines).toEqual([]);
  });

  it("rejects invalid input", () => {
    expect(() => priceDocument({ lines: [line(100, 0)], pricesIncludeTax: false })).toThrow(
      /quantity must be positive/,
    );
    expect(() => priceDocument({ lines: [line(-1, 1)], pricesIncludeTax: false })).toThrow(
      /price cannot be negative/,
    );
    expect(() =>
      priceDocument({
        lines: [line(100, 1, { discount: { kind: "amount", amount: money(101) } })],
        pricesIncludeTax: false,
      }),
    ).toThrow(/cannot exceed/);
    expect(() =>
      priceDocument({
        lines: [line(100, 1, { discount: { kind: "amount", amount: money(-1) } })],
        pricesIncludeTax: false,
      }),
    ).toThrow(/cannot be negative/);
    expect(() =>
      priceDocument({
        lines: [line(100, 1)],
        pricesIncludeTax: false,
        billDiscount: { kind: "percent", rate: 2_000_000 },
      }),
    ).toThrow(TaxError);
    expect(() =>
      priceDocument({
        lines: [line(100, 1, { taxes: [{ code: "VAT", rate: 1.5 }] })],
        pricesIncludeTax: false,
      }),
    ).toThrow(TaxError);
  });
});

describe("settleTender", () => {
  it("returns change for overpayment instead of booking it as paid", () => {
    expect(settleTender(money(23600), money(25000))).toEqual({
      applied: 23600,
      change: 1400,
      due: 0,
    });
  });

  it("handles exact and partial payment", () => {
    expect(settleTender(money(1000), money(1000))).toEqual({ applied: 1000, change: 0, due: 0 });
    expect(settleTender(money(1000), money(0))).toEqual({ applied: 0, change: 0, due: 1000 });
    expect(settleTender(money(1000), money(400))).toEqual({ applied: 400, change: 0, due: 600 });
  });

  it("rejects negative amounts", () => {
    expect(() => settleTender(money(-1), money(0))).toThrow(TaxError);
  });
});
