import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import {
  ArrowDownRight,
  ArrowUpRight,
  Bell,
  CalendarCheck,
  HandCoins,
  PackagePlus,
  Plus,
  ShoppingCart,
  Truck,
} from "lucide-react";
import { businessDate } from "@inventory/shared";
import type { Api } from "@inventory/shared";
import { Avatar } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusBadge, StockBadge } from "@/components/app/badges";
import { TrendChart } from "@/components/app/charts";
import { ProductImage } from "@/components/app/product-image";
import { EmptyState, ErrorState, Page, QueryView, Section } from "@/components/app/page";
import { useCompactMoney, useLocale, useMoney, usePublicSettings } from "@/lib/business";
import { showDateTime, showQty } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useCan, useSession } from "@/stores/session";
import { changeFromYesterday, columnTotal, lastDays, salesByDay, topItems } from "./metrics";

interface QueryState<T> {
  data: T | undefined;
  error: unknown;
  isPending: boolean;
}

function Kpi({
  label,
  to,
  icon,
  query,
  value,
  hint,
}: {
  label: string;
  to?: string;
  icon: ReactNode;
  query: QueryState<unknown>;
  value: string;
  hint?: ReactNode;
}) {
  const body = (
    <div className="flex h-full flex-col gap-1.5 rounded-xl border border-border bg-card p-4 shadow-xs transition-colors group-hover:border-ring/40">
      <div className="flex items-center justify-between text-caption">
        <span>{label}</span>
        <span className="text-muted-foreground [&>svg]:size-4">{icon}</span>
      </div>
      {query.isPending ? (
        <Skeleton className="h-7 w-24" />
      ) : query.error ? (
        <p className="text-2xl font-semibold text-muted-foreground">—</p>
      ) : (
        <p className="num text-2xl font-semibold tracking-tight">{value}</p>
      )}
      <div className="min-h-4 text-caption">{!query.isPending && !query.error && hint}</div>
    </div>
  );
  return to ? (
    <Link
      to={to}
      className="group block rounded-xl outline-none focus-visible:ring-[3px] focus-visible:ring-ring/40"
    >
      {body}
    </Link>
  ) : (
    body
  );
}

function Delta({ change }: { change: number | null }) {
  if (change === null) return <span>No sales yesterday to compare</span>;
  const up = change >= 0;
  const Icon = up ? ArrowUpRight : ArrowDownRight;
  return (
    <span className="inline-flex items-center gap-1">
      <span
        className={cn("inline-flex items-center font-medium", up ? "text-success" : "text-danger")}
      >
        <Icon className="size-3.5" aria-hidden />
        {Math.abs(Math.round(change * 100))}%
      </span>
      vs yesterday
    </span>
  );
}

function QuickAction({ to, icon, children }: { to: string; icon: ReactNode; children: ReactNode }) {
  return (
    <Button variant="outline" size="sm" render={<Link to={to} />} nativeButton={false}>
      {icon}
      {children}
    </Button>
  );
}

function reportInput(report: string, range?: { from: string; to: string }, partyType?: string) {
  return {
    report,
    from: range?.from ?? null,
    to: range?.to ?? null,
    partyType: partyType ?? null,
  };
}

/** Today at a glance: the numbers, the trend, what to do next. Built from existing reports. */
export default function Dashboard() {
  const user = useSession((s) => s.user);
  const settings = usePublicSettings();
  const money = useMoney();
  const locale = useLocale();
  const canSales = useCan("sales.view");
  const canReports = useCan("reports.view");
  const canProducts = useCan("products.view");
  const canNewProduct = useCan("products.manage");
  const canBuy = useCan("purchases.manage");
  const canReceive = useCan("payments.receive");
  const canClose = useCan("day_close.perform");
  const canSell = useCan("sales.create");
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const days = lastDays(today, 14);
  const month = lastDays(today, 30);
  const from14 = days[0] ?? today;
  const from30 = month[0] ?? today;

  const trend = useCommand(
    "reports_run",
    reportInput("sales_summary", { from: from14, to: today }),
    { enabled: canReports },
  );
  const purchasesToday = useCommand(
    "reports_run",
    reportInput("purchases", { from: today, to: today }),
    { enabled: canReports },
  );
  const receivables = useCommand(
    "reports_run",
    reportInput("party_balances", undefined, "customer"),
    {
      enabled: canReports,
    },
  );
  const payables = useCommand("reports_run", reportInput("party_balances", undefined, "supplier"), {
    enabled: canReports,
  });
  const topSellers = useCommand(
    "reports_run",
    reportInput("sales_by_item", { from: from30, to: today }),
    { enabled: canReports },
  );
  const lowStock = useCommand(
    "products_list",
    { lowStockOnly: true, page: 1, pageSize: 6 },
    { enabled: canProducts },
  );
  const invoices = useCommand(
    "sales_list_invoices",
    { from: today, to: today, page: 1, pageSize: 6 },
    { enabled: canSales },
  );
  const notices = useCommand("notifications_list", {});
  const markRead = useAction("notifications_mark_read", { invalidates: ["notifications_list"] });

  const points = salesByDay(trend.data, days);
  const todayPoint = points.at(-1);
  const compact = useCompactMoney();
  const dayLabel = (iso: string) =>
    new Date(`${iso}T00:00:00Z`).toLocaleDateString(locale.locale, {
      day: "numeric",
      month: "short",
      timeZone: "UTC",
    });
  const weekTotal = points.reduce((sum, p) => sum + p.net, 0);

  return (
    <Page
      title={`Hello, ${user?.name.split(" ")[0] ?? ""}`}
      description={`${settings.data?.business.name ?? ""} · ${today}`}
      actions={
        <>
          {canNewProduct && (
            <QuickAction to="/inventory/products/new" icon={<PackagePlus />}>
              New product
            </QuickAction>
          )}
          {canBuy && (
            <QuickAction to="/purchases/bills/new" icon={<Truck />}>
              Purchase bill
            </QuickAction>
          )}
          {canReceive && (
            <QuickAction to="/payments" icon={<HandCoins />}>
              Record payment
            </QuickAction>
          )}
          {canClose && (
            <QuickAction to="/accounting/day-close" icon={<CalendarCheck />}>
              Close the day
            </QuickAction>
          )}
          {canSell && (
            <Button render={<Link to="/pos" />} nativeButton={false}>
              <ShoppingCart /> Open the till
            </Button>
          )}
        </>
      }
    >
      {canReports && (
        <div className="grid grid-cols-2 gap-3 md:grid-cols-3 2xl:grid-cols-6">
          <Kpi
            label="Sales today"
            to="/reports/sales-summary"
            icon={<ShoppingCart />}
            query={trend}
            value={money(todayPoint?.net ?? 0)}
            hint={<Delta change={changeFromYesterday(points)} />}
          />
          <Kpi
            label="Bills today"
            to="/sales/invoices"
            icon={<Plus />}
            query={trend}
            value={String(todayPoint?.bills ?? 0)}
            hint="Across every counter"
          />
          <Kpi
            label="Purchases today"
            to="/purchases"
            icon={<Truck />}
            query={purchasesToday}
            value={money(columnTotal(purchasesToday.data, "total"))}
            hint="Bills received"
          />
          <Kpi
            label="Customers owe you"
            to="/reports/receivables"
            icon={<ArrowDownRight />}
            query={receivables}
            value={money(columnTotal(receivables.data, "balance"))}
            hint="Receivables"
          />
          <Kpi
            label="You owe suppliers"
            to="/reports/payables"
            icon={<ArrowUpRight />}
            query={payables}
            value={money(columnTotal(payables.data, "balance"))}
            hint="Payables"
          />
          <Kpi
            label="Low stock"
            to="/inventory/products"
            icon={<Bell />}
            query={lowStock}
            value={String(lowStock.data?.total ?? 0)}
            hint={(lowStock.data?.total ?? 0) > 0 ? "Items to reorder" : "Everything is stocked"}
          />
        </div>
      )}

      {canReports && (
        <div className="grid gap-4 lg:grid-cols-3">
          <Section
            title="Sales, last 14 days"
            className="lg:col-span-2"
            actions={
              <span className="text-caption">
                <span className="num font-medium text-foreground">{money(weekTotal)}</span> net
              </span>
            }
          >
            {trend.error ? (
              <ErrorState error={trend.error} onRetry={() => void trend.refetch()} />
            ) : trend.isPending ? (
              <Skeleton className="h-[220px] w-full" />
            ) : (
              <TrendChart
                data={points.map((p) => ({ label: dayLabel(p.date), value: p.net }))}
                format={money}
                axisFormat={compact}
                summary={`Net sales per day for the last 14 days, ${money(weekTotal)} in total`}
              />
            )}
          </Section>
          <Section title="Top sellers, 30 days">
            <QueryView query={topSellers}>
              {(table) => {
                const items = topItems(table, 5);
                if (items.length === 0) {
                  return <p className="text-sm text-muted-foreground">No sales in this period.</p>;
                }
                const best = items[0]?.revenue || 1;
                return (
                  <ol className="flex flex-col gap-3">
                    {items.map((i, n) => (
                      <li key={`${i.item}-${n}`} className="flex flex-col gap-1">
                        <div className="flex items-center justify-between gap-3 text-sm">
                          <span className="flex min-w-0 items-center gap-2">
                            <Avatar name={i.item} size="sm" />
                            <span className="truncate font-medium">{i.item}</span>
                          </span>
                          <span className="shrink-0 num text-table">{money(i.revenue)}</span>
                        </div>
                        <div className="h-1 overflow-hidden rounded-full bg-muted" aria-hidden>
                          <div
                            className="h-full rounded-full bg-chart-1"
                            style={{ width: `${Math.max(4, (i.revenue / best) * 100)}%` }}
                          />
                        </div>
                      </li>
                    ))}
                  </ol>
                );
              }}
            </QueryView>
          </Section>
        </div>
      )}

      <div className="grid gap-4 lg:grid-cols-3">
        {canSales && (
          <Section
            title="Today's bills"
            className="lg:col-span-2"
            actions={
              <Button
                variant="ghost"
                size="sm"
                render={<Link to="/sales/invoices" />}
                nativeButton={false}
              >
                All invoices
              </Button>
            }
          >
            <QueryView query={invoices}>
              {(page) =>
                page.items.length === 0 ? (
                  <EmptyState
                    title="No bills yet today"
                    description="Sales from every counter appear here."
                  />
                ) : (
                  <div className="flex flex-col">
                    {page.items.map((i) => (
                      <Link
                        key={i.id}
                        to={`/sales/invoices/${i.id}`}
                        className="flex items-center justify-between gap-3 border-b border-border/60 py-2.5 text-sm last:border-0 hover:bg-muted/50"
                      >
                        <span className="w-36 font-medium">{i.docNo}</span>
                        <span className="min-w-0 flex-1 truncate text-muted-foreground">
                          {i.customerName}
                        </span>
                        <StatusBadge status={i.status === "voided" ? "voided" : i.paymentStatus} />
                        <span className="w-28 text-right num font-medium">{money(i.total)}</span>
                      </Link>
                    ))}
                    {page.total > page.items.length && (
                      <p className="pt-2 text-caption">{page.total} bills so far today</p>
                    )}
                  </div>
                )
              }
            </QueryView>
          </Section>
        )}

        <div className="flex flex-col gap-4">
          {canProducts && (
            <Section
              title="Low stock"
              actions={
                <Button
                  variant="ghost"
                  size="sm"
                  render={<Link to="/inventory/products" />}
                  nativeButton={false}
                >
                  Products
                </Button>
              }
            >
              <QueryView query={lowStock}>
                {(page) =>
                  page.items.length === 0 ? (
                    <p className="text-sm text-muted-foreground">Nothing is running low.</p>
                  ) : (
                    <ul className="flex flex-col gap-2.5">
                      {page.items.map((p) => (
                        <LowStockRow key={p.variantId} item={p} />
                      ))}
                    </ul>
                  )
                }
              </QueryView>
            </Section>
          )}
          <Section title="Notices">
            <QueryView query={notices}>
              {(list) =>
                list.length === 0 ? (
                  <p className="text-sm text-muted-foreground">Nothing needs your attention.</p>
                ) : (
                  <ul className="flex flex-col gap-2">
                    {list.map((n) => (
                      <li
                        key={n.id}
                        className="flex items-start gap-2 rounded-lg border border-border p-2.5 text-sm"
                      >
                        <Bell
                          className={cn(
                            "mt-0.5 size-4 shrink-0",
                            n.severity === "error" ? "text-danger" : "text-warning",
                          )}
                          aria-hidden
                        />
                        <div className="min-w-0 flex-1">
                          <p className="font-medium">{n.title}</p>
                          {n.body && <p className="text-muted-foreground">{n.body}</p>}
                          <p className="text-caption">{showDateTime(n.createdAt)}</p>
                        </div>
                        <Button
                          variant="ghost"
                          size="xs"
                          onClick={() => markRead.mutate({ input: { id: n.id } })}
                        >
                          Dismiss
                        </Button>
                      </li>
                    ))}
                  </ul>
                )
              }
            </QueryView>
          </Section>
        </div>
      </div>
    </Page>
  );
}

function LowStockRow({ item }: { item: Api.ProductListItem }) {
  return (
    <li>
      <Link
        to={`/inventory/products/${item.productId}`}
        className="flex items-center justify-between gap-3 rounded-md text-sm hover:bg-muted/50"
      >
        <span className="flex min-w-0 items-center gap-2">
          <ProductImage
            productId={item.productId}
            imageKey={item.imageKey}
            name={item.name}
            size="sm"
          />
          <span className="min-w-0">
            <span className="block truncate font-medium">{item.name}</span>
            <span className="num text-caption">
              {showQty(item.stock)} {item.unitCode} left
            </span>
          </span>
        </span>
        <StockBadge stock={item.stock} reorderLevel={item.reorderLevel} />
      </Link>
    </li>
  );
}
