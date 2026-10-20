import type { Api } from "@inventory/shared";

export interface ReportSeries {
  /** What the line shows ("Net sales"). */
  label: string;
  /** ISO date and amount (minor units), oldest first. */
  points: { date: string; value: number }[];
}

/** The money column worth drawing, in order of preference. */
const PREFERRED = ["net", "total", "amount", "closing", "balance"];

/**
 * A report that is one row per day (a "date" column and a money column) can be drawn as a trend.
 * Everything else (grouped or ledger reports) stays a table.
 */
export function reportSeries(table: Api.ReportTable): ReportSeries | null {
  const dateAt = table.columns.findIndex((c) => c.key === "date" && c.kind === "date");
  if (dateAt < 0 || table.rows.length < 3 || table.rows.length > 366) return null;
  for (const key of PREFERRED) {
    const at = table.columns.findIndex((c) => c.key === key && c.kind === "money");
    if (at < 0) continue;
    const byDate = new Map<string, number>();
    for (const row of table.rows) {
      const date = row[dateAt];
      const value = row[at];
      if (typeof date !== "string" || typeof value !== "number") return null;
      byDate.set(date, (byDate.get(date) ?? 0) + value);
    }
    // Several rows on one day (a day book) are added up; the days must be distinct to be a trend.
    if (byDate.size < 3) return null;
    return {
      label: table.columns[at]?.label ?? key,
      points: [...byDate.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([date, value]) => ({ date, value })),
    };
  }
  return null;
}
