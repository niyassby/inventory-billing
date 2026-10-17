import type { Qty } from "./qty";
import { QTY_SCALE } from "./qty";
import type { RoundingMode } from "./rounding";
import { DEFAULT_ROUNDING, allocateProportionally, divRound, toSafeNumber } from "./rounding";

/**
 * Money is an integer count of the currency's minor unit (paise, cents, fils…). Never a float.
 * The brand stops a plain number (for example a quantity or a rupee amount) being passed by mistake.
 */
declare const moneyBrand: unique symbol;
export type Money = number & { readonly [moneyBrand]: true };

export interface Currency {
  /** ISO 4217 code. */
  code: string;
  /** Number of minor-unit digits (INR 2, KWD 3, JPY 0). */
  minorDigits: number;
}

export const CURRENCIES = {
  INR: { code: "INR", minorDigits: 2 },
  USD: { code: "USD", minorDigits: 2 },
  EUR: { code: "EUR", minorDigits: 2 },
  GBP: { code: "GBP", minorDigits: 2 },
  AED: { code: "AED", minorDigits: 2 },
  SAR: { code: "SAR", minorDigits: 2 },
  KWD: { code: "KWD", minorDigits: 3 },
  JPY: { code: "JPY", minorDigits: 0 },
} as const satisfies Record<string, Currency>;

export const DEFAULT_CURRENCY: Currency = CURRENCIES.INR;

export class MoneyError extends Error {
  override name = "MoneyError";
}

export function money(minor: number): Money {
  if (!Number.isSafeInteger(minor)) {
    throw new MoneyError(`Money must be a safe integer of minor units, got ${String(minor)}`);
  }
  // Normalise -0 so equality checks and serialisation behave.
  return (minor === 0 ? 0 : minor) as Money;
}

export const ZERO: Money = money(0);

function minorFactor(currency: Currency): bigint {
  return 10n ** BigInt(currency.minorDigits);
}

const DECIMAL_PATTERN = /^([+-])?(\d+)(?:\.(\d+))?$/;

/**
 * Parses a user-entered decimal string ("1,234.50", "-3.5", "12") into Money without going through
 * floating point. More decimals than the currency allows is an error, not a silent rounding.
 */
export function parseMoney(input: string, currency: Currency = DEFAULT_CURRENCY): Money {
  const cleaned = input.trim().replace(/[,\s_]/g, "");
  const match = DECIMAL_PATTERN.exec(cleaned);
  if (!match) {
    throw new MoneyError(`"${input}" is not a valid amount`);
  }
  const [, sign, whole = "0", fraction = ""] = match;
  if (fraction.length > currency.minorDigits) {
    throw new MoneyError(
      `"${input}" has more than ${currency.minorDigits} decimal places for ${currency.code}`,
    );
  }
  const padded = fraction.padEnd(currency.minorDigits, "0");
  const value = BigInt(whole) * minorFactor(currency) + BigInt(padded === "" ? "0" : padded);
  return money(toSafeNumber(sign === "-" ? -value : value));
}

/** Exact decimal string in major units, e.g. 123450 → "1234.50". Suitable for inputs and exports. */
export function toMajorString(amount: Money, currency: Currency = DEFAULT_CURRENCY): string {
  const negative = amount < 0;
  const abs = BigInt(Math.abs(amount));
  const factor = minorFactor(currency);
  const whole = (abs / factor).toString();
  if (currency.minorDigits === 0) {
    return `${negative ? "-" : ""}${whole}`;
  }
  const fraction = (abs % factor).toString().padStart(currency.minorDigits, "0");
  return `${negative ? "-" : ""}${whole}.${fraction}`;
}

/** Locale-aware display string, e.g. "₹1,234.50". Display only, never parse this back. */
export function formatMoney(
  amount: Money,
  currency: Currency = DEFAULT_CURRENCY,
  locale = "en-IN",
): string {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: currency.code,
    minimumFractionDigits: currency.minorDigits,
    maximumFractionDigits: currency.minorDigits,
  }).format(Number(toMajorString(amount, currency)));
}

/**
 * Converts a legacy floating-point major amount (the old REAL columns) to Money. Used only by
 * migration code; the second value says whether rounding changed the amount by more than 0.001.
 */
export function fromLegacyFloat(
  major: number,
  currency: Currency = DEFAULT_CURRENCY,
): { amount: Money; lossy: boolean } {
  if (!Number.isFinite(major)) {
    throw new MoneyError(`Legacy amount ${String(major)} is not finite`);
  }
  const scale = 10 ** currency.minorDigits;
  const scaled = major * scale;
  const rounded = Math.round(scaled);
  // A 0.001 tolerance in minor units absorbs normal binary-float noise (0.1 + 0.2 style).
  return { amount: money(rounded), lossy: Math.abs(scaled - rounded) > 0.001 };
}

export function add(...amounts: Money[]): Money {
  return money(amounts.reduce<number>((acc, a) => acc + a, 0));
}

export function subtract(a: Money, b: Money): Money {
  return money(a - b);
}

export function negate(a: Money): Money {
  return money(0 - a);
}

export function sum(amounts: readonly Money[]): Money {
  return add(...amounts);
}

export function maxMoney(a: Money, b: Money): Money {
  return a >= b ? a : b;
}

export function minMoney(a: Money, b: Money): Money {
  return a <= b ? a : b;
}

/** unit price × quantity, rounded once to the minor unit. */
export function multiplyByQty(
  unitPrice: Money,
  qty: Qty,
  mode: RoundingMode = DEFAULT_ROUNDING,
): Money {
  return money(toSafeNumber(divRound(BigInt(unitPrice) * BigInt(qty), BigInt(QTY_SCALE), mode)));
}

/**
 * Applies a rate expressed in parts per million (see RATE_SCALE in tax.ts), rounded once.
 * e.g. applyRate(10000, 180000) = 1800 (18% of ₹100.00).
 */
export function applyRate(
  amount: Money,
  ratePpm: number,
  mode: RoundingMode = DEFAULT_ROUNDING,
): Money {
  if (!Number.isSafeInteger(ratePpm)) {
    throw new MoneyError("Rate must be an integer number of parts per million");
  }
  return money(toSafeNumber(divRound(BigInt(amount) * BigInt(ratePpm), 1_000_000n, mode)));
}

/** Rounds an amount to a multiple of `unit` (e.g. 100 paise = nearest rupee). */
export function roundToUnit(
  amount: Money,
  unit: Money,
  mode: RoundingMode = DEFAULT_ROUNDING,
): Money {
  if (unit <= 0) {
    throw new MoneyError("Rounding unit must be positive");
  }
  return money(toSafeNumber(divRound(BigInt(amount), BigInt(unit), mode) * BigInt(unit)));
}

/** Splits an amount over weights; the parts always add up exactly to the amount. */
export function allocate(amount: Money, weights: readonly number[]): Money[] {
  return allocateProportionally(amount, weights).map(money);
}
