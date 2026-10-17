/**
 * Integer division with explicit rounding. All money and quantity maths goes through here so that
 * the rounding rule lives in one place (Master Prompt §7: "tax and rounding rules are centralized").
 *
 * Works on bigint so intermediate products (price × qty × rate) never lose precision.
 */
export type RoundingMode =
  /** Commercial rounding: .5 goes away from zero. Default for invoices. */
  | "half_up"
  /** Banker's rounding: .5 goes to the even neighbour. */
  | "half_even"
  /** Towards zero (truncate). */
  | "down"
  /** Away from zero. */
  | "up";

export const DEFAULT_ROUNDING: RoundingMode = "half_up";

export function divRound(
  numerator: bigint,
  denominator: bigint,
  mode: RoundingMode = DEFAULT_ROUNDING,
): bigint {
  if (denominator === 0n) {
    throw new RangeError("Division by zero");
  }
  // Normalise so the denominator is positive; the sign lives on the numerator.
  if (denominator < 0n) {
    numerator = -numerator;
    denominator = -denominator;
  }
  const negative = numerator < 0n;
  const abs = negative ? -numerator : numerator;
  const quotient = abs / denominator;
  const remainder = abs % denominator;

  let rounded = quotient;
  if (remainder !== 0n) {
    const twice = remainder * 2n;
    switch (mode) {
      case "down":
        break;
      case "up":
        rounded = quotient + 1n;
        break;
      case "half_up":
        if (twice >= denominator) rounded = quotient + 1n;
        break;
      case "half_even":
        if (twice > denominator || (twice === denominator && quotient % 2n === 1n)) {
          rounded = quotient + 1n;
        }
        break;
    }
  }
  return negative ? -rounded : rounded;
}

/** Converts a bigint result back to a JS number, refusing values outside the safe integer range. */
export function toSafeNumber(value: bigint): number {
  if (value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(Number.MIN_SAFE_INTEGER)) {
    throw new RangeError(`Value ${value.toString()} is outside the safe integer range`);
  }
  return Number(value);
}

/**
 * Splits `total` into integer parts proportional to `weights` (largest-remainder method).
 * The parts always sum exactly to `total`; ties go to the earliest index, so results are
 * deterministic and identical on every device.
 */
export function allocateProportionally(total: number, weights: readonly number[]): number[] {
  if (!Number.isSafeInteger(total)) {
    throw new RangeError("total must be a safe integer");
  }
  if (weights.length === 0) {
    if (total === 0) return [];
    throw new RangeError("Cannot allocate a non-zero total over no weights");
  }
  if (weights.some((w) => !Number.isSafeInteger(w) || w < 0)) {
    throw new RangeError("weights must be non-negative safe integers");
  }
  const weightSum = weights.reduce((acc, w) => acc + BigInt(w), 0n);
  if (weightSum === 0n) {
    if (total === 0) return weights.map(() => 0);
    throw new RangeError("Cannot allocate a non-zero total when every weight is zero");
  }

  const sign = total < 0 ? -1n : 1n;
  const absTotal = BigInt(Math.abs(total));
  const parts = weights.map((w) => {
    const exact = absTotal * BigInt(w);
    return { floor: exact / weightSum, remainder: exact % weightSum };
  });
  let leftover = absTotal - parts.reduce((acc, p) => acc + p.floor, 0n);

  const order = parts
    .map((p, index) => ({ index, remainder: p.remainder }))
    .sort((a, b) => {
      if (a.remainder === b.remainder) return a.index - b.index;
      return a.remainder > b.remainder ? -1 : 1;
    });

  const result = parts.map((p) => p.floor);
  for (const { index } of order) {
    if (leftover === 0n) break;
    result[index] = (result[index] ?? 0n) + 1n;
    leftover -= 1n;
  }
  return result.map((v) => toSafeNumber(v * sign));
}
