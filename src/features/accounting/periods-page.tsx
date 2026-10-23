import { Fragment, useState } from "react";
import type { Api } from "@inventory/shared";
import { Link } from "react-router-dom";
import { Lock, LockOpen } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { StatusBadge } from "@/components/app/badges";
import { confirm } from "@/components/app/dialogs";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { useMoney } from "@/lib/business";
import { showDate, showDateTime } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { run } from "@/lib/run";

/** Locked days, reopened days and financial years. */
export default function PeriodsPage() {
  const money = useMoney();
  const status = useCommand("period_status", {});
  const years = useCommand("years_list", {});
  const closes = useCommand("day_closes_list", {});
  const [closing, setClosing] = useState<Api.YearRow | null>(null);
  const reopenDay = useAction("day_reopen", {
    invalidates: ["period_status", "day_closes_list", "day_summary"],
    success: "Day reopened",
  });
  const reopenYear = useAction("year_reopen", {
    invalidates: ["years_list", "period_status", "reports_run"],
    success: (y) => `${y.fyLabel} reopened`,
  });
  const canManage = status.data?.canManage ?? false;
  return (
    <Page
      title="Periods and year end"
      description="Which days are locked, days the owner reopened, and closing a financial year."
    >
      <QueryView query={status}>
        {(s) => (
          <Section
            title={
              <span className="flex items-center gap-2">
                {s.lockedThrough ? (
                  <Lock className="size-4" aria-hidden />
                ) : (
                  <LockOpen className="size-4" aria-hidden />
                )}
                Locked days
              </span>
            }
          >
            <Facts
              items={[
                ["Closed-day lock", s.lockClosedDays ? "On" : "Off"],
                ["Locked up to", s.lockedThrough ? showDate(s.lockedThrough) : "Nothing is locked"],
                [
                  "Reopened days",
                  s.reopenedDays.length
                    ? s.reopenedDays.map((d) => showDate(d)).join(", ")
                    : "None",
                ],
              ]}
            />
            <p className="mt-3 text-sm text-muted-foreground">
              {s.lockClosedDays
                ? "A bill, payment, expense, stock change or journal dated on or before the last closed day needs a manager's approval (period override) and is recorded in the audit log."
                : "Closing a day only records it. Turn on the lock in "}
              {!s.lockClosedDays && (
                <Link
                  className="text-primary underline underline-offset-4"
                  to="/settings/accounting"
                >
                  Settings → Accounting
                </Link>
              )}
              {!s.lockClosedDays &&
                " so closed days can't change without approval. A closed financial year is always locked."}
            </p>
          </Section>
        )}
      </QueryView>
      <Section title="Closed days">
        <QueryView query={closes}>
          {(list) =>
            list.length === 0 ? (
              <p className="text-sm text-muted-foreground">No day has been closed yet.</p>
            ) : (
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1 text-left font-medium">Day</th>
                    <th className="py-1 text-left font-medium">Status</th>
                    <th className="py-1 text-left font-medium">Closed by</th>
                    <th className="py-1 text-right font-medium">Sales</th>
                    <th className="py-1" />
                  </tr>
                </thead>
                <tbody>
                  {list.map((c) => (
                    <tr key={c.businessDate} className="border-t border-border/40">
                      <td className="py-1.5 font-medium">{showDate(c.businessDate)}</td>
                      <td>
                        <StatusBadge status={c.status} />
                        {c.reopenReason && (
                          <span className="ml-2 text-xs text-muted-foreground">
                            {c.reopenedBy ?? ""}: {c.reopenReason}
                          </span>
                        )}
                      </td>
                      <td className="text-muted-foreground">{c.closedBy}</td>
                      <td className="text-right tabular-nums">{money(c.salesTotal)}</td>
                      <td className="text-right">
                        {canManage && c.status === "closed" && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={run(async () => {
                              const reason = await confirm({
                                title: `Reopen ${showDate(c.businessDate)}?`,
                                description:
                                  "Documents can be dated on that day again until it is closed again (Day close).",
                                confirmLabel: "Reopen the day",
                                reason: { label: "Reason", required: true },
                              });
                              if (reason !== false)
                                reopenDay.mutate({
                                  input: { businessDate: c.businessDate, reason },
                                });
                            })}
                          >
                            Reopen
                          </Button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </QueryView>
      </Section>
      <Section title="Financial years">
        <QueryView query={years}>
          {(list) => (
            <table className="w-full text-sm">
              <thead className="text-xs text-muted-foreground">
                <tr>
                  <th className="py-1 text-left font-medium">Year</th>
                  <th className="py-1 text-left font-medium">Status</th>
                  <th className="py-1 text-right font-medium">Net profit</th>
                  <th className="py-1" />
                </tr>
              </thead>
              <tbody>
                {list.map((y) => (
                  <tr key={y.startDate} className="border-t border-border/40">
                    <td className="py-1.5">
                      <span className="font-medium">{y.fyLabel}</span>
                      <span className="ml-2 text-xs text-muted-foreground">
                        {showDate(y.startDate)} – {showDate(y.endDate)}
                      </span>
                    </td>
                    <td>
                      <StatusBadge status={y.status} />
                      {y.closedAt && y.status === "closed" && (
                        <span className="ml-2 text-xs text-muted-foreground">
                          {showDateTime(y.closedAt)} · {y.closedBy}
                        </span>
                      )}
                    </td>
                    <td className="text-right tabular-nums">{money(y.netProfit)}</td>
                    <td className="text-right">
                      {canManage && y.canClose && (
                        <Button size="sm" variant="outline" onClick={() => setClosing(y)}>
                          Close the year
                        </Button>
                      )}
                      {canManage && y.status === "closed" && (
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={run(async () => {
                            const reason = await confirm({
                              title: `Reopen ${y.fyLabel}?`,
                              description:
                                "Its closing entry is reversed and its days unlock. Close it again when done.",
                              confirmLabel: "Reopen the year",
                              destructive: true,
                              reason: { label: "Reason", required: true },
                            });
                            if (reason !== false)
                              reopenYear.mutate({ input: { endDate: y.endDate, reason } });
                          })}
                        >
                          Reopen
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </QueryView>
      </Section>
      {closing && <CloseYearDialog year={closing} onClose={() => setClosing(null)} />}
    </Page>
  );
}

function CloseYearDialog({ year, onClose }: { year: Api.YearRow; onClose: () => void }) {
  const money = useMoney();
  const [reason, setReason] = useState("");
  const preview = useCommand("year_close_preview", { endDate: year.endDate });
  const close = useAction("year_close", {
    invalidates: ["years_list", "period_status", "reports_run"],
    success: (y) => `${y.fyLabel} closed`,
    onSuccess: onClose,
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Close the year {year.fyLabel}</DialogTitle>
          <DialogDescription>
            One journal entry on {showDate(year.endDate)} moves every income and expense balance of
            the year to Retained Earnings, and the year is locked. Balances of assets, liabilities
            and parties carry forward as they are.
          </DialogDescription>
        </DialogHeader>
        <QueryView query={preview}>
          {(p) => (
            <div className="flex max-h-[55vh] flex-col gap-3 overflow-y-auto">
              {p.blockers.length > 0 && (
                <Alert variant="warning">
                  <AlertTitle>Not yet</AlertTitle>
                  <AlertDescription>
                    <ul className="list-disc pl-4">
                      {p.blockers.map((b) => (
                        <li key={b}>{b}</li>
                      ))}
                    </ul>
                  </AlertDescription>
                </Alert>
              )}
              <table className="w-full text-sm">
                <tbody>
                  {[["Income", p.income] as const, ["Expenses", p.expenses] as const].map(
                    ([title, lines]) => (
                      <Fragment key={title}>
                        <tr>
                          <th
                            colSpan={2}
                            className="pt-2 text-left text-xs font-medium text-muted-foreground"
                          >
                            {title}
                          </th>
                        </tr>
                        {lines.map((l) => (
                          <tr key={`${title}-${l.code}`} className="border-t border-border/40">
                            <td className="py-1">
                              {l.code} {l.name}
                            </td>
                            <td className="text-right tabular-nums">{money(l.amount)}</td>
                          </tr>
                        ))}
                      </Fragment>
                    ),
                  )}
                  <tr className="border-t-2 border-border font-semibold">
                    <td className="py-2">
                      {p.netProfit >= 0 ? "Net profit" : "Net loss"} to Retained Earnings
                    </td>
                    <td className="text-right tabular-nums">{money(Math.abs(p.netProfit))}</td>
                  </tr>
                </tbody>
              </table>
              <Field>
                <FieldLabel htmlFor="fy-reason">Note (optional)</FieldLabel>
                <Input
                  id="fy-reason"
                  value={reason}
                  maxLength={300}
                  onChange={(e) => setReason(e.target.value)}
                />
              </Field>
              <DialogFooter>
                <Button variant="ghost" onClick={onClose}>
                  Cancel
                </Button>
                <Button
                  disabled={p.blockers.length > 0 || close.isPending}
                  onClick={() =>
                    close.mutate({
                      input: {
                        endDate: year.endDate,
                        expectedNetProfit: p.netProfit,
                        reason: reason.trim() || null,
                      },
                    })
                  }
                >
                  Close {year.fyLabel}
                </Button>
              </DialogFooter>
            </div>
          )}
        </QueryView>
      </DialogContent>
    </Dialog>
  );
}
