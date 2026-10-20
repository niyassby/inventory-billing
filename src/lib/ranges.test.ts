import { describe, expect, it } from "vitest";
import { presetRange } from "./ranges";

describe("presetRange", () => {
  it("covers days, weeks and months", () => {
    expect(presetRange("today", "2026-10-19")).toEqual({ from: "2026-10-19", to: "2026-10-19" });
    expect(presetRange("yesterday", "2026-10-01")).toEqual({
      from: "2026-09-30",
      to: "2026-09-30",
    });
    expect(presetRange("week", "2026-10-19")).toEqual({ from: "2026-10-13", to: "2026-10-19" });
    expect(presetRange("month", "2026-10-19")).toEqual({ from: "2026-10-01", to: "2026-10-19" });
    expect(presetRange("last_month", "2026-10-19")).toEqual({
      from: "2026-09-01",
      to: "2026-09-30",
    });
    expect(presetRange("last_month", "2026-01-05")).toEqual({
      from: "2025-12-01",
      to: "2025-12-31",
    });
  });

  it("knows the financial year", () => {
    expect(presetRange("fy", "2026-10-19", 4)).toEqual({ from: "2026-04-01", to: "2026-10-19" });
    expect(presetRange("fy", "2026-02-10", 4)).toEqual({ from: "2025-04-01", to: "2026-02-10" });
    expect(presetRange("fy", "2026-02-10", 1)).toEqual({ from: "2026-01-01", to: "2026-02-10" });
    expect(presetRange("year", "2026-10-19")).toEqual({ from: "2026-01-01", to: "2026-10-19" });
  });
});
