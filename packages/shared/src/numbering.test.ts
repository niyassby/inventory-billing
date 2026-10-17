import { describe, expect, it } from "vitest";
import {
  NumberingError,
  businessDate,
  financialYear,
  formatDocNumber,
  isValidGstInvoiceNumber,
} from "./numbering";

describe("financialYear", () => {
  it("handles an April–March financial year", () => {
    expect(financialYear("2025-04-01", 4)).toEqual({
      startYear: 2025,
      endYear: 2026,
      label: "25-26",
    });
    expect(financialYear("2026-03-31", 4).label).toBe("25-26");
    expect(financialYear("2026-04-01", 4).label).toBe("26-27");
    expect(financialYear("2099-12-01", 4).label).toBe("99-00");
  });

  it("handles a calendar-year financial year", () => {
    expect(financialYear("2025-01-15", 1)).toEqual({
      startYear: 2025,
      endYear: 2025,
      label: "2025",
    });
  });

  it("rejects bad input", () => {
    expect(() => financialYear("2025-04-01", 0)).toThrow(NumberingError);
    expect(() => financialYear("01/04/2025", 4)).toThrow(NumberingError);
    expect(() => financialYear("2025-13-01", 4)).toThrow(NumberingError);
  });
});

describe("formatDocNumber", () => {
  it("fills the template tokens", () => {
    expect(
      formatDocNumber("{prefix}/{fy}/{seq}", { prefix: "INV", fy: "25-26", seq: 123, padding: 6 }),
    ).toBe("INV/25-26/000123");
    expect(
      formatDocNumber("{branch}-{prefix}{seq}", {
        prefix: "S",
        fy: "25-26",
        branch: "C2",
        seq: 7,
        padding: 0,
      }),
    ).toBe("C2-S7");
  });

  it("rejects invalid series configuration", () => {
    const base = { prefix: "INV", fy: "25-26", seq: 1, padding: 4 };
    expect(() => formatDocNumber("{prefix}", base)).toThrow(/must contain \{seq\}/);
    expect(() => formatDocNumber("{branch}{seq}", base)).toThrow(/branch/);
    expect(() => formatDocNumber("{seq}", { ...base, seq: 0 })).toThrow(NumberingError);
    expect(() => formatDocNumber("{seq}", { ...base, padding: 20 })).toThrow(NumberingError);
  });
});

describe("isValidGstInvoiceNumber", () => {
  it("allows up to 16 letters, digits, / and -", () => {
    expect(isValidGstInvoiceNumber("INV/25-26/000123")).toBe(true);
    expect(isValidGstInvoiceNumber("INV/25-26/0000123")).toBe(false);
    expect(isValidGstInvoiceNumber("INV 1")).toBe(false);
    expect(isValidGstInvoiceNumber("")).toBe(false);
  });
});

describe("businessDate", () => {
  it("uses the business timezone, not UTC", () => {
    const instant = new Date("2025-03-31T20:00:00Z"); // 01:30 on 1 April in India
    expect(businessDate(instant, "Asia/Kolkata")).toBe("2025-04-01");
    expect(businessDate(instant, "UTC")).toBe("2025-03-31");
    expect(businessDate(instant, "America/New_York")).toBe("2025-03-31");
  });
});
