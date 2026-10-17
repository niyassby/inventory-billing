/** GSTIN (India GST registration number) format and checksum validation. */

const GSTIN_PATTERN = /^[0-9]{2}[A-Z]{5}[0-9]{4}[A-Z][1-9A-Z]Z[0-9A-Z]$/;
const CHARSET = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/** Check character for the first 14 characters (GSTN's base-36 mod algorithm). */
export function gstinCheckChar(first14: string): string {
  let total = 0;
  for (let i = 0; i < 14; i++) {
    const value = CHARSET.indexOf(first14.charAt(i));
    if (value < 0) throw new RangeError(`Invalid GSTIN character "${first14.charAt(i)}"`);
    const product = value * (i % 2 === 0 ? 1 : 2);
    total += Math.floor(product / 36) + (product % 36);
  }
  return CHARSET.charAt((36 - (total % 36)) % 36);
}

export function isValidGstin(value: string): boolean {
  const gstin = value.trim().toUpperCase();
  if (!GSTIN_PATTERN.test(gstin)) return false;
  return gstinCheckChar(gstin.slice(0, 14)) === gstin.charAt(14);
}

/** First two digits of a GSTIN are the state code, used to decide intra- vs inter-state supply. */
export function gstinStateCode(value: string): string | null {
  return isValidGstin(value) ? value.trim().slice(0, 2) : null;
}
