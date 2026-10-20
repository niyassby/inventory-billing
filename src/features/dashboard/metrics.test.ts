import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { changeFromYesterday, columnTotal, lastDays, salesByDay, topItems } from "./metrics";

function table(
  columns: string[],
  rows: unknown[][],
  totals: unknown[] | null = null,
): Api.ReportTable {
  return {
    report: "x",
    title: "x",
    from: "2026-10-01",
    to: "2026-10-20",
    columns: columns.map((key) => ({ key, label: key, kind: "text" })),
    rows,
    totals,
    truncated: false,
  };
}

describe("dashboard metrics", () => {
  it("lists the last days, oldest first, across a month boundary", () => {
    expect(lastDays("2026-11-02", 4)).toEqual([
      "2026-10-30",
      "2026-10-31",
      "2026-11-01",
      "2026-11-02",
    ]);
  });

  it("uses the totals row, or sums the rows when there is none", () => {
    const rows = [
      ["a", 100],
      ["b", 250],
    ];
    expect(columnTotal(table(["name", "total"], rows, ["Total", 350]), "total")).toBe(350);
    expect(columnTotal(table(["name", "total"], rows), "total")).toBe(350);
    expect(columnTotal(table(["name", "total"], rows), "missing")).toBe(0);
    expect(columnTotal(undefined, "total")).toBe(0);
  });

  it("fills days without sales with zero", () => {
    const t = table(
      ["date", "bills", "net"],
      [
        ["2026-10-19", 3, 5000],
        ["2026-10-20", 2, 7000],
      ],
    );
    const points = salesByDay(t, ["2026-10-18", "2026-10-19", "2026-10-20"]);
    expect(points.map((p) => p.net)).toEqual([0, 5000, 7000]);
    expect(points.map((p) => p.bills)).toEqual([0, 3, 2]);
    expect(salesByDay(undefined, ["2026-10-20"])).toEqual([
      { date: "2026-10-20", net: 0, bills: 0 },
    ]);
  });

  it("ranks items by revenue", () => {
    const t = table(
      ["item", "sku", "qty", "revenue"],
      [
        ["Tea", "T", 2000, 300],
        ["Rice", "R", 1000, 900],
        ["Oil", "O", 3000, 600],
      ],
    );
    expect(topItems(t, 2).map((i) => i.item)).toEqual(["Rice", "Oil"]);
  });

  it("compares today with yesterday, and says nothing when yesterday had no sales", () => {
    expect(
      changeFromYesterday([
        { date: "a", net: 1000, bills: 1 },
        { date: "b", net: 1500, bills: 2 },
      ]),
    ).toBeCloseTo(0.5);
    expect(
      changeFromYesterday([
        { date: "a", net: 0, bills: 0 },
        { date: "b", net: 1500, bills: 2 },
      ]),
    ).toBeNull();
  });
});
