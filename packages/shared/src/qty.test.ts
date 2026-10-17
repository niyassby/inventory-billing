import { describe, expect, it } from "vitest";
import {
  QtyError,
  ZERO_QTY,
  addQty,
  formatQty,
  parseQty,
  qty,
  scaleQty,
  subtractQty,
  units,
} from "./qty";

describe("qty", () => {
  it("builds quantities", () => {
    expect(qty(1500)).toBe(1500);
    expect(units(3)).toBe(3000);
    expect(Object.is(qty(-0), 0)).toBe(true);
    expect(ZERO_QTY).toBe(0);
    expect(() => qty(1.5)).toThrow(QtyError);
    expect(() => units(1.5)).toThrow(QtyError);
  });

  it.each([
    ["2", 2000],
    ["1.5", 1500],
    ["0.005", 5],
    ["1,000", 1000000],
    ["-2.25", -2250],
  ])("parses %s", (input, expected) => {
    expect(parseQty(input)).toBe(expected);
  });

  it("enforces a unit's decimal limit", () => {
    expect(() => parseQty("1.5", 0)).toThrow(/at most 0 decimal/);
    expect(() => parseQty("1.0001")).toThrow(QtyError);
    expect(() => parseQty("abc")).toThrow(QtyError);
  });

  it("formats compactly", () => {
    expect(formatQty(qty(1500))).toBe("1.5");
    expect(formatQty(qty(2000))).toBe("2");
    expect(formatQty(qty(2000), 2)).toBe("2.00");
    expect(formatQty(qty(-5))).toBe("-0.005");
  });

  it("adds and subtracts", () => {
    expect(addQty(units(1), qty(500))).toBe(1500);
    expect(subtractQty(units(1), units(3))).toBe(-2000);
  });

  it("converts units with exact ratios", () => {
    expect(scaleQty(units(2), 12, 1)).toBe(24000); // 2 boxes → 24 pcs
    expect(scaleQty(units(1), 1, 3)).toBe(333); // 1/3 rounded to 0.333
    expect(() => scaleQty(units(1), 1, 0)).toThrow(QtyError);
    expect(() => scaleQty(units(1), 1.5, 1)).toThrow(QtyError);
  });
});
