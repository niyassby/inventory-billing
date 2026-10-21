import { describe, expect, it } from "vitest";
import { isoInRange, parseIsoDate, toIsoDate } from "./dates";

describe("calendar days", () => {
  it("round-trips a day without shifting it", () => {
    for (const iso of ["2026-10-20", "2026-01-01", "2026-12-31", "2028-02-29"]) {
      const date = parseIsoDate(iso);
      expect(date).toBeDefined();
      expect(toIsoDate(date as Date)).toBe(iso);
    }
  });

  it("refuses text that is not a real day", () => {
    expect(parseIsoDate("")).toBeUndefined();
    expect(parseIsoDate(null)).toBeUndefined();
    expect(parseIsoDate("20-10-2026")).toBeUndefined();
    expect(parseIsoDate("2026-02-31")).toBeUndefined();
    expect(parseIsoDate("2026-13-01")).toBeUndefined();
  });

  it("checks a day against optional limits, inclusive", () => {
    expect(isoInRange("2026-10-20", "2026-10-20", "2026-10-20")).toBe(true);
    expect(isoInRange("2026-10-19", "2026-10-20")).toBe(false);
    expect(isoInRange("2026-10-21", undefined, "2026-10-20")).toBe(false);
    expect(isoInRange("2026-10-21")).toBe(true);
  });
});
