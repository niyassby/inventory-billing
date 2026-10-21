/**
 * Calendar days travel as `YYYY-MM-DD` text (the backend's business dates). The date picker works
 * with `Date` objects in the viewer's local time; these two functions convert between them without
 * ever going through UTC, so a day never shifts by one.
 */

const ISO = /^(\d{4})-(\d{2})-(\d{2})$/;

/** `2026-10-20` → a local `Date` for that day; anything else (including 2026-02-31) → undefined. */
export function parseIsoDate(iso: string | null | undefined): Date | undefined {
  const match = iso ? ISO.exec(iso) : null;
  if (!match) return undefined;
  const [y, m, d] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const date = new Date(y, m - 1, d);
  return date.getFullYear() === y && date.getMonth() === m - 1 && date.getDate() === d
    ? date
    : undefined;
}

/** A local `Date` → `YYYY-MM-DD` for the same calendar day. */
export function toIsoDate(date: Date): string {
  const y = String(date.getFullYear()).padStart(4, "0");
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** Whether `iso` is inside the optional `min` / `max` days (inclusive). */
export function isoInRange(iso: string, min?: string, max?: string): boolean {
  return (!min || iso >= min) && (!max || iso <= max);
}
