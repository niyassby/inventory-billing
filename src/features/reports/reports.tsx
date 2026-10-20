import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { Api } from "@inventory/shared";
import { businessDate } from "@inventory/shared";
import { Download, FileSpreadsheet, Printer } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Page, QueryView, Section } from "@/components/app/page";
import { WarehouseSelect } from "@/components/app/warehouse-select";
import { localeOf, usePublicSettings } from "@/lib/business";
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
        about: "Bills, tax, returns and net sales by day.",
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
        about: "Quantity, average cost and value of stock on hand.",
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

export function ReportsIndex() {
  return (
    <Page
      title="Reports"
      description="Pick a report, choose the period, then view, print or export it."
    >
      <div className="grid gap-6">
        {CATALOG.map((g) => (
          <section key={g.group}>
            <h2 className="mb-2 font-heading text-sm font-semibold text-muted-foreground">
              {g.group}
            </h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {g.entries.map((e) => (
                <Link
                  key={e.id}
                  to={`/reports/${e.id}`}
                  className="rounded-2xl border border-border/60 bg-card p-4 transition-colors hover:bg-muted/50 focus-visible:bg-muted focus-visible:outline-none"
                >
                  <p className="font-medium">{e.title}</p>
                  <p className="mt-1 text-sm text-muted-foreground">{e.about}</p>
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

export function ReportTableView({ table }: { table: Api.ReportTable }) {
  const settings = usePublicSettings();
  const locale = localeOf(settings.data);
  const total = table.totals;
  return (
    <div className="overflow-auto rounded-2xl border border-border/60 bg-card">
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-card">
          <tr className="border-b border-border/60">
            {table.columns.map((c) => (
              <th
                key={c.key}
                scope="col"
                className={cn(
                  "h-10 px-3 text-xs font-medium whitespace-nowrap text-muted-foreground",
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
            return (
              <tr
                key={i}
                className={cn("border-b border-border/40", bold && "bg-muted/40 font-semibold")}
              >
                {table.columns.map((c, j) => (
                  <td
                    key={c.key}
                    className={cn(
                      "px-3 py-1.5 whitespace-nowrap",
                      NUMERIC.has(c.kind) ? "text-right tabular-nums" : "",
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
            <tr className="border-t-2 border-border font-semibold">
              {table.columns.map((c, j) => (
                <td
                  key={c.key}
                  className={cn(
                    "px-3 py-2 whitespace-nowrap",
                    NUMERIC.has(c.kind) ? "text-right tabular-nums" : "",
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

export function ReportPage() {
  const { name = "" } = useParams();
  const entry = ALL.find((e) => e.id === name);
  const settings = usePublicSettings();
  const locale = localeOf(settings.data);
  const canExport = useCan("reports.export");
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const fyStart = settings.data?.business.fyStartMonth ?? 4;
  const [range, setRange] = useState(() => presetRange("month", today, fyStart));
  const [preset, setPreset] = useState<RangePreset | "custom">("month");
  const [warehouseId, setWarehouseId] = useState("");
  const [accountId, setAccountId] = useState("");
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
  const choose = (p: RangePreset) => {
    setPreset(p);
    setRange(presetRange(p, today, fyStart));
  };
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
              <NativeSelect
                aria-label="Period"
                value={preset}
                onChange={(e) =>
                  e.target.value !== "custom" && choose(e.target.value as RangePreset)
                }
              >
                {PRESETS.map(([value, text]) => (
                  <NativeSelectOption key={value} value={value}>
                    {text}
                  </NativeSelectOption>
                ))}
                <NativeSelectOption value="custom">Custom dates</NativeSelectOption>
              </NativeSelect>
              <Input
                type="date"
                aria-label="From"
                className="w-40"
                value={range.from}
                onChange={(e) => {
                  setPreset("custom");
                  setRange((r) => ({ ...r, from: e.target.value }));
                }}
              />
              <Input
                type="date"
                aria-label="To"
                className="w-40"
                value={range.to}
                onChange={(e) => {
                  setPreset("custom");
                  setRange((r) => ({ ...r, to: e.target.value }));
                }}
              />
            </>
          ) : (
            entry.report === "trial_balance" && (
              <Input
                type="date"
                aria-label="As of"
                className="w-40"
                value={range.to}
                onChange={(e) => setRange((r) => ({ ...r, to: e.target.value }))}
              />
            )
          )}
          {stockReport && (
            <div className="w-48">
              <WarehouseSelect
                value={warehouseId}
                onChange={setWarehouseId}
                label="Store"
                allowDefault={false}
                emptyLabel="All stores"
              />
            </div>
          )}
          {bookReport && (
            <NativeSelect
              aria-label="Account"
              value={accountId}
              onChange={(e) => setAccountId(e.target.value)}
            >
              <NativeSelectOption value="">All {subtype} accounts</NativeSelectOption>
              {accounts.data
                ?.filter((a) => a.subtype === subtype && a.isActive)
                .map((a) => (
                  <NativeSelectOption key={a.id} value={a.id}>
                    {a.name}
                  </NativeSelectOption>
                ))}
            </NativeSelect>
          )}
        </div>
      </Section>
      <QueryView query={query}>
        {(table) => (
          <>
            <ReportTableView table={table} />
            {table.truncated && (
              <p className="text-sm text-amber-600">
                Only the first rows are shown. Narrow the period, or export for the full list.
              </p>
            )}
          </>
        )}
      </QueryView>
    </Page>
  );
}
