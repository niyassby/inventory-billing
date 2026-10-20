import { Link } from "react-router-dom";
import { Bell } from "lucide-react";
import { businessDate } from "@inventory/shared";
import { Button } from "@/components/ui/button";
import { Page, Section, QueryView, EmptyState } from "@/components/app/page";
import { StatusBadge } from "@/components/app/badges";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showDateTime } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { useCan, useSession } from "@/stores/session";

/** Today at a glance. Charts and the full dashboard come with the reports (Phases 6–7). */
export default function Dashboard() {
  const user = useSession((s) => s.user);
  const settings = usePublicSettings();
  const money = useMoney();
  const canSales = useCan("sales.view");
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const invoices = useCommand(
    "sales_list_invoices",
    { from: today, to: today, page: 1, pageSize: 8 },
    { enabled: canSales },
  );
  const notices = useCommand("notifications_list", {});
  const markRead = useAction("notifications_mark_read", { invalidates: ["notifications_list"] });
  return (
    <Page
      title={`Hello, ${user?.name ?? ""}`}
      description={`${settings.data?.business.name ?? ""} · ${today}`}
      actions={
        <Button render={<Link to="/pos" />} nativeButton={false}>
          Open the till
        </Button>
      }
    >
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
                    <p className="mb-2 text-sm text-muted-foreground">
                      {page.total} bills so far today
                    </p>
                    {page.items.map((i) => (
                      <Link
                        key={i.id}
                        to={`/sales/invoices/${i.id}`}
                        className="flex items-center justify-between gap-3 border-b border-border/40 py-2 text-sm last:border-0 hover:bg-muted/40"
                      >
                        <span className="font-medium">{i.docNo}</span>
                        <span className="min-w-0 flex-1 truncate text-muted-foreground">
                          {i.customerName}
                        </span>
                        <StatusBadge status={i.status === "voided" ? "voided" : i.paymentStatus} />
                        <span className="w-28 text-right tabular-nums">{money(i.total)}</span>
                      </Link>
                    ))}
                  </div>
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
                      className="flex items-start gap-2 rounded-xl border border-border/60 p-2 text-sm"
                    >
                      <Bell
                        className={
                          n.severity === "error"
                            ? "mt-0.5 size-4 text-destructive"
                            : "mt-0.5 size-4 text-warning"
                        }
                        aria-hidden
                      />
                      <div className="min-w-0 flex-1">
                        <p className="font-medium">{n.title}</p>
                        {n.body && <p className="text-muted-foreground">{n.body}</p>}
                        <p className="text-xs text-muted-foreground">{showDateTime(n.createdAt)}</p>
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
    </Page>
  );
}
