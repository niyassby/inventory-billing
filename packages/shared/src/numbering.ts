/** Document numbering and business-date helpers (financial year, local business date). */

export interface FinancialYear {
  /** Calendar year in which the FY starts. */
  startYear: number;
  /** Calendar year in which the FY ends (same as startYear when the FY starts in January). */
  endYear: number;
  /** "25-26" for Apr 2025–Mar 2026, "2025" for a calendar-year FY. */
  label: string;
}

export class NumberingError extends Error {
  override name = "NumberingError";
}

/**
 * Financial year containing a business date. `startMonth` is 1–12 (India: 4 = April).
 * `businessDate` is "YYYY-MM-DD" in the business timezone (see `businessDate()`).
 */
export function financialYear(businessDateIso: string, startMonth: number): FinancialYear {
  if (!Number.isInteger(startMonth) || startMonth < 1 || startMonth > 12) {
    throw new NumberingError("startMonth must be between 1 and 12");
  }
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(businessDateIso);
  if (!match) throw new NumberingError(`"${businessDateIso}" is not a YYYY-MM-DD date`);
  const year = Number(match[1]);
  const month = Number(match[2]);
  if (month < 1 || month > 12)
    throw new NumberingError(`"${businessDateIso}" has an invalid month`);

  if (startMonth === 1) {
    return { startYear: year, endYear: year, label: String(year) };
  }
  const startYear = month >= startMonth ? year : year - 1;
  const endYear = startYear + 1;
  const short = (y: number) => String(y % 100).padStart(2, "0");
  return { startYear, endYear, label: `${short(startYear)}-${short(endYear)}` };
}

export interface NumberParts {
  prefix: string;
  /** Financial-year label from `financialYear()`. */
  fy: string;
  /** Branch / counter code, used by per-device sub-series. */
  branch?: string | undefined;
  seq: number;
  /** Zero-pad the sequence to this many digits. */
  padding: number;
}

/**
 * Formats a document number from a series template. Tokens: {prefix} {fy} {branch} {seq}.
 * e.g. "{prefix}/{fy}/{seq}" → "INV/25-26/000123".
 */
export function formatDocNumber(template: string, parts: NumberParts): string {
  if (!Number.isSafeInteger(parts.seq) || parts.seq < 1) {
    throw new NumberingError("Sequence must be a positive integer");
  }
  if (!Number.isInteger(parts.padding) || parts.padding < 0 || parts.padding > 12) {
    throw new NumberingError("Padding must be between 0 and 12");
  }
  if (template.includes("{branch}") && !parts.branch) {
    throw new NumberingError("Template uses {branch} but no branch code was given");
  }
  if (!template.includes("{seq}")) {
    throw new NumberingError("Template must contain {seq}");
  }
  return template
    .replaceAll("{prefix}", parts.prefix)
    .replaceAll("{fy}", parts.fy)
    .replaceAll("{branch}", parts.branch ?? "")
    .replaceAll("{seq}", String(parts.seq).padStart(parts.padding, "0"));
}

/** Indian GST rule: invoice numbers are at most 16 characters of letters, digits, "/" and "-". */
export function isValidGstInvoiceNumber(value: string): boolean {
  return /^[A-Za-z0-9/-]{1,16}$/.test(value);
}

/**
 * The calendar date of `instant` in the business timezone, as "YYYY-MM-DD". Use this, not
 * `toISOString()` (UTC), for "today" and for an invoice's date (audit B-20).
 */
export function businessDate(instant: Date, timeZone: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(instant);
  const get = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((p) => p.type === type)?.value ?? "";
  return `${get("year")}-${get("month")}-${get("day")}`;
}
