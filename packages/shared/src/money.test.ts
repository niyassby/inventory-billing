import { describe, expect, it } from "vitest";
import {
  CURRENCIES,
  MoneyError,
  ZERO,
  add,
  allocate,
  applyRate,
  formatMoney,
  fromLegacyFloat,
  maxMoney,
  minMoney,
  money,
  multiplyByQty,
  negate,
  parseMoney,
  roundToUnit,
  subtract,
  sum,
  toMajorString,
} from "./money";
import { parseQty, units } from "./qty";

describe("money()", () => {
  it("accepts safe integers and normalises -0", () => {
    expect(money(125)).toBe(125);
    expect(Object.is(money(-0), 0)).toBe(true);
  });

  it("rejects fractions and unsafe values", () => {
    expect(() => money(1.5)).toThrow(MoneyError);
    expect(() => money(Number.NaN)).toThrow(MoneyError);
    expect(() => money(2 ** 60)).toThrow(MoneyError);
  });
});

describe("parseMoney", () => {
  it.each([
    ["0", 0],
    ["12", 1200],
    ["12.5", 1250],
    ["12.50", 1250],
    ["1,234.56", 123456],
    [" 99.99 ", 9999],
    ["-3.5", -350],
    ["+7", 700],
    ["0.01", 1],
  ])("parses %s as INR", (input, expected) => {
    expect(parseMoney(input)).toBe(expected);
  });

  it("respects the currency's minor digits", () => {
    expect(parseMoney("1.234", CURRENCIES.KWD)).toBe(1234);
    expect(parseMoney("500", CURRENCIES.JPY)).toBe(500);
    expect(() => parseMoney("1.5", CURRENCIES.JPY)).toThrow(MoneyError);
  });

  it("rejects too many decimals instead of rounding silently", () => {
    expect(() => parseMoney("1.005")).toThrow(/decimal places/);
  });

  it.each(["", "abc", "1.2.3", "1e5", "--1", "."])("rejects %j", (input) => {
    expect(() => parseMoney(input)).toThrow(MoneyError);
  });

  it("avoids float drift (0.1 + 0.2)", () => {
    expect(add(parseMoney("0.1"), parseMoney("0.2"))).toBe(parseMoney("0.3"));
  });
});

describe("toMajorString / formatMoney", () => {
  it("formats exact decimals", () => {
    expect(toMajorString(money(123450))).toBe("1234.50");
    expect(toMajorString(money(5))).toBe("0.05");
    expect(toMajorString(money(-5))).toBe("-0.05");
    expect(toMajorString(money(1234), CURRENCIES.KWD)).toBe("1.234");
    expect(toMajorString(money(500), CURRENCIES.JPY)).toBe("500");
  });

  it("round-trips with parseMoney", () => {
    for (const v of [0, 1, 99, 100, 123456789, -42]) {
      expect(parseMoney(toMajorString(money(v)))).toBe(v);
    }
  });

  it("formats for display with the locale", () => {
    expect(formatMoney(money(123456))).toBe("₹1,234.56");
    expect(formatMoney(money(123456), CURRENCIES.USD, "en-US")).toBe("$1,234.56");
    expect(formatMoney(money(10000000))).toBe("₹1,00,000.00");
  });
});

describe("fromLegacyFloat", () => {
  it("converts clean values without loss", () => {
    expect(fromLegacyFloat(79.99)).toEqual({ amount: 7999, lossy: false });
    expect(fromLegacyFloat(0.1 + 0.2)).toEqual({ amount: 30, lossy: false });
  });

  it("flags values that needed real rounding", () => {
    expect(fromLegacyFloat(10.005).lossy).toBe(true);
    expect(fromLegacyFloat(33.333333).amount).toBe(3333);
  });

  it("rejects non-finite values", () => {
    expect(() => fromLegacyFloat(Number.POSITIVE_INFINITY)).toThrow(MoneyError);
  });
});

describe("arithmetic", () => {
  it("adds, subtracts, negates, sums", () => {
    expect(add(money(100), money(250), money(-50))).toBe(300);
    expect(add()).toBe(ZERO);
    expect(subtract(money(100), money(250))).toBe(-150);
    expect(negate(money(100))).toBe(-100);
    expect(sum([money(1), money(2), money(3)])).toBe(6);
    expect(maxMoney(money(1), money(2))).toBe(2);
    expect(minMoney(money(1), money(2))).toBe(1);
  });

  it("multiplies by fractional quantities with one rounding step", () => {
    expect(multiplyByQty(money(9999), units(3))).toBe(29997);
    // ₹45.50/kg × 1.255 kg = ₹57.1025 → ₹57.10
    expect(multiplyByQty(money(4550), parseQty("1.255"))).toBe(5710);
    // ₹10.01 × 0.5 = 5.005 → half_up 5.01, half_even 5.00
    expect(multiplyByQty(money(1001), parseQty("0.5"))).toBe(501);
    expect(multiplyByQty(money(1001), parseQty("0.5"), "half_even")).toBe(500);
  });

  it("applies ppm rates", () => {
    expect(applyRate(money(10000), 180000)).toBe(1800);
    expect(applyRate(money(999), 50000)).toBe(50); // 5% of 9.99 = 0.4995 → 0.50
    expect(() => applyRate(money(1), 1.5)).toThrow(MoneyError);
  });

  it("rounds to a unit (nearest rupee)", () => {
    expect(roundToUnit(money(23655), money(100))).toBe(23700);
    expect(roundToUnit(money(23649), money(100))).toBe(23600);
    expect(roundToUnit(money(23650), money(100))).toBe(23700);
    expect(() => roundToUnit(money(1), money(0))).toThrow(MoneyError);
  });

  it("allocates exactly", () => {
    expect(allocate(money(1000), [1, 1, 1])).toEqual([334, 333, 333]);
  });
});
