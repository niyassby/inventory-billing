import type { Api } from "@inventory/shared";

/**
 * Shapes report tables (the backend's one table format) into what the dashboard draws. There is
 * no dashboard endpoint: every figure comes from `reports_run` and the product list.
 */

/** The last `count` calendar days up to and including `today` (YYYY-MM-DD), oldest first. */
export function lastDays(today: string, count: number): string[] {
  const [y, m, d] = today.split("-").map(Number);
  const end = Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1);
  return Array.from({ length: count }, (_, i) => {
    const date = new Date(end - (count - 1 - i) * 86_400_000);
    return date.toISOString().slice(0, 10);
  });
}

function columnIndex(table: Api.ReportTable, key: string): number {
  return table.columns.findIndex((c) => c.key === key);
}

function asText(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function asNumber(value: unknown): number {
  return typeof value === "number" ? value : 0;
}

/** The total of a money or count column: the table's own totals row, or the sum of its rows. */
export function columnTotal(table: Api.ReportTable | undefined, key: string): number {
  if (!table) return 0;
  const index = columnIndex(table, key);
  if (index < 0) return 0;
  if (table.totals) return asNumber(table.totals[index]);
  return table.rows.reduce((sum, row) => sum + asNumber(row[index]), 0);
}

export interface DayPoint {
  date: string;
  net: number;
  bills: number;
}

/** Net sales and bill count for every day in `days`, zero where nothing was sold. */
export function salesByDay(table: Api.ReportTable | undefined, days: string[]): DayPoint[] {
  const dateAt = table ? columnIndex(table, "date") : -1;
  const netAt = table ? columnIndex(table, "net") : -1;
  const billsAt = table ? columnIndex(table, "bills") : -1;
  const byDate = new Map<string, DayPoint>();
  for (const row of table?.rows ?? []) {
    const date = row[dateAt];
    if (typeof date === "string") {
      byDate.set(date, { date, net: asNumber(row[netAt]), bills: asNumber(row[billsAt]) });
    }
  }
  return days.map((date) => byDate.get(date) ?? { date, net: 0, bills: 0 });
}

export interface TopItem {
  item: string;
  qty: number;
  revenue: number;
}

/** The items that earned the most, best first. */
export function topItems(table: Api.ReportTable | undefined, count: number): TopItem[] {
  if (!table) return [];
  const itemAt = columnIndex(table, "item");
  const qtyAt = columnIndex(table, "qty");
  const revenueAt = columnIndex(table, "revenue");
  return table.rows
    .map((row) => ({
      item: asText(row[itemAt]),
      qty: asNumber(row[qtyAt]),
      revenue: asNumber(row[revenueAt]),
    }))
    .sort((a, b) => b.revenue - a.revenue)
    .slice(0, count);
}

/** Today against yesterday as a fraction (0.12 = 12 % more); null when yesterday had no sales. */
export function changeFromYesterday(points: DayPoint[]): number | null {
  const today = points.at(-1);
  const yesterday = points.at(-2);
  if (!today || !yesterday || yesterday.net <= 0) return null;
  return (today.net - yesterday.net) / yesterday.net;
}
