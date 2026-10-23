import { useState } from "react";
import type { Api } from "@inventory/shared";
import { businessDate } from "@inventory/shared";
import { Link2, Link2Off, Upload, Wand2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DatePicker } from "@/components/ui/date-picker";
import { Field, FieldLabel } from "@/components/ui/field";
import { SelectField } from "@/components/ui/select-field";
import { Textarea } from "@/components/ui/textarea";
import { StatusBadge } from "@/components/app/badges";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput } from "@/components/app/inputs";
import { EmptyState, Facts, Page, QueryView, Section } from "@/components/app/page";
import { useMoney, usePublicSettings } from "@/lib/business";
import { label, showDate, showDateTime } from "@/lib/format";
import { useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { cn } from "@/lib/utils";
import { StatementImport } from "./statement-import";

const REFRESH = ["bank_view", "bank_reconciliation", "bank_history"] as const;

/** Bank reconciliation: statement lines on the left, the books on the right, matched. */
export default function BankPage() {
  const settings = usePublicSettings();
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const accounts = useCommand("payments_accounts", {});
  const banks = (accounts.data ?? []).filter((a) => a.subtype === "bank");
  const [chosen, setChosen] = useState("");
  const accountId = chosen || banks[0]?.id || "";
  const [importing, setImporting] = useState(false);
  const view = useList(
    "bank_view",
    { accountId, from: null, to: null },
    { enabled: Boolean(accountId) },
  );
  const auto = useAction("bank_auto_match", {
    invalidates: [...REFRESH],
    success: (r) =>
      `${r.matched} line(s) matched${r.ambiguous ? `, ${r.ambiguous} need you to choose` : ""}`,
  });
  return (
    <Page
      title="Bank reconciliation"
      description="Import the bank's statement, match each line to the books, and check the balances agree."
      actions={
        accountId && (
          <>
            <Button variant="outline" onClick={() => setImporting(true)}>
              <Upload /> Import statement
            </Button>
            <Button disabled={auto.isPending} onClick={() => auto.mutate({ input: { accountId } })}>
              <Wand2 /> Match automatically
            </Button>
          </>
        )
      }
    >
      {banks.length === 0 && !accounts.isPending ? (
        <EmptyState
          title="No bank account yet"
          description="Add your bank account under Accounts first."
        />
      ) : (
        <>
          <SelectField
            aria-label="Bank account"
            className="w-64"
            value={accountId}
            onValueChange={setChosen}
            options={banks.map((a) => ({ value: a.id, label: a.name }))}
          />
          <QueryView query={view}>{(v) => <Workbench view={v} today={today} />}</QueryView>
          {accountId && <ReconcilePanel accountId={accountId} today={today} />}
          {accountId && <History accountId={accountId} />}
        </>
      )}
      {importing && accountId && (
        <StatementImport
          accountId={accountId}
          accountName={banks.find((b) => b.id === accountId)?.name ?? "the bank account"}
          onClose={() => setImporting(false)}
        />
      )}
    </Page>
  );
}

function Workbench({ view, today }: { view: Api.BankView; today: string }) {
  const money = useMoney();
  const [statement, setStatement] = useState<string | null>(null);
  const [books, setBooks] = useState<string[]>([]);
  const [clearedOn, setClearedOn] = useState(today);
  const match = useAction("bank_match", { invalidates: [...REFRESH], success: "Matched" });
  const unmatch = useAction("bank_unmatch", { invalidates: [...REFRESH], success: "Unmatched" });
  const ignore = useAction("bank_ignore", { invalidates: [...REFRESH] });
  const tick = useAction("bank_tick", { invalidates: [...REFRESH], success: "Marked cleared" });
  const chosenLine = view.statementLines.find((l) => l.id === statement);
  const chosenTotal = view.bookLines
    .filter((b) => books.includes(b.journalLineId))
    .reduce((n, b) => n + b.amount, 0);
  const open = view.statementLines.filter((l) => l.status === "unmatched");
  const done = view.statementLines.filter((l) => l.status !== "unmatched");
  const uncleared = view.bookLines.filter((b) => !b.clearedDate);
  const cleared = view.bookLines.filter((b) => b.clearedDate);
  const toggle = (id: string) =>
    setBooks((b) => (b.includes(id) ? b.filter((x) => x !== id) : [...b, id]));
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Section
        title={`Statement (${open.length} to match)`}
        actions={
          chosenLine && (
            <Button
              size="sm"
              disabled={books.length === 0 || chosenTotal !== chosenLine.amount || match.isPending}
              onClick={() =>
                match.mutate(
                  { input: { statementLineId: chosenLine.id, journalLineIds: books } },
                  {
                    onSuccess: () => {
                      setStatement(null);
                      setBooks([]);
                    },
                  },
                )
              }
            >
              <Link2 /> Match {books.length ? `(${money(chosenTotal)})` : ""}
            </Button>
          )
        }
      >
        {view.statementLines.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No statement lines yet. Import the bank's CSV statement.
          </p>
        ) : (
          <ul className="flex flex-col" aria-label="Statement lines">
            {[...open, ...done].map((l) => (
              <li
                key={l.id}
                className={cn(
                  "flex items-start gap-2 border-t border-border/40 py-2 text-sm first:border-0",
                  statement === l.id && "bg-primary-soft/60",
                )}
              >
                {l.status === "unmatched" ? (
                  <input
                    type="radio"
                    name="statement-line"
                    className="mt-1"
                    aria-label={`Choose ${l.description}`}
                    checked={statement === l.id}
                    onChange={() => setStatement(l.id)}
                  />
                ) : (
                  <span className="w-3.5" />
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium" title={l.description}>
                    {l.description}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    {showDate(l.txnDate)}
                    {l.reference && ` · ${l.reference}`}
                    {l.ignoreNote && ` · ${l.ignoreNote}`}
                  </span>
                </span>
                <span className={cn("tabular-nums", l.amount < 0 && "text-danger")}>
                  {money(l.amount)}
                </span>
                {l.status !== "unmatched" && <StatusBadge status={l.status} />}
                {l.status === "matched" && (
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    aria-label={`Unmatch ${l.description}`}
                    onClick={() =>
                      unmatch.mutate({ input: { statementLineId: l.id, journalLineId: null } })
                    }
                  >
                    <Link2Off />
                  </Button>
                )}
                {l.status === "unmatched" && (
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={run(async () => {
                      const note = await confirm({
                        title: "Ignore this line?",
                        description:
                          "Only for lines that are not real money (an opening-balance row, a line repeated by the bank).",
                        confirmLabel: "Ignore",
                        reason: { label: "Why", required: true },
                      });
                      if (note !== false) ignore.mutate({ input: { statementLineId: l.id, note } });
                    })}
                  >
                    Ignore
                  </Button>
                )}
                {l.status === "ignored" && (
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() => ignore.mutate({ input: { statementLineId: l.id, note: null } })}
                  >
                    Restore
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </Section>
      <Section
        title={`Books (${uncleared.length} not cleared)`}
        actions={
          books.length > 0 &&
          !chosenLine && (
            <span className="flex items-center gap-2">
              <DatePicker
                aria-label="Cleared on"
                className="w-40"
                value={clearedOn}
                max={today}
                onChange={(v) => v && setClearedOn(v)}
              />
              <Button
                size="sm"
                variant="outline"
                onClick={() =>
                  tick.mutate(
                    {
                      input: {
                        accountId: view.accountId,
                        journalLineIds: books,
                        clearedDate: clearedOn,
                      },
                    },
                    { onSuccess: () => setBooks([]) },
                  )
                }
              >
                Mark cleared
              </Button>
            </span>
          )
        }
      >
        <p className="mb-2 text-xs text-muted-foreground">
          Choose a statement line, then the book lines it stands for. Automatic matching looks{" "}
          {view.matchDays} days either side.
        </p>
        <ul className="flex flex-col" aria-label="Book lines">
          {[...uncleared, ...cleared].map((b) => (
            <li
              key={b.journalLineId}
              className="flex items-start gap-2 border-t border-border/40 py-2 text-sm first:border-0"
            >
              {!b.clearedDate ? (
                <Checkbox
                  className="mt-0.5"
                  aria-label={`Choose ${b.narration}`}
                  checked={books.includes(b.journalLineId)}
                  onCheckedChange={() => toggle(b.journalLineId)}
                />
              ) : (
                <span className="w-4" />
              )}
              <span className="min-w-0 flex-1">
                <span className="block truncate" title={b.narration}>
                  {b.narration}
                </span>
                <span className="text-xs text-muted-foreground">
                  {showDate(b.businessDate)} · {label(b.sourceType)}
                  {b.reference && ` · ${b.reference}`}
                  {b.clearedDate && ` · cleared ${showDate(b.clearedDate)} (${b.method ?? ""})`}
                </span>
              </span>
              <span className={cn("tabular-nums", b.amount < 0 && "text-danger")}>
                {money(b.amount)}
              </span>
              {b.method === "ticked" && (
                <Button
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Not cleared: ${b.narration}`}
                  onClick={() =>
                    unmatch.mutate({
                      input: { statementLineId: null, journalLineId: b.journalLineId },
                    })
                  }
                >
                  <Link2Off />
                </Button>
              )}
              {b.clearedDate && b.method !== "ticked" && <Badge variant="success">Matched</Badge>}
            </li>
          ))}
        </ul>
      </Section>
    </div>
  );
}

function ReconcilePanel({ accountId, today }: { accountId: string; today: string }) {
  const money = useMoney();
  const [day, setDay] = useState(today);
  const [balance, setBalance] = useState<number | null>(null);
  const [notes, setNotes] = useState("");
  const r = useList("bank_reconciliation", {
    accountId,
    statementDate: day,
    statementBalance: balance,
  });
  const finish = useAction("bank_finish", {
    invalidates: [...REFRESH],
    success: "Reconciliation saved",
  });
  return (
    <Section title="Reconciliation statement">
      <div className="mb-3 flex flex-wrap items-end gap-3">
        <Field>
          <FieldLabel htmlFor="br-date">Statement date</FieldLabel>
          <DatePicker
            id="br-date"
            className="w-44"
            value={day}
            max={today}
            onChange={(v) => v && setDay(v)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="br-balance">Balance on the statement</FieldLabel>
          <MoneyInput
            id="br-balance"
            className="w-44"
            value={balance ?? r.data?.statementBalance ?? null}
            onValueChange={setBalance}
          />
        </Field>
      </div>
      <QueryView query={r}>
        {(rec) => (
          <>
            <Facts
              items={[
                ["Balance in the books", money(rec.bookBalance)],
                [
                  `Less money in not on the statement (${rec.unclearedIn.length})`,
                  `−${money(rec.unclearedInTotal)}`,
                ],
                [
                  `Add money out not on the statement (${rec.unclearedOut.length})`,
                  money(rec.unclearedOutTotal),
                ],
                [
                  `Statement lines not in the books (${rec.notInBooks.length})`,
                  money(rec.notInBooksTotal),
                ],
                ["The statement should show", money(rec.expectedStatementBalance)],
                [
                  "The statement shows",
                  rec.statementBalance !== null && rec.statementBalance !== undefined
                    ? money(rec.statementBalance)
                    : "—",
                ],
                [
                  "Difference",
                  rec.difference === null || rec.difference === undefined ? (
                    "—"
                  ) : rec.difference === 0 ? (
                    <span className="font-semibold text-success">Reconciled</span>
                  ) : (
                    <span className="font-semibold text-danger">{money(rec.difference)}</span>
                  ),
                ],
              ]}
            />
            <div className="mt-3 flex flex-wrap items-end gap-3">
              {rec.difference !== 0 && (
                <Field className="min-w-64 flex-1">
                  <FieldLabel htmlFor="br-notes">Why they still differ</FieldLabel>
                  <Textarea
                    id="br-notes"
                    className="min-h-10"
                    value={notes}
                    onChange={(e) => setNotes(e.target.value)}
                  />
                </Field>
              )}
              <Button
                disabled={
                  rec.statementBalance === null ||
                  rec.statementBalance === undefined ||
                  finish.isPending ||
                  (rec.difference !== 0 && !notes.trim())
                }
                onClick={() =>
                  finish.mutate({
                    input: {
                      accountId,
                      statementDate: day,
                      statementBalance: rec.statementBalance ?? 0,
                      notes: notes.trim() || null,
                    },
                  })
                }
              >
                Finish reconciliation
              </Button>
            </div>
          </>
        )}
      </QueryView>
    </Section>
  );
}

function History({ accountId }: { accountId: string }) {
  const money = useMoney();
  const history = useCommand("bank_history", { accountId });
  return (
    <Section title="Earlier reconciliations">
      <QueryView query={history}>
        {(list) =>
          list.length === 0 ? (
            <p className="text-sm text-muted-foreground">None yet.</p>
          ) : (
            <table className="w-full text-sm">
              <thead className="text-xs text-muted-foreground">
                <tr>
                  <th className="py-1 text-left font-medium">Statement date</th>
                  <th className="py-1 text-right font-medium">Statement</th>
                  <th className="py-1 text-right font-medium">Books</th>
                  <th className="py-1 text-right font-medium">Difference</th>
                  <th className="py-1 text-left font-medium">By</th>
                </tr>
              </thead>
              <tbody>
                {list.map((h) => (
                  <tr key={h.id} className="border-t border-border/40">
                    <td className="py-1.5 font-medium">{showDate(h.statementDate)}</td>
                    <td className="text-right tabular-nums">{money(h.statementBalance)}</td>
                    <td className="text-right tabular-nums">{money(h.bookBalance)}</td>
                    <td
                      className={cn("text-right tabular-nums", h.difference !== 0 && "text-danger")}
                    >
                      {money(h.difference)}
                    </td>
                    <td className="text-muted-foreground">
                      {h.createdByName} · {showDateTime(h.createdAt)}
                      {h.notes && ` · ${h.notes}`}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )
        }
      </QueryView>
    </Section>
  );
}
