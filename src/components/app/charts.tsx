import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

export interface TrendPoint {
  /** The text on the axis and in the tooltip title. */
  label: string;
  value: number;
}

/**
 * A sales-style trend: one smooth area in the brand chart colour. Colours come from the design
 * tokens (CSS variables), so it follows the light and dark themes. The summary is read out to
 * screen readers in place of the picture.
 */
export function TrendChart({
  data,
  format,
  axisFormat,
  summary,
  height = 220,
}: {
  data: TrendPoint[];
  /** Full value for the tooltip ("₹12,450.00"). */
  format: (value: number) => string;
  /** Short value for the axis ("12.5K"). */
  axisFormat: (value: number) => string;
  summary: string;
  height?: number;
}) {
  return (
    <div role="img" aria-label={summary} style={{ height }} className="w-full">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <defs>
            <linearGradient id="trend-fill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--chart-1)" stopOpacity={0.28} />
              <stop offset="100%" stopColor="var(--chart-1)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid vertical={false} stroke="var(--border)" strokeDasharray="3 3" />
          <XAxis
            dataKey="label"
            tickLine={false}
            axisLine={false}
            tickMargin={8}
            minTickGap={24}
            tick={{ fill: "var(--muted-foreground)", fontSize: 11 }}
          />
          <YAxis
            width={44}
            tickLine={false}
            axisLine={false}
            tickFormatter={axisFormat}
            tick={{ fill: "var(--muted-foreground)", fontSize: 11 }}
          />
          <Tooltip
            cursor={{ stroke: "var(--border)" }}
            content={({ active, payload, label }) => {
              const point = payload[0];
              if (!active || typeof point?.value !== "number") return null;
              return (
                <div className="rounded-lg border border-border bg-popover px-3 py-2 text-xs shadow-md">
                  <p className="text-muted-foreground">{String(label)}</p>
                  <p className="num text-sm font-semibold text-foreground">{format(point.value)}</p>
                </div>
              );
            }}
          />
          <Area
            type="monotone"
            dataKey="value"
            stroke="var(--chart-1)"
            strokeWidth={2}
            fill="url(#trend-fill)"
            activeDot={{ r: 4, fill: "var(--chart-1)", stroke: "var(--card)", strokeWidth: 2 }}
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
