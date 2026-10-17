import { describe, expect, it } from "vitest";
import { allocateProportionally, divRound, toSafeNumber } from "./rounding";

describe("divRound", () => {
  it.each([
    // numerator, denominator, half_up, half_even, down, up
    [5n, 2n, 3n, 2n, 2n, 3n],
    [7n, 2n, 4n, 4n, 3n, 4n],
    [-5n, 2n, -3n, -2n, -2n, -3n],
    [10n, 4n, 3n, 2n, 2n, 3n],
    [11n, 4n, 3n, 3n, 2n, 3n],
    [9n, 4n, 2n, 2n, 2n, 3n],
    [6n, 3n, 2n, 2n, 2n, 2n],
    [0n, 7n, 0n, 0n, 0n, 0n],
  ])("%d / %d", (n, d, halfUp, halfEven, down, up) => {
    expect(divRound(n, d, "half_up")).toBe(halfUp);
    expect(divRound(n, d, "half_even")).toBe(halfEven);
    expect(divRound(n, d, "down")).toBe(down);
    expect(divRound(n, d, "up")).toBe(up);
  });

  it("defaults to half_up", () => {
    expect(divRound(15n, 10n)).toBe(2n);
  });

  it("handles a negative denominator", () => {
    expect(divRound(5n, -2n, "half_up")).toBe(-3n);
  });

  it("rejects division by zero", () => {
    expect(() => divRound(1n, 0n)).toThrow(RangeError);
  });
});

describe("toSafeNumber", () => {
  it("converts safe values", () => {
    expect(toSafeNumber(42n)).toBe(42);
    expect(toSafeNumber(-42n)).toBe(-42);
  });

  it("refuses unsafe values", () => {
    expect(() => toSafeNumber(BigInt(Number.MAX_SAFE_INTEGER) + 1n)).toThrow(RangeError);
    expect(() => toSafeNumber(BigInt(Number.MIN_SAFE_INTEGER) - 1n)).toThrow(RangeError);
  });
});

describe("allocateProportionally", () => {
  it("always sums to the total", () => {
    const parts = allocateProportionally(1000, [10000, 5000]);
    expect(parts).toEqual([667, 333]);
    expect(parts.reduce((a, b) => a + b, 0)).toBe(1000);
  });

  it("gives ties to the earliest index", () => {
    expect(allocateProportionally(1, [1, 1, 1])).toEqual([1, 0, 0]);
    expect(allocateProportionally(2, [1, 1, 1])).toEqual([1, 1, 0]);
  });

  it("keeps the sign of a negative total", () => {
    expect(allocateProportionally(-100, [1, 3])).toEqual([-25, -75]);
  });

  it("handles zero totals and zero weights", () => {
    expect(allocateProportionally(0, [])).toEqual([]);
    expect(allocateProportionally(0, [0, 0])).toEqual([0, 0]);
    expect(allocateProportionally(50, [0, 2])).toEqual([0, 50]);
  });

  it("rejects impossible allocations", () => {
    expect(() => allocateProportionally(5, [])).toThrow(RangeError);
    expect(() => allocateProportionally(5, [0, 0])).toThrow(RangeError);
    expect(() => allocateProportionally(5, [-1, 2])).toThrow(RangeError);
    expect(() => allocateProportionally(1.5, [1])).toThrow(RangeError);
  });
});
