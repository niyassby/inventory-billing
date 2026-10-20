export type RangePreset = "today" | "yesterday" | "week" | "month" | "last_month" | "fy" | "year";

export const PRESETS: [RangePreset, string][] = [
  ["today", "Today"],
  ["yesterday", "Yesterday"],
  ["week", "Last 7 days"],
  ["month", "This month"],
  ["last_month", "Last month"],
  ["fy", "This financial year"],
  ["year", "This calendar year"],
];

function parts(iso: string): [number, number, number] {
  const [y = 1970, m = 1, d = 1] = iso.split("-").map(Number);
  return [y, m, d];
}

function iso(y: number, m: number, d: number): string {
  return new Date(Date.UTC(y, m - 1, d)).toISOString().slice(0, 10);
}

/** First and last day of a preset, relative to `today` (YYYY-MM-DD in the business time zone). */
export function presetRange(
  preset: RangePreset,
  today: string,
  fyStartMonth = 4,
): { from: string; to: string } {
  const [y, m, d] = parts(today);
  switch (preset) {
    case "today":
      return { from: today, to: today };
    case "yesterday": {
      const day = iso(y, m, d - 1);
      return { from: day, to: day };
    }
    case "week":
      return { from: iso(y, m, d - 6), to: today };
    case "month":
      return { from: iso(y, m, 1), to: today };
    case "last_month":
      return { from: iso(y, m - 1, 1), to: iso(y, m, 0) };
    case "fy": {
      const startYear = m >= fyStartMonth ? y : y - 1;
      return { from: iso(startYear, fyStartMonth, 1), to: today };
    }
    case "year":
      return { from: iso(y, 1, 1), to: today };
  }
}
