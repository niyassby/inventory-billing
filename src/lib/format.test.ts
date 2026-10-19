import { describe, expect, it } from "vitest";
import {
  label,
  moneyText,
  readMoney,
  readQty,
  readRate,
  showDate,
  showDateTime,
  showMoney,
  showQty,
  showRate,
} from "./format";

describe("format", () => {
  it("shows and reads money exactly", () => {
    expect(showMoney(123_450)).toBe("₹1,234.50");
    expect(moneyText(5)).toBe("0.05");
    expect(readMoney("1,234.5")).toEqual({ value: 123_450 });
    expect(readMoney("")).toEqual({ value: null });
    expect(readMoney("1.234").error).toContain("decimal");
    expect(readMoney("abc").error).toContain("not a valid amount");
  });

  it("shows and reads quantities", () => {
    expect(showQty(1500)).toBe("1.5");
    expect(readQty("2")).toEqual({ value: 2000 });
    expect(readQty("1.5", 0).error).toBeDefined();
    expect(readQty("x").error).toBeDefined();
    expect(readQty(" ")).toEqual({ value: null });
  });

  it("shows and reads rates in ppm", () => {
    expect(showRate(180_000)).toBe("18%");
    expect(showRate(25_000)).toBe("2.5%");
    expect(readRate("18%")).toEqual({ value: 180_000 });
    expect(readRate("2.5")).toEqual({ value: 25_000 });
    expect(readRate("").value).toBeNull();
    expect(readRate("200").error).toBe("At most 100%");
    expect(readRate("1.23456").error).toBeDefined();
  });

  it("shows dates and labels", () => {
    expect(showDate("2026-10-17")).toBe("17 Oct 2026");
    expect(showDate(null)).toBe("");
    expect(showDate("bad")).toBe("bad");
    expect(showDateTime("2026-10-17T05:30:00Z")).toContain("2026");
    expect(showDateTime("nope")).toBe("nope");
    expect(showDateTime(undefined)).toBe("");
    expect(label("bank_transfer")).toBe("Bank transfer");
  });
});
