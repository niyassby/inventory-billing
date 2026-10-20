import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { cellText, fileName, rawValue, tableToCsv, tableToHtml } from "./export";

const table: Api.ReportTable = {
  report: "sales_summary",
  title: "Sales summary",
  from: "2026-10-01",
  to: "2026-10-31",
  columns: [
    { key: "date", label: "Date", kind: "date" },
    { key: "name", label: "Name", kind: "text" },
    { key: "qty", label: "Qty", kind: "qty" },
    { key: "total", label: "Total", kind: "money" },
    { key: "margin", label: "Margin", kind: "percent" },
  ],
  rows: [
    ["2026-10-17", 'Rice, "basmati"', 1500, 123_450, 600_000],
    ["2026-10-18", "=SUM(A1)", 1000, -500, null],
  ],
  totals: ["Total", null, 2500, 122_950, null],
  truncated: false,
};

describe("export", () => {
  it("shows cells the way the screen does", () => {
    const [date, , qty, money, percent] = table.columns as [
      Api.Column,
      Api.Column,
      Api.Column,
      Api.Column,
      Api.Column,
    ];
    expect(cellText(date, "2026-10-17")).toBe("17 Oct 2026");
    expect(cellText(qty, 1500)).toBe("1.5");
    expect(cellText(money, 123_450)).toBe("₹1,234.50");
    expect(cellText(percent, 600_000)).toBe("60.0%");
    expect(cellText(money, null)).toBe("");
    expect(cellText({ key: "r", label: "Rate", kind: "rate" }, 180_000)).toBe("18%");
  });

  it("gives spreadsheets exact numbers", () => {
    const money = table.columns[3] as Api.Column;
    expect(rawValue(money, 123_450, 2)).toBe(1234.5);
    expect(rawValue(money, 5, 3)).toBe(0.005);
    expect(rawValue(table.columns[2] as Api.Column, 1500, 2)).toBe(1.5);
    expect(rawValue(table.columns[4] as Api.Column, 600_000, 2)).toBe(0.6);
    expect(rawValue(money, null, 2)).toBe("");
  });

  it("writes safe CSV", () => {
    const csv = tableToCsv(table);
    const lines = csv.replace("﻿", "").trim().split("\r\n");
    expect(lines[0]).toBe("Date,Name,Qty,Total,Margin");
    expect(lines[1]).toBe('2026-10-17,"Rice, ""basmati""",1.5,1234.5,0.6');
    // A text cell that looks like a formula is quoted as text, a negative number stays a number.
    expect(lines[2]).toBe("2026-10-18,'=SUM(A1),1,-5,");
    expect(lines[3]).toBe("Total,,2.5,1229.5,");
  });

  it("makes a printable page and file names", () => {
    const html = tableToHtml({ ...table, title: "Sales <summary>" }, "Ravi & Sons");
    expect(html).toContain("Sales &lt;summary&gt;");
    expect(html).toContain("Ravi &amp; Sons");
    expect(html).toContain('class="total"');
    expect(html).toContain("₹1,234.50");
    expect(fileName(table, "csv")).toBe("sales-summary-2026-10-01-to-2026-10-31.csv");
  });
});
