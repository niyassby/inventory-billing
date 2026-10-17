import type { RoundingMode } from "./rounding";
import { DEFAULT_ROUNDING, divRound, toSafeNumber } from "./rounding";

/**
 * Quantities are fixed-point integers with 3 decimals (thousandths of the stock unit), so 1.5 kg is
 * stored as 1500. Enough for weights, volumes and lengths without float drift.
 */
declare const qtyBrand: unique symbol;
export type Qty = number & { readonly [qtyBrand]: true };

export const QTY_DECIMALS = 3;
export const QTY_SCALE = 10 ** QTY_DECIMALS;

export class QtyError extends Error {
  override name = "QtyError";
}

export function qty(milli: number): Qty {
  if (!Number.isSafeInteger(milli)) {
    throw new QtyError(`Quantity must be a safe integer of thousandths, got ${String(milli)}`);
  }
  return (milli === 0 ? 0 : milli) as Qty;
}

/** A whole number of units, e.g. units(3) = 3 pcs. */
export function units(count: number): Qty {
  if (!Number.isSafeInteger(count)) {
    throw new QtyError(`Unit count must be an integer, got ${String(count)}`);
  }
  return qty(count * QTY_SCALE);
}

export const ZERO_QTY: Qty = qty(0);

const DECIMAL_PATTERN = /^([+-])?(\d+)(?:\.(\d+))?$/;

/** Parses "2", "1.25" or "0.005" exactly. `maxDecimals` lets a unit allow fewer (pcs → 0). */
export function parseQty(input: string, maxDecimals: number = QTY_DECIMALS): Qty {
  const cleaned = input.trim().replace(/[,\s_]/g, "");
  const match = DECIMAL_PATTERN.exec(cleaned);
  if (!match) {
    throw new QtyError(`"${input}" is not a valid quantity`);
  }
  const [, sign, whole = "0", fraction = ""] = match;
  const allowed = Math.min(maxDecimals, QTY_DECIMALS);
  if (fraction.length > allowed) {
    throw new QtyError(`"${input}" allows at most ${allowed} decimal places`);
  }
  const value = BigInt(whole) * BigInt(QTY_SCALE) + BigInt(fraction.padEnd(QTY_DECIMALS, "0"));
  return qty(toSafeNumber(sign === "-" ? -value : value));
}

/** "1500" → "1.5"; trailing zeros are dropped, `minDecimals` keeps some for display. */
export function formatQty(value: Qty, minDecimals = 0): string {
  const negative = value < 0;
  const abs = Math.abs(value);
  const whole = Math.trunc(abs / QTY_SCALE).toString();
  let fraction = (abs % QTY_SCALE).toString().padStart(QTY_DECIMALS, "0").replace(/0+$/, "");
  if (fraction.length < minDecimals) fraction = fraction.padEnd(minDecimals, "0");
  return `${negative ? "-" : ""}${whole}${fraction ? `.${fraction}` : ""}`;
}

export function addQty(...values: Qty[]): Qty {
  return qty(values.reduce<number>((acc, v) => acc + v, 0));
}

export function subtractQty(a: Qty, b: Qty): Qty {
  return qty(a - b);
}

/**
 * Converts between units using an exact ratio, e.g. 1 box = 12 pcs → scaleQty(boxes, 12, 1).
 * The result is rounded to the quantity precision.
 */
export function scaleQty(
  value: Qty,
  numerator: number,
  denominator: number,
  mode: RoundingMode = DEFAULT_ROUNDING,
): Qty {
  if (!Number.isSafeInteger(numerator) || !Number.isSafeInteger(denominator) || denominator <= 0) {
    throw new QtyError("Conversion ratio must be integers with a positive denominator");
  }
  return qty(toSafeNumber(divRound(BigInt(value) * BigInt(numerator), BigInt(denominator), mode)));
}
