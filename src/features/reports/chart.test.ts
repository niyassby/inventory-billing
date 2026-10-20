import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { reportSeries } from "./chart";

function table(columns: [string, string][], rows: unknown[][]): Api.ReportTable {
  return {
    report: "x",
    title: "x",
    from: "2026-10-01",
    to: "2026-10-31",
    columns: columns.map(([key, kind]) => ({ key, label: key.toUpperCase(), kind })),
    rows,
    totals: null,
    truncated: false,
  };
}

describe("reportSeries", () => {
  it("draws a one-row-per-day report with a net column", () => {
    const s = reportSeries(
      table(
        [
          ["date", "date"],
          ["bills", "count"],
          ["total", "money"],
          ["net", "money"],
        ],
        [
          ["2026-10-02", 1, 500, 400],
          ["2026-10-01", 2, 300, 300],
          ["2026-10-03", 1, 100, 100],
        ],
      ),
    );
    expect(s?.label).toBe("NET");
    expect(s?.points.map((p) => p.date)).toEqual(["2026-10-01", "2026-10-02", "2026-10-03"]);
    expect(s?.points.map((p) => p.value)).toEqual([300, 400, 100]);
  });

  it("adds up several rows on the same day", () => {
    const s = reportSeries(
      table(
        [
          ["date", "date"],
          ["amount", "money"],
        ],
        [
          ["2026-10-01", 100],
          ["2026-10-01", 50],
          ["2026-10-02", 10],
          ["2026-10-03", 20],
        ],
      ),
    );
    expect(s?.points[0]?.value).toBe(150);
  });

  it("leaves grouped and short reports as tables", () => {
    expect(
      reportSeries(
        table(
          [
            ["item", "text"],
            ["revenue", "money"],
          ],
          [["a", 1]],
        ),
      ),
    ).toBeNull();
    expect(
      reportSeries(
        table(
          [
            ["date", "date"],
            ["net", "money"],
          ],
          [
            ["2026-10-01", 1],
            ["2026-10-02", 2],
          ],
        ),
      ),
    ).toBeNull();
  });
});
