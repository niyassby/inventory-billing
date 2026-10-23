import { useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import type { Api } from "@inventory/shared";
import { businessDate } from "@inventory/shared";
import { CheckCheck, Plus, Send, ShieldAlert, Trash2, Undo2 } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { DatePicker } from "@/components/ui/date-picker";
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
import { SelectField } from "@/components/ui/select-field";
import { StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput } from "@/components/app/inputs";
import { Page, QueryView } from "@/components/app/page";
import { PartyPicker } from "@/components/app/pickers";
import { SearchInput } from "@/components/app/search-input";
import { requestId } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showDate, showDateTime } from "@/lib/format";
import { useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useCan } from "@/stores/session";
import { Attachments } from "./attachments";

const STATUSES = ["draft", "submitted", "approved", "posted", "reversed"];
const REFRESH = ["journals_list", "journals_get", "accounts_list", "reports_run"] as const;
/** Control accounts: an owner must allow them on the journal. */
const RESTRICTED = new Set(["receivable", "payable", "tax_input", "tax_output"]);

interface LineDraft {
  key: string;
  accountId: string;
  partyId: string;
  partyName: string;
  debit: number | null;
  credit: number | null;
  memo: string;
}

const blank = (): LineDraft => ({
  key: requestId(),
  accountId: "",
  partyId: "",
  partyName: "",
  debit: null,
  credit: null,
  memo: "",
});

/** Manual journals: written, reviewed, approved, then posted. */
export default function JournalsPage() {
  const money = useMoney();
  const canWrite = useCan("journal.manage");
  const table = useTableState("journals", { filters: { q: "", status: "" } });
  const [search, setSearch] = useSearchFilter(table);
  const [params] = useSearchParams();
  const [open, setOpen] = useState<string | null>(() => params.get("open"));
  const query = useList("journals_list", {
    search: table.filter("q") || null,
    status: table.filter("status") || null,
    ...table.input,
  });
  const columns = useMemo<ColumnDef<Api.JournalListItem>[]>(
    () => [
      {
        header: "Number",
        accessorKey: "docNo",
        meta: { sortKey: "docNo" },
        cell: ({ row }) => <span className="font-medium">{row.original.docNo}</span>,
      },
      {
        header: "Date",
        accessorKey: "businessDate",
        meta: { sortKey: "date" },
        cell: ({ row }) => showDate(row.original.businessDate),
      },
      {
        header: "Narration",
        accessorKey: "narration",
        cell: ({ row }) => (
          <span className="block max-w-md truncate" title={row.original.narration}>
            {row.original.narration}
            {row.original.attachmentCount > 0 && (
              <span className="ml-2 text-xs text-muted-foreground">
                · {row.original.attachmentCount} file(s)
              </span>
            )}
          </span>
        ),
      },
      { header: "Written by", accessorFn: (r) => r.createdByName ?? "" },
      {
        header: "Status",
        accessorKey: "status",
        meta: { sortKey: "status" },
        cell: ({ row }) => <StatusBadge status={row.original.status} />,
      },
      {
        header: "Amount",
        accessorKey: "total",
        meta: { align: "right", sortKey: "total" },
        cell: ({ row }) => money(row.original.total),
      },
    ],
    [money],
  );
  return (
    <Page
      title="Journals"
      description="Adjustments the documents don't make on their own: written, approved by the owner, then posted. A posted journal is corrected only by reversing it."
      actions={
        canWrite && (
          <Button onClick={() => setOpen("new")}>
            <Plus /> New journal
          </Button>
        )
      }
    >
      <div className="flex flex-wrap gap-3">
        <SearchInput
          aria-label="Search journals"
          placeholder="Number or narration"
          className="max-w-xs"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <SelectField
          aria-label="Status"
          className="w-44"
          value={table.filter("status")}
          onValueChange={(v) => table.setFilter("status", v)}
          options={[
            { value: "", label: "Every status" },
            ...STATUSES.map((s) => ({ value: s, label: s.charAt(0).toUpperCase() + s.slice(1) })),
          ]}
        />
      </div>
      <DataTable
        viewKey="journals"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) => setOpen(r.id)}
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{
          title: "No journals yet",
          description: "Use a journal for accruals, corrections and opening balances.",
        }}
      />
      {open && <JournalDialog id={open === "new" ? null : open} onClose={() => setOpen(null)} />}
    </Page>
  );
}

function JournalDialog({ id, onClose }: { id: string | null; onClose: () => void }) {
  const [current, setCurrent] = useState(id);
  const journal = useCommand("journals_get", { id: current ?? "" }, { enabled: Boolean(current) });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-4xl">
        {current ? (
          <QueryView query={journal}>
            {(j) => (
              <JournalBody key={j.updatedAt} journal={j} onClose={onClose} onSaved={setCurrent} />
            )}
          </QueryView>
        ) : (
          <JournalBody journal={null} onClose={onClose} onSaved={setCurrent} />
        )}
      </DialogContent>
    </Dialog>
  );
}

/** One review step (submit, approve, post) with its toast. */
function useStep(command: "journals_submit" | "journals_approve" | "journals_post", text: string) {
  return useAction(command, {
    invalidates: [...REFRESH],
    success: (j) => `${j.docNo} ${text}`,
  });
}

function JournalBody({
  journal,
  onClose,
  onSaved,
}: {
  journal: Api.JournalDetail | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  const money = useMoney();
  const settings = usePublicSettings();
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const canWrite = useCan("journal.manage");
  const canApprove = useCan("journal.approve");
  const canAllow = useCan("journal.restricted");
  const accounts = useCommand("accounts_list", {});
  const status = journal?.status ?? "draft";
  const editable = canWrite && status === "draft" && !journal?.reversalOf;
  const datesOnly = canWrite && status === "draft" && Boolean(journal?.reversalOf);
  const [date, setDate] = useState(journal?.businessDate ?? today);
  const [narration, setNarration] = useState(journal?.narration ?? "");
  const [lines, setLines] = useState<LineDraft[]>(
    journal
      ? journal.lines.map((l) => ({
          key: requestId(),
          accountId: l.accountId,
          partyId: l.partyId ?? "",
          partyName: l.partyName ?? "",
          debit: l.debit || null,
          credit: l.credit || null,
          memo: l.memo ?? "",
        }))
      : [blank(), blank()],
  );
  const [key] = useState(requestId);
  const accountList = (accounts.data ?? []).filter(
    (a) => a.isActive && a.subtype !== "inventory" && a.code !== "2050",
  );
  const subtype = (id: string) => accountList.find((a) => a.id === id)?.subtype ?? "";
  const debit = lines.reduce((n, l) => n + (l.debit ?? 0), 0);
  const credit = lines.reduce((n, l) => n + (l.credit ?? 0), 0);
  const usesRestricted = lines.some((l) => RESTRICTED.has(subtype(l.accountId)));
  const done = { invalidates: [...REFRESH] };
  const save = useAction("journals_save", {
    ...done,
    success: (j) => `${j.docNo} saved`,
    onSuccess: (j) => onSaved(j.id),
  });
  const submit = useStep("journals_submit", "submitted");
  const approve = useStep("journals_approve", "approved");
  const post = useStep("journals_post", "posted");
  const sendBack = useAction("journals_send_back", { ...done, success: "Sent back to draft" });
  const allow = useAction("journals_allow_restricted", {
    ...done,
    success: "Control accounts allowed",
  });
  const remove = useAction("journals_delete", {
    invalidates: ["journals_list"],
    success: "Draft deleted",
    onSuccess: onClose,
  });
  const reverse = useAction("journals_reverse", {
    ...done,
    success: (j) => `${j.docNo} drafted to reverse it`,
    onSuccess: (j) => onSaved(j.id),
  });
  const set = (i: number, patch: Partial<LineDraft>) =>
    setLines((ls) => ls.map((l, j) => (j === i ? { ...l, ...patch } : l)));
  const ref = journal ? { id: journal.id, expectedUpdatedAt: journal.updatedAt, note: null } : null;
  const saveDraft = () =>
    save.mutate({
      input: {
        id: journal?.id ?? null,
        expectedUpdatedAt: journal?.updatedAt ?? null,
        idempotencyKey: journal ? null : key,
        businessDate: date,
        narration,
        lines: lines
          .filter((l) => l.accountId || l.debit || l.credit)
          .map((l) => ({
            accountId: l.accountId,
            partyId: l.partyId || null,
            debit: l.debit ?? 0,
            credit: l.credit ?? 0,
            memo: l.memo || null,
          })),
      },
    });
  return (
    <>
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          {journal ? journal.docNo : "New journal"}
          <StatusBadge status={status} />
        </DialogTitle>
        <DialogDescription>
          {journal?.reversalOf
            ? `Reverses ${journal.reversalOf.docNo}: every line swapped.`
            : "Debits and credits must be equal. Stock accounts change only through stock documents."}
        </DialogDescription>
      </DialogHeader>
      <div className="flex max-h-[65vh] flex-col gap-4 overflow-y-auto pr-1">
        {journal?.reviewNote && status === "draft" && (
          <Alert variant="warning">
            <AlertTitle>Sent back</AlertTitle>
            <AlertDescription>{journal.reviewNote}</AlertDescription>
          </Alert>
        )}
        <div className="grid gap-3 sm:grid-cols-[12rem_1fr]">
          <Field>
            <FieldLabel htmlFor="jv-date">Date</FieldLabel>
            <DatePicker
              id="jv-date"
              value={date}
              max={today}
              disabled={!editable && !datesOnly}
              onChange={(v) => v && setDate(v)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="jv-narration">Narration</FieldLabel>
            <Input
              id="jv-narration"
              value={narration}
              maxLength={300}
              disabled={!editable && !datesOnly}
              onChange={(e) => setNarration(e.target.value)}
            />
          </Field>
        </div>
        <table className="w-full text-sm">
          <thead className="text-xs text-muted-foreground">
            <tr>
              <th className="py-1 text-left font-medium">Account</th>
              <th className="py-1 text-left font-medium">Customer or supplier</th>
              <th className="w-36 py-1 text-right font-medium">Debit</th>
              <th className="w-36 py-1 text-right font-medium">Credit</th>
              <th className="py-1 text-left font-medium">Memo</th>
              {editable && <th className="w-10" />}
            </tr>
          </thead>
          <tbody>
            {lines.map((l, i) => {
              const needsParty = ["receivable", "payable"].includes(subtype(l.accountId));
              return (
                <tr key={l.key} className="border-t border-border/40 align-top">
                  <td className="py-1.5 pr-2">
                    <SelectField
                      aria-label={`Account, line ${i + 1}`}
                      value={l.accountId}
                      disabled={!editable}
                      onValueChange={(accountId) =>
                        set(i, { accountId, partyId: "", partyName: "" })
                      }
                      options={[
                        { value: "", label: "Choose an account" },
                        ...accountList.map((a) => ({
                          value: a.id,
                          label: `${a.code} ${a.name}`,
                        })),
                      ]}
                    />
                  </td>
                  <td className="py-1.5 pr-2">
                    {needsParty ? (
                      l.partyId ? (
                        <span className="flex items-center gap-1">
                          <span className="truncate">{l.partyName}</span>
                          {editable && (
                            <Button
                              size="xs"
                              variant="ghost"
                              onClick={() => set(i, { partyId: "", partyName: "" })}
                            >
                              Change
                            </Button>
                          )}
                        </span>
                      ) : editable ? (
                        <PartyPicker
                          partyType={subtype(l.accountId) === "payable" ? "supplier" : "customer"}
                          onSelect={(p) => set(i, { partyId: p.id, partyName: p.name })}
                        />
                      ) : null
                    ) : (
                      <span className="text-xs text-muted-foreground">—</span>
                    )}
                  </td>
                  <td className="py-1.5 pr-2">
                    <MoneyInput
                      aria-label={`Debit, line ${i + 1}`}
                      value={l.debit}
                      disabled={!editable}
                      onValueChange={(debit) => set(i, { debit, credit: debit ? null : l.credit })}
                    />
                  </td>
                  <td className="py-1.5 pr-2">
                    <MoneyInput
                      aria-label={`Credit, line ${i + 1}`}
                      value={l.credit}
                      disabled={!editable}
                      onValueChange={(credit) => set(i, { credit, debit: credit ? null : l.debit })}
                    />
                  </td>
                  <td className="py-1.5 pr-2">
                    <Input
                      aria-label={`Memo, line ${i + 1}`}
                      value={l.memo}
                      maxLength={200}
                      disabled={!editable}
                      onChange={(e) => set(i, { memo: e.target.value })}
                    />
                  </td>
                  {editable && (
                    <td className="py-1.5">
                      <Button
                        size="icon-sm"
                        variant="ghost"
                        aria-label={`Remove line ${i + 1}`}
                        disabled={lines.length <= 2}
                        onClick={() => setLines((ls) => ls.filter((_, j) => j !== i))}
                      >
                        <Trash2 />
                      </Button>
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
          <tfoot>
            <tr className="border-t-2 border-border font-semibold">
              <td colSpan={2} className="py-2">
                {editable && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setLines((ls) => [...ls, blank()])}
                  >
                    <Plus /> Add a line
                  </Button>
                )}
              </td>
              <td className="py-2 text-right tabular-nums">{money(debit)}</td>
              <td className="py-2 text-right tabular-nums">{money(credit)}</td>
              <td colSpan={editable ? 2 : 1} className="py-2 pl-2 text-sm font-normal">
                {debit !== credit ? (
                  <span className="text-danger">Difference {money(Math.abs(debit - credit))}</span>
                ) : debit > 0 ? (
                  <span className="text-success">Balanced</span>
                ) : null}
              </td>
            </tr>
          </tfoot>
        </table>
        {(usesRestricted || journal?.allowRestricted) && (
          <Alert variant={journal?.allowRestricted ? "info" : "warning"}>
            <AlertTitle>Control accounts</AlertTitle>
            <AlertDescription>
              {journal?.allowRestricted
                ? `Allowed by ${journal.restrictedByName ?? "the owner"}: ${journal.restrictedReason ?? ""}`
                : "Receivables, payables and tax are kept by the documents. An owner must allow this journal, with a reason, before it can go further. Receivable and payable lines also post to the party's account."}
            </AlertDescription>
          </Alert>
        )}
        {journal && (
          <section className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">Supporting documents</h3>
            <Attachments
              entityType="manual_journal"
              entityId={journal.id}
              items={journal.attachments}
              editable={canWrite && !["posted", "reversed"].includes(status)}
              invalidates={["journals_get", "journals_list"]}
            />
          </section>
        )}
        {journal && (
          <p className="text-xs text-muted-foreground">
            Written by {journal.createdByName ?? "—"} {showDateTime(journal.createdAt)}
            {journal.submittedAt && ` · submitted ${showDateTime(journal.submittedAt)}`}
            {journal.approvedByName && ` · approved by ${journal.approvedByName}`}
            {journal.postedAt &&
              ` · posted ${showDateTime(journal.postedAt)} by ${journal.postedByName ?? ""}`}
            {journal.reversedBy && ` · reversed by ${journal.reversedBy.docNo}`}
          </p>
        )}
      </div>
      <DialogFooter className="flex-wrap gap-2">
        {journal && status === "draft" && canWrite && !journal.reversalOf && (
          <Button
            variant="ghost"
            className="mr-auto"
            onClick={run(async () => {
              const ok = await confirm({
                title: `Delete ${journal.docNo}?`,
                description: "The draft is thrown away. Its number is not used again.",
                confirmLabel: "Delete draft",
                destructive: true,
              });
              if (ok !== false && ref) remove.mutate({ input: ref });
            })}
          >
            <Trash2 /> Delete draft
          </Button>
        )}
        {journal &&
          usesRestricted &&
          !journal.allowRestricted &&
          canAllow &&
          ["draft", "submitted"].includes(status) && (
            <Button
              variant="outline"
              onClick={run(async () => {
                const reason = await confirm({
                  title: "Allow control accounts on this journal?",
                  description: "The reason is kept with the journal and in the audit log.",
                  confirmLabel: "Allow",
                  reason: { label: "Why", required: true },
                });
                if (reason !== false && ref) allow.mutate({ input: { ...ref, note: reason } });
              })}
            >
              <ShieldAlert /> Allow control accounts
            </Button>
          )}
        {(editable || datesOnly) && (
          <Button variant="outline" disabled={save.isPending} onClick={saveDraft}>
            {journal ? "Save changes" : "Save draft"}
          </Button>
        )}
        {journal && status === "draft" && canWrite && (
          <Button
            disabled={submit.isPending || debit !== credit || debit === 0}
            onClick={() => ref && submit.mutate({ input: ref })}
          >
            <Send /> Submit for approval
          </Button>
        )}
        {journal && ["submitted", "approved"].includes(status) && canApprove && (
          <Button
            variant="outline"
            onClick={run(async () => {
              const note = await confirm({
                title: `Send ${journal.docNo} back?`,
                description: "It returns to draft with your note.",
                confirmLabel: "Send back",
                reason: { label: "What to fix", required: true },
              });
              if (note !== false && ref) sendBack.mutate({ input: { ...ref, note } });
            })}
          >
            Send back
          </Button>
        )}
        {journal && status === "submitted" && canApprove && (
          <Button
            disabled={approve.isPending}
            onClick={() => ref && approve.mutate({ input: ref })}
          >
            <CheckCheck /> Approve
          </Button>
        )}
        {journal && status === "approved" && canWrite && (
          <Button disabled={post.isPending} onClick={() => ref && post.mutate({ input: ref })}>
            Post to the books
          </Button>
        )}
        {journal && status === "posted" && canWrite && !journal.reversalOf && (
          <Button
            variant="outline"
            onClick={run(async () => {
              const reason = await confirm({
                title: `Reverse ${journal.docNo}?`,
                description:
                  "A new journal with every line swapped is drafted. It is approved and posted like any journal.",
                confirmLabel: "Draft the reversal",
                reason: { label: "Reason", required: true },
              });
              if (reason !== false)
                reverse.mutate({
                  input: {
                    id: journal.id,
                    idempotencyKey: requestId(),
                    businessDate: null,
                    reason,
                  },
                });
            })}
          >
            <Undo2 /> Reverse
          </Button>
        )}
        <Button variant="ghost" onClick={onClose}>
          Close
        </Button>
      </DialogFooter>
    </>
  );
}
