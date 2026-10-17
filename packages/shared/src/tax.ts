import type { Money } from "./money";
import {
  ZERO,
  add,
  allocate,
  applyRate,
  money,
  multiplyByQty,
  roundToUnit,
  subtract,
} from "./money";
import type { Qty } from "./qty";
import type { RoundingMode } from "./rounding";
import { DEFAULT_ROUNDING, divRound, toSafeNumber } from "./rounding";

/**
 * Rates (tax and percentage discounts) are integers in parts per million: 100% = 1_000_000.
 * ppm keeps GST's 0.25% / 0.1% rates exact even after splitting into CGST + SGST halves.
 */
export const RATE_SCALE = 1_000_000;

/** 18 → 180000. Accepts up to 4 decimals ("0.25" style rates). */
export function percentToRate(percent: number): number {
  const rate = Math.round(percent * 10_000);
  if (!Number.isFinite(percent) || Math.abs(percent * 10_000 - rate) > 1e-6) {
    throw new TaxError(`Rate ${String(percent)}% has more than 4 decimal places`);
  }
  return rate;
}

export function rateToPercent(rate: number): number {
  return rate / 10_000;
}

export class TaxError extends Error {
  override name = "TaxError";
}

export interface TaxComponentRate {
  /** "CGST", "SGST", "IGST", "VAT", "CESS"… */
  code: string;
  rate: number;
}

export type Discount = { kind: "amount"; amount: Money } | { kind: "percent"; rate: number };

export interface LineInput {
  unitPrice: Money;
  qty: Qty;
  discount?: Discount | undefined;
  /** Every component that applies to the line; an empty list means untaxed. */
  taxes: readonly TaxComponentRate[];
}

export interface DocumentInput {
  lines: readonly LineInput[];
  /** true when unit prices already include tax (typical retail MRP pricing). */
  pricesIncludeTax: boolean;
  /** Bill-level discount, spread over lines in proportion to their value before tax. */
  billDiscount?: Discount | undefined;
  /** Round the grand total to a multiple of this (e.g. 100 paise); undefined = no round-off. */
  roundTotalTo?: Money | undefined;
  rounding?: RoundingMode | undefined;
}

export interface TaxAmount {
  code: string;
  rate: number;
  amount: Money;
}

export interface LineResult {
  /** unit price × qty, as entered (inclusive or exclusive of tax). */
  gross: Money;
  lineDiscount: Money;
  billDiscount: Money;
  /** Value tax is charged on. */
  taxable: Money;
  taxes: TaxAmount[];
  tax: Money;
  /** taxable + tax: what the customer pays for this line. */
  total: Money;
}

export interface DocumentResult {
  lines: LineResult[];
  /** Sum of line gross values. */
  gross: Money;
  lineDiscount: Money;
  billDiscount: Money;
  taxable: Money;
  tax: Money;
  /** Tax summed per component code + rate, for invoice footers and tax reports. */
  taxSummary: TaxAmount[];
  /** total before round-off. */
  totalBeforeRounding: Money;
  roundOff: Money;
  total: Money;
}

function validateRate(rate: number, label: string): void {
  if (!Number.isSafeInteger(rate) || rate < 0 || rate > RATE_SCALE) {
    throw new TaxError(`${label} must be an integer between 0 and ${RATE_SCALE} ppm`);
  }
}

function discountAmount(base: Money, discount: Discount | undefined, mode: RoundingMode): Money {
  if (!discount) return ZERO;
  if (discount.kind === "amount") {
    if (discount.amount < 0) throw new TaxError("Discount cannot be negative");
    if (discount.amount > base)
      throw new TaxError("Discount cannot exceed the amount it applies to");
    return discount.amount;
  }
  validateRate(discount.rate, "Discount rate");
  return applyRate(base, discount.rate, mode);
}

/**
 * GST components for a rate: intra-state supply splits into CGST + SGST halves, inter-state is
 * IGST. VAT-style regimes just pass `[{ code: "VAT", rate }]`.
 */
export function gstComponents(rate: number, interState: boolean): TaxComponentRate[] {
  validateRate(rate, "GST rate");
  if (rate === 0) return [];
  if (interState) return [{ code: "IGST", rate }];
  if (rate % 2 !== 0) {
    throw new TaxError("GST rate cannot be split evenly into CGST and SGST");
  }
  return [
    { code: "CGST", rate: rate / 2 },
    { code: "SGST", rate: rate / 2 },
  ];
}

function computeTaxes(
  net: Money,
  taxes: readonly TaxComponentRate[],
  inclusive: boolean,
  mode: RoundingMode,
): { taxable: Money; amounts: TaxAmount[] } {
  if (taxes.length === 0) {
    return { taxable: net, amounts: [] };
  }
  if (!inclusive) {
    // Exclusive: each component is computed and rounded on the taxable value separately,
    // which is how GST invoices show CGST and SGST.
    return {
      taxable: net,
      amounts: taxes.map((t) => ({
        code: t.code,
        rate: t.rate,
        amount: applyRate(net, t.rate, mode),
      })),
    };
  }
  // Inclusive: back the tax out of the price, then split it over components by rate so the
  // parts add up exactly to the tax portion.
  const totalRate = taxes.reduce((acc, t) => acc + t.rate, 0);
  const taxable = money(
    toSafeNumber(divRound(BigInt(net) * BigInt(RATE_SCALE), BigInt(RATE_SCALE + totalRate), mode)),
  );
  const tax = subtract(net, taxable);
  const parts =
    totalRate === 0
      ? taxes.map(() => ZERO)
      : allocate(
          tax,
          taxes.map((t) => t.rate),
        );
  return {
    taxable,
    amounts: taxes.map((t, i) => ({ code: t.code, rate: t.rate, amount: parts[i] ?? ZERO })),
  };
}

/**
 * Prices a whole document. Every screen preview and the backend use the same algorithm, so the
 * backend can reject any request whose totals differ from its own computation.
 *
 * Order of operations: gross → line discount → bill discount (pro-rata) → tax → round-off.
 * Discounts reduce the taxable value, as GST and VAT require.
 */
export function priceDocument(input: DocumentInput): DocumentResult {
  const mode = input.rounding ?? DEFAULT_ROUNDING;

  const stage1 = input.lines.map((line, index) => {
    if (line.qty <= 0) throw new TaxError(`Line ${index + 1}: quantity must be positive`);
    if (line.unitPrice < 0) throw new TaxError(`Line ${index + 1}: price cannot be negative`);
    for (const t of line.taxes) validateRate(t.rate, `Line ${index + 1}: tax rate`);
    const gross = multiplyByQty(line.unitPrice, line.qty, mode);
    const lineDiscount = discountAmount(gross, line.discount, mode);
    return { line, gross, lineDiscount, afterLineDiscount: subtract(gross, lineDiscount) };
  });

  const discountBase = add(...stage1.map((s) => s.afterLineDiscount));
  const billDiscountTotal = discountAmount(discountBase, input.billDiscount, mode);
  const billShares =
    billDiscountTotal === 0
      ? stage1.map(() => ZERO)
      : allocate(
          billDiscountTotal,
          stage1.map((s) => s.afterLineDiscount),
        );

  const lines: LineResult[] = stage1.map((s, i) => {
    const billDiscount = billShares[i] ?? ZERO;
    const net = subtract(s.afterLineDiscount, billDiscount);
    const { taxable, amounts } = computeTaxes(net, s.line.taxes, input.pricesIncludeTax, mode);
    const tax = add(...amounts.map((a) => a.amount));
    return {
      gross: s.gross,
      lineDiscount: s.lineDiscount,
      billDiscount,
      taxable,
      taxes: amounts,
      tax,
      total: add(taxable, tax),
    };
  });

  const summary = new Map<string, TaxAmount>();
  for (const line of lines) {
    for (const t of line.taxes) {
      const key = `${t.code}@${String(t.rate)}`;
      const existing = summary.get(key);
      summary.set(
        key,
        existing ? { ...existing, amount: add(existing.amount, t.amount) } : { ...t },
      );
    }
  }

  const totalBeforeRounding = add(...lines.map((l) => l.total));
  let total = totalBeforeRounding;
  if (input.roundTotalTo !== undefined) {
    total = roundToUnit(totalBeforeRounding, input.roundTotalTo, mode);
  }

  return {
    lines,
    gross: add(...lines.map((l) => l.gross)),
    lineDiscount: add(...lines.map((l) => l.lineDiscount)),
    billDiscount: billDiscountTotal,
    taxable: add(...lines.map((l) => l.taxable)),
    tax: add(...lines.map((l) => l.tax)),
    taxSummary: [...summary.values()].sort((a, b) =>
      a.code === b.code ? a.rate - b.rate : a.code.localeCompare(b.code),
    ),
    totalBeforeRounding,
    roundOff: subtract(total, totalBeforeRounding),
    total,
  };
}

export interface TenderResult {
  /** Amount applied to the invoice (never more than the total). */
  applied: Money;
  /** Cash handed back to the customer. */
  change: Money;
  /** Still owed after this tender (goes to the customer's receivable). */
  due: Money;
}

/**
 * Splits what the customer handed over into applied / change / due. Fixes the legacy bug where the
 * whole tendered amount was booked as paid (audit B-4).
 */
export function settleTender(total: Money, tendered: Money): TenderResult {
  if (total < 0 || tendered < 0) throw new TaxError("Amounts cannot be negative");
  if (tendered >= total) {
    return { applied: total, change: subtract(tendered, total), due: ZERO };
  }
  return { applied: tendered, change: ZERO, due: subtract(total, tendered) };
}
