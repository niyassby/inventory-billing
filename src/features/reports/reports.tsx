import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import type { Api } from "@inventory/shared";
import { businessDate } from "@inventory/shared";
import {
  Boxes,
  ChartColumn,
  Download,
  FileSpreadsheet,
  Landmark,
  Printer,
  Truck,
  Users,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { TrendChart } from "@/components/app/charts";
import { Page, QueryView, Section } from "@/components/app/page";
import { WarehouseSelect } from "@/components/app/warehouse-select";
import { localeOf, useCompactMoney, useMoney, usePublicSettings } from "@/lib/business";
import {
  cellText,
  downloadText,
  downloadXlsx,
  fileName,
  tableToCsv,
  tableToHtml,
} from "@/lib/export";
import { useList, useCommand } from "@/lib/queries";
import { PRESETS, presetRange } from "@/lib/ranges";
import type { RangePreset } from "@/lib/ranges";
import { run } from "@/lib/run";
import { cn } from "@/lib/utils";
import { useCan } from "@/stores/session";
import { printHtml } from "@/features/sales/print";
import { reportSeries } from "./chart";
import { SavedViews } from "./saved-views";

interface Entry {
  id: string;
  report: string;
  title: string;
  about: string;
  /** Shows a single date ("as of") instead of a range. */
  asOf?: boolean;
  partyType?: "customer" | "supplier";
  permission: string;
}

const CATALOG: { group: string; entries: Entry[] }[] = [
  {
    group: "Sales",
    entries: [
      {
        id: "sales-summary",
        report: "sales_summary",
        title: "Sales summary",
        about:
          "Bills, tax, returns and net sales by day, month, customer, cashier, item or category.",
        permission: "reports.view",
      },
      {
        id: "sales-invoices",
        report: "sales_invoices",
        title: "Sales invoices",
        about: "Every invoice of the period, by customer, cashier or item; open any bill.",
        permission: "reports.view",
      },
      {
        id: "item-profit",
        report: "sales_by_item",
        title: "Item profit",
        about: "What each item sold for, cost and margin.",
        permission: "reports.view",
      },
    ],
  },
  {
    group: "Purchases and expenses",
    entries: [
      {
        id: "purchases",
        report: "purchases",
        title: "Purchases",
        about: "Supplier bills with tax and what was paid.",
        permission: "reports.view",
      },
      {
        id: "expenses",
        report: "expenses",
        title: "Expenses",
        about: "Spending by category.",
        permission: "reports.view",
      },
    ],
  },
  {
    group: "Stock",
    entries: [
      {
        id: "stock-valuation",
        report: "stock_valuation",
        title: "Stock valuation",
        about: "Quantity, unit cost and value of stock on hand (average cost or FIFO).",
        asOf: true,
        permission: "reports.view",
      },
      {
        id: "stock-movement",
        report: "stock_movement",
        title: "Stock movement",
        about: "Opening, in, out and closing quantity per item.",
        permission: "reports.view",
      },
      {
        id: "expiry",
        report: "expiry",
        title: "Expired and near-expiry stock",
        about: "Batches past or near their expiry date, with their value.",
        asOf: true,
        permission: "reports.view",
      },
    ],
  },
  {
    group: "Customers and suppliers",
    entries: [
      {
        id: "receivables",
        report: "party_balances",
        partyType: "customer",
        title: "Receivables",
        about: "Who owes you money.",
        asOf: true,
        permission: "reports.view",
      },
      {
        id: "payables",
        report: "party_balances",
        partyType: "supplier",
        title: "Payables",
        about: "Who you owe money to.",
        asOf: true,
        permission: "reports.view",
      },
    ],
  },
  {
    group: "Tax and accounts",
    entries: [
      {
        id: "tax-summary",
        report: "tax_summary",
        title: "Tax summary",
        about: "Tax on sales and purchases by rate, and the net payable.",
        permission: "reports.view",
      },
      {
        id: "profit-loss",
        report: "profit_loss",
        title: "Profit and loss",
        about: "Income, cost of goods and expenses.",
        permission: "reports.view",
      },
      {
        id: "trial-balance",
        report: "trial_balance",
        title: "Trial balance",
        about: "Every account's debit or credit balance.",
        asOf: true,
        permission: "reports.view",
      },
      {
        id: "day-book",
        report: "day_book",
        title: "Day book",
        about: "Every posting of the period, in order.",
        permission: "reports.view",
      },
      {
        id: "cash-book",
        report: "cash_book",
        title: "Cash book",
        about: "Cash received and paid, with a running balance.",
        permission: "reports.view",
      },
      {
        id: "bank-book",
        report: "bank_book",
        title: "Bank book",
        about: "Bank receipts and payments, with a running balance.",
        permission: "reports.view",
      },
    ],
  },
];

const ALL = CATALOG.flatMap((g) => g.entries);

const GROUP_ICON: Record<string, LucideIcon> = {
  Sales: ChartColumn,
  "Purchases and expenses": Truck,
  Stock: Boxes,
  "Customers and suppliers": Users,
  "Tax and accounts": Landmark,
};

export function ReportsIndex() {
  return (
    <Page
      title="Reports"
      description="Pick a report, choose the period, then view, print or export it."
    >
      <div className="grid gap-6">
        {CATALOG.map((g) => (
          <section key={g.group}>
            <h2 className="mb-2 font-heading text-section-title">{g.group}</h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {g.entries.map((e) => (
                <Link
                  key={e.id}
                  to={`/reports/${e.id}`}
                  className="group flex items-start gap-3 rounded-xl border border-border bg-card p-4 shadow-xs transition-[border-color,box-shadow] hover:border-ring/50 hover:shadow-sm focus-visible:border-ring focus-visible:outline-none"
                >
                  <span className="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary-soft text-primary">
                    {(() => {
                      const Icon = GROUP_ICON[g.group] ?? ChartColumn;
                      return <Icon className="size-4" aria-hidden />;
                    })()}
                  </span>
                  <span className="min-w-0">
                    <span className="block font-medium group-hover:text-primary">{e.title}</span>
                    <span className="mt-0.5 block text-sm text-muted-foreground">{e.about}</span>
                  </span>
                </Link>
              ))}
            </div>
          </section>
        ))}
      </div>
    </Page>
  );
}

const NUMERIC = new Set(["money", "qty", "rate", "percent", "count"]);

export function ReportTableView({
  table,
  onRow,
}: {
  table: Api.ReportTable;
  /** A row was chosen (only rows with a link). */
  onRow?: (index: number) => void;
}) {
  const settings = usePublicSettings();
  const locale = localeOf(settings.data);
  const total = table.totals;
  return (
    <div className="overflow-auto rounded-xl border border-border bg-card shadow-xs">
      <table className="w-full text-table">
        <thead className="sticky top-0 bg-muted/80 backdrop-blur-sm">
          <tr className="border-b border-border">
            {table.columns.map((c) => (
              <th
                key={c.key}
                scope="col"
                className={cn(
                  "h-9 px-3 text-xs font-medium whitespace-nowrap text-muted-foreground",
                  NUMERIC.has(c.kind) ? "text-right" : "text-left",
                )}
              >
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {table.rows.length === 0 && (
            <tr>
              <td colSpan={table.columns.length} className="p-8 text-center text-muted-foreground">
                Nothing in this period.
              </td>
            </tr>
          )}
          {table.rows.map((row, i) => {
            // Subtotal lines of the profit and loss are shown in bold.
            const bold = table.report === "profit_loss" && row[1] === "";
            const linked = Boolean(onRow && table.links?.[i]);
            return (
              <tr
                key={i}
                className={cn(
                  "border-b border-border/60",
                  bold && "bg-muted/40 font-semibold",
                  linked &&
                    "cursor-pointer hover:bg-muted/50 focus-visible:bg-muted/50 focus-visible:outline-none",
                )}
                {...(linked
                  ? {
                      tabIndex: 0,
                      title: "Open",
                      onClick: () => onRow?.(i),
                      onKeyDown: (e: React.KeyboardEvent) => {
                        if (e.key === "Enter") onRow?.(i);
                      },
                    }
                  : {})}
              >
                {table.columns.map((c, j) => (
                  <td
                    key={c.key}
                    className={cn(
                      "px-3 py-1.5 whitespace-nowrap",
                      NUMERIC.has(c.kind) ? "text-right num" : "",
                    )}
                  >
                    {cellText(c, row[j], locale)}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
        {total && (
          <tfoot>
            <tr className="border-t-2 border-border bg-muted/40 font-semibold">
              {table.columns.map((c, j) => (
                <td
                  key={c.key}
                  className={cn(
                    "px-3 py-2 whitespace-nowrap",
                    NUMERIC.has(c.kind) ? "text-right num" : "",
                  )}
                >
                  {cellText(c, total[j], locale)}
                </td>
              ))}
            </tr>
          </tfoot>
        )}
      </table>
    </div>
  );
}

function ReportChart({ table }: { table: Api.ReportTable }) {
  const money = useMoney();
  const compact = useCompactMoney();
  const series = reportSeries(table);
  if (!series) return null;
  const total = series.points.reduce((n, p) => n + p.value, 0);
  const dayLabel = (iso: string) =>
    new Date(`${iso}T00:00:00Z`).toLocaleDateString(undefined, {
      day: "numeric",
      month: "short",
      timeZone: "UTC",
    });
  return (
    <Section
      title={`${series.label} by day`}
      actions={<span className="num text-caption">{money(total)} in this period</span>}
    >
      <TrendChart
        data={series.points.map((p) => ({ label: dayLabel(p.date), value: p.value }))}
        format={money}
        axisFormat={compact}
        summary={`${series.label} per day, ${money(total)} in total`}
        height={200}
      />
    </Section>
  );
}

/** Where a document link of a report row opens. */
function documentPath(link: Api.RowLink): string | null {
  const id = link.id ?? "";
  switch (link.kind) {
    case "sales_invoice":
      return `/sales/invoices/${id}`;
    case "sales_return":
      return `/sales/returns/${id}`;
    case "purchase_bill":
      return `/purchases/bills/${id}`;
    case "goods_receipt":
      return `/purchases/receipts/${id}`;
    case "purchase_return":
      return `/purchases/returns/${id}`;
    case "manual_journal":
      return `/accounting/journals?open=${id}`;
    default:
      return null;
  }
}

const FILTERS = ["partyId", "userId", "categoryId", "variantId"] as const;
const FILTER_LABEL: Record<(typeof FILTERS)[number], string> = {
  partyId: "One customer or supplier",
  userId: "One cashier",
  categoryId: "One category",
  variantId: "One item",
};

/** Where a row leads: another report (its id in the catalogue, with the parameters) or a document. */
function linkPath(link: Api.RowLink): string | null {
  if (link.kind !== "report") return documentPath(link);
  const target = ALL.find((e) => e.report === link.report && !e.partyType);
  if (!target) return null;
  const params = new URLSearchParams();
  if (link.from) params.set("from", link.from);
  if (link.to) params.set("to", link.to);
  if (link.groupBy) params.set("group", link.groupBy);
  for (const f of FILTERS) {
    const v = link[f];
    if (v) params.set(f, v);
  }
  return `/reports/${target.id}?${params.toString()}`;
}

export function ReportPage() {
  const { name = "" } = useParams();
  const navigate = useNavigate();
  const entry = ALL.find((e) => e.id === name);
  const settings = usePublicSettings();
  const locale = localeOf(settings.data);
  const canExport = useCan("reports.export");
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const fyStart = settings.data?.business.fyStartMonth ?? 4;
  // Everything the report shows is in the address: drill-downs, back and shared links work.
  const [params, setParams] = useSearchParams();
  const preset = (params.get("period") ?? (params.get("from") ? "custom" : "month")) as
    RangePreset | "custom";
  const range =
    preset === "custom"
      ? { from: params.get("from") ?? today, to: params.get("to") ?? today }
      : presetRange(preset, today, fyStart);
  const setParam = (patch: Record<string, string | null>) =>
    setParams(
      (p) => {
        const next = new URLSearchParams(p);
        for (const [k, v] of Object.entries(patch)) {
          if (v) next.set(k, v);
          else next.delete(k);
        }
        return next;
      },
      { replace: true },
    );
  const warehouseId = params.get("warehouseId") ?? "";
  const accountId = params.get("accountId") ?? "";
  const group = params.get("group") ?? "";
  const accounts = useCommand(
    "accounts_list",
    {},
    { enabled: entry?.report === "cash_book" || entry?.report === "bank_book" },
  );
  const subtype = entry?.report === "cash_book" ? "cash" : "bank";
  const query = useList(
    "reports_run",
    {
      report: entry?.report ?? "sales_summary",
      from: range.from,
      to: range.to,
      warehouseId: warehouseId || null,
      partyType: entry?.partyType ?? null,
      accountId: accountId || null,
      groupBy: group || null,
      partyId: params.get("partyId"),
      userId: params.get("userId"),
      categoryId: params.get("categoryId"),
      variantId: params.get("variantId"),
    },
    { enabled: Boolean(entry) },
  );
  if (!entry) {
    return (
      <Page title="Report not found" back={{ to: "/reports", label: "Reports" }}>
        <p className="text-sm text-muted-foreground">There is no such report.</p>
      </Page>
    );
  }
  const stockReport = entry.report.startsWith("stock_");
  const bookReport = entry.report === "cash_book" || entry.report === "bank_book";
  const choose = (p: RangePreset) => setParam({ period: p, from: null, to: null });
  const current: Api.ViewParams = {
    period: preset,
    from: preset === "custom" ? range.from : null,
    to: preset === "custom" ? range.to : null,
    groupBy: group || null,
    warehouseId: warehouseId || null,
    partyType: entry.partyType ?? null,
    accountId: accountId || null,
    partyId: params.get("partyId"),
    userId: params.get("userId"),
    categoryId: params.get("categoryId"),
    variantId: params.get("variantId"),
  };
  const apply = (v: Api.ViewParams) => {
    const next = new URLSearchParams();
    if (v.period) next.set("period", v.period);
    if (v.period === "custom") {
      if (v.from) next.set("from", v.from);
      if (v.to) next.set("to", v.to);
    }
    if (v.groupBy) next.set("group", v.groupBy);
    if (v.warehouseId) next.set("warehouseId", v.warehouseId);
    if (v.accountId) next.set("accountId", v.accountId);
    for (const f of FILTERS) {
      const value = v[f];
      if (value) next.set(f, value);
    }
    setParams(next, { replace: true });
  };
  const activeFilters = FILTERS.filter((f) => params.get(f));
  return (
    <Page
      title={entry.title}
      description={entry.about}
      back={{ to: "/reports", label: "Reports" }}
      actions={
        query.data && (
          <>
            <Button
              variant="outline"
              onClick={() =>
                printHtml(tableToHtml(query.data, settings.data?.business.name ?? "", locale))
              }
            >
              <Printer /> Print / PDF
            </Button>
            {canExport && (
              <>
                <Button
                  variant="outline"
                  onClick={() =>
                    downloadText(
                      fileName(query.data, "csv"),
                      tableToCsv(query.data, locale.currency.minorDigits),
                      "text/csv;charset=utf-8",
                    )
                  }
                >
                  <Download /> CSV
                </Button>
                <Button
                  variant="outline"
                  onClick={run(() => downloadXlsx(query.data, locale.currency.minorDigits))}
                >
                  <FileSpreadsheet /> Excel
                </Button>
              </>
            )}
          </>
        )
      }
    >
      <Section>
        <div className="flex flex-wrap items-end gap-3">
          {!entry.asOf ? (
            <>
              <SelectField
                aria-label="Period"
                className="w-44"
                value={preset}
                onValueChange={(v) => v !== "custom" && choose(v as RangePreset)}
                options={[
                  ...PRESETS.map(([value, text]) => ({ value, label: text })),
                  { value: "custom", label: "Custom dates" },
                ]}
              />
              <DatePicker
                aria-label="From"
                className="w-40"
                value={range.from}
                onChange={(v) => setParam({ period: "custom", from: v, to: range.to })}
              />
              <DatePicker
                aria-label="To"
                className="w-40"
                value={range.to}
                onChange={(v) => setParam({ period: "custom", from: range.from, to: v })}
              />
            </>
          ) : (
            entry.report === "trial_balance" && (
              <DatePicker
                aria-label="As of"
                className="w-40"
                value={range.to}
                onChange={(v) => setParam({ period: "custom", from: range.from, to: v })}
              />
            )
          )}
          {(query.data?.groupOptions ?? []).length > 0 && (
            <SelectField
              aria-label="Group by"
              className="w-44"
              value={query.data?.groupBy ?? group}
              onValueChange={(v) => setParam({ group: v })}
              options={(query.data?.groupOptions ?? []).map((o) => ({
                value: o.key,
                label: `By ${o.label.toLowerCase()}`,
              }))}
            />
          )}
          {stockReport && (
            <div className="w-48">
              <WarehouseSelect
                value={warehouseId}
                onChange={(v) => setParam({ warehouseId: v })}
                label="Store"
                allowDefault={false}
                emptyLabel="All stores"
              />
            </div>
          )}
          {bookReport && (
            <SelectField
              aria-label="Account"
              value={accountId}
              onValueChange={(v) => setParam({ accountId: v })}
              options={[
                { value: "", label: `All ${subtype} accounts` },
                ...(accounts.data ?? [])
                  .filter((a) => a.subtype === subtype && a.isActive)
                  .map((a) => ({ value: a.id, label: a.name })),
              ]}
            />
          )}
          <div className="ml-auto">
            <SavedViews report={entry.report} current={current} onApply={apply} />
          </div>
        </div>
        {activeFilters.length > 0 && (
          <div
            className="mt-3 flex flex-wrap gap-2"
            aria-label="Filters from the report you came from"
          >
            {activeFilters.map((f) => (
              <Button
                key={f}
                size="xs"
                variant="secondary"
                onClick={() => setParam({ [f]: null })}
                aria-label={`Remove filter: ${FILTER_LABEL[f]}`}
              >
                {FILTER_LABEL[f]} ×
              </Button>
            ))}
          </div>
        )}
      </Section>
      <QueryView query={query}>
        {(table) => (
          <>
            <ReportChart table={table} />
            <ReportTableView
              table={table}
              onRow={(i) => {
                const link = table.links?.[i];
                const path = link ? linkPath(link) : null;
                if (path) void navigate(path);
              }}
            />
            {table.truncated && (
              <p className="text-sm text-warning">
                Only the first rows are shown. Narrow the period, or export for the full list.
              </p>
            )}
          </>
        )}
      </QueryView>
    </Page>
  );
}
