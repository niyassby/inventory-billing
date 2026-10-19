import type { Currency } from "@inventory/shared";
import {
  DEFAULT_CURRENCY,
  MoneyError,
  QtyError,
  formatMoney,
  formatQty,
  money,
  parseMoney,
  parseQty,
  qty,
  toMajorString,
} from "@inventory/shared";

/** Business display preferences (from settings_public). */
export interface Locale {
  currency: Currency;
  locale: string;
}

export const DEFAULT_LOCALE: Locale = { currency: DEFAULT_CURRENCY, locale: "en-IN" };

/** ₹1,234.50 (display only). */
export function showMoney(minor: number, l: Locale = DEFAULT_LOCALE): string {
  return formatMoney(money(minor), l.currency, l.locale);
}

/** 1234.50 for an input field. */
export function moneyText(minor: number, l: Locale = DEFAULT_LOCALE): string {
  return toMajorString(money(minor), l.currency);
}

/** "12.5" → 1250; empty → null; invalid → an error message. */
export function readMoney(
  text: string,
  l: Locale = DEFAULT_LOCALE,
): { value: number | null; error?: string } {
  if (text.trim() === "") return { value: null };
  try {
    return { value: parseMoney(text, l.currency) };
  } catch (e) {
    return { value: null, error: e instanceof MoneyError ? e.message : "Invalid amount" };
  }
}

/** 1500 → "1.5" (thousandths of a unit). */
export function showQty(milli: number): string {
  return formatQty(qty(milli));
}

export function readQty(text: string, maxDecimals = 3): { value: number | null; error?: string } {
  if (text.trim() === "") return { value: null };
  try {
    return { value: parseQty(text, maxDecimals) };
  } catch (e) {
    return { value: null, error: e instanceof QtyError ? e.message : "Invalid quantity" };
  }
}

/** Rates are parts per million: 180000 → "18%". */
export function showRate(ppm: number): string {
  const percent = ppm / 10_000;
  return `${Number.isInteger(percent) ? percent.toString() : percent.toFixed(2).replace(/0+$/, "")}%`;
}

/** "18" or "2.5" (%) → ppm. */
export function readRate(text: string): { value: number | null; error?: string } {
  const cleaned = text.trim().replace(/%$/, "");
  if (cleaned === "") return { value: null };
  if (!/^\d+(\.\d{1,4})?$/.test(cleaned))
    return { value: null, error: "Enter a percentage like 18 or 2.5" };
  const value = Math.round(Number(cleaned) * 10_000);
  if (value > 1_000_000) return { value: null, error: "At most 100%" };
  return { value };
}

/** "2026-10-17" → "17 Oct 2026". */
export function showDate(iso: string | null | undefined, locale = DEFAULT_LOCALE.locale): string {
  if (!iso) return "";
  const [y, m, d] = iso.slice(0, 10).split("-").map(Number);
  if (!y || !m || !d) return iso;
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(Date.UTC(y, m - 1, d)));
}

/** RFC 3339 instant → local date and time. */
export function showDateTime(
  instant: string | null | undefined,
  locale = DEFAULT_LOCALE.locale,
): string {
  if (!instant) return "";
  const date = new Date(instant);
  if (Number.isNaN(date.getTime())) return instant;
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

/** Readable labels for backend codes ("bank_transfer" → "Bank transfer"). */
export function label(code: string): string {
  const text = code.replace(/_/g, " ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}
