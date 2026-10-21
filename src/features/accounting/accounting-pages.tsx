import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { businessDate } from "@inventory/shared";
import { Ban, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
import { Textarea } from "@/components/ui/textarea";
import { StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput, RateInput } from "@/components/app/inputs";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { PartyPicker } from "@/components/app/pickers";
import { call, requestId } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { label, showDate, showDateTime } from "@/lib/format";
import { errorText, useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";
import { useQueryClient } from "@tanstack/react-query";

// ------------------------------------------------------------------------------------- expenses

function ExpenseDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const queryClient = useQueryClient();
  const categories = useCommand("expenses_categories", {}, { enabled: open });
  const [key, setKey] = useState(requestId);
  const [categoryId, setCategoryId] = useState("");
  const [title, setTitle] = useState("");
  const [amount, setAmount] = useState<number | null>(null);
  const [taxRate, setTaxRate] = useState<number | null>(0);
  const [mode, setMode] = useState("cash");
  const [onCredit, setOnCredit] = useState(false);
  const [supplier, setSupplier] = useState<{ id: string; name: string } | null>(null);
  const [reference, setReference] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setKey(requestId());
      setTitle("");
      setAmount(null);
      setTaxRate(0);
      setOnCredit(false);
      setSupplier(null);
      setReference("");
    }
  }, [open]);
  useEffect(() => {
    if (open && !categoryId && categories.data?.[0]) setCategoryId(categories.data[0].id);
  }, [open, categoryId, categories.data]);
  const total = (amount ?? 0) + Math.round(((amount ?? 0) * (taxRate ?? 0)) / 1_000_000);
  const valid = Boolean(categoryId && title.trim() && amount && (!onCredit || supplier));
  const save = async () => {
    if (!amount) return;
    setBusy(true);
    try {
      const e = await call("expenses_create", {
        idempotencyKey: key,
        categoryId,
        title,
        referenceNo: reference || null,
        amount,
        taxRate: taxRate ?? 0,
        partyId: supplier?.id ?? null,
        payment: onCredit
          ? null
          : { mode, amount: total, tendered: null, accountId: null, reference: null },
      });
      toast.success(`${e.docNo} saved`);
      for (const k of ["expenses_list", "parties_get", "parties_list"])
        void queryClient.invalidateQueries({ queryKey: [k] });
      onClose();
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <DialogHeader>
            <DialogTitle>New expense</DialogTitle>
          </DialogHeader>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field>
              <FieldLabel htmlFor="ex-cat">Category</FieldLabel>
              <SelectField
                id="ex-cat"
                className="w-full"
                value={categoryId}
                onValueChange={setCategoryId}
                options={[...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name }))]}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ex-title">What for</FieldLabel>
              <Input
                id="ex-title"
                autoFocus
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="ex-amount">Amount (before tax)</FieldLabel>
              <MoneyInput id="ex-amount" value={amount} onValueChange={setAmount} />
            </Field>
            <Field>
              <FieldLabel htmlFor="ex-tax">Tax on the bill</FieldLabel>
              <RateInput id="ex-tax" value={taxRate} onValueChange={setTaxRate} />
            </Field>
            <Field>
              <FieldLabel htmlFor="ex-ref">Bill / reference no.</FieldLabel>
              <Input id="ex-ref" value={reference} onChange={(e) => setReference(e.target.value)} />
            </Field>
            {!onCredit && (
              <Field>
                <FieldLabel htmlFor="ex-mode">Paid by</FieldLabel>
                <SelectField
                  id="ex-mode"
                  className="w-full"
                  value={mode}
                  onValueChange={setMode}
                  options={[
                    ...["cash", "upi", "card", "bank_transfer", "cheque"].map((m) => ({
                      value: m,
                      label: label(m),
                    })),
                  ]}
                />
              </Field>
            )}
          </div>
          <label className="flex items-center gap-2 text-sm">
            <Checkbox checked={onCredit} onCheckedChange={setOnCredit} /> Not paid yet (owed to a
            supplier)
          </label>
          {onCredit &&
            (supplier ? (
              <p className="text-sm">
                Owed to <strong>{supplier.name}</strong>{" "}
                <Button type="button" variant="ghost" size="xs" onClick={() => setSupplier(null)}>
                  Change
                </Button>
              </p>
            ) : (
              <PartyPicker
                partyType="supplier"
                onSelect={(p) => setSupplier({ id: p.id, name: p.name })}
              />
            ))}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !valid}>
              Save {total ? "·" : ""} {total ? <TotalText minor={total} /> : null}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function TotalText({ minor }: { minor: number }) {
  const money = useMoney();
  return <>{money(minor)}</>;
}

function CategoriesDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const categories = useCommand("expenses_categories", {}, { enabled: open });
  const [name, setName] = useState("");
  const save = useAction("expenses_save_category", {
    invalidates: ["expenses_categories"],
    success: "Category added",
    onSuccess: () => setName(""),
  });
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Expense categories</DialogTitle>
          <DialogDescription>
            Each category posts to the General Expenses account.
          </DialogDescription>
        </DialogHeader>
        <ul className="max-h-60 divide-y divide-border/40 overflow-auto text-sm">
          {categories.data?.map((c) => (
            <li key={c.id} className="py-1.5">
              {c.name}
            </li>
          ))}
        </ul>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) save.mutate({ input: { name: name.trim() } });
          }}
        >
          <Input
            aria-label="New category"
            placeholder="New category"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Button type="submit" disabled={!name.trim() || save.isPending}>
            Add
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function ExpensesPage() {
  const money = useMoney();
  const canManage = useCan("expenses.manage");
  const categories = useCommand("expenses_categories", {});
  const table = useTableState("expenses", { filters: { q: "", category: "" } });
  const [search, setSearch] = useSearchFilter(table);
  const [creating, setCreating] = useState(false);
  const [managing, setManaging] = useState(false);
  const query = useList("expenses_list", {
    search: table.filter("q") || null,
    categoryId: table.filter("category") || null,
    ...table.input,
  });
  const voidExpense = useAction("expenses_void", {
    invalidates: ["expenses_list", "parties_get"],
    success: "Expense voided",
  });
  const columns = useMemo<ColumnDef<Api.ExpenseDetail>[]>(
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
      { header: "Category", accessorKey: "categoryName", meta: { sortKey: "category" } },
      { header: "What for", accessorKey: "title", meta: { sortKey: "title" } },
      {
        header: "Status",
        accessorKey: "status",
        meta: { sortKey: "status" },
        cell: ({ row }) =>
          row.original.status === "voided" ? (
            <StatusBadge status="voided" />
          ) : row.original.amountPaid >= row.original.total ? (
            <StatusBadge status="paid" />
          ) : (
            <StatusBadge status="unpaid" />
          ),
      },
      {
        header: "Total",
        accessorKey: "total",
        meta: { align: "right", sortKey: "total" },
        cell: ({ row }) => money(row.original.total),
      },
      {
        header: "",
        id: "void",
        enableSorting: false,
        cell: ({ row }) =>
          canManage && row.original.status !== "voided" ? (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Void ${row.original.docNo}`}
              onClick={run(async () => {
                const reason = await confirm({
                  title: `Void ${row.original.docNo}?`,
                  description:
                    "The money (or the supplier's balance) is put back and the books are reversed.",
                  confirmLabel: "Void expense",
                  destructive: true,
                  reason: { label: "Reason", required: true },
                });
                if (reason !== false)
                  voidExpense.mutate({ input: { id: row.original.id, reason } });
              })}
            >
              <Ban />
            </Button>
          ) : null,
      },
    ],
    [money, canManage, voidExpense],
  );
  return (
    <Page
      title="Expenses"
      description="Rent, salaries, electricity and everything else you spend on running the business."
      actions={
        canManage && (
          <>
            <Button variant="outline" onClick={() => setManaging(true)}>
              Categories
            </Button>
            <Button onClick={() => setCreating(true)}>
              <Plus /> New expense
            </Button>
          </>
        )
      }
    >
      <div className="flex flex-wrap gap-3">
        <SearchInput
          aria-label="Search expenses"
          placeholder="Number or what for"
          className="max-w-xs"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <SelectField
          aria-label="Category"
          className="w-48"
          value={table.filter("category")}
          onValueChange={(v) => table.setFilter("category", v)}
          options={[
            { value: "", label: "All categories" },
            ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
          ]}
        />
      </div>
      <DataTable
        viewKey="expenses"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{ title: "No expenses yet" }}
      />
      <ExpenseDialog open={creating} onClose={() => setCreating(false)} />
      <CategoriesDialog open={managing} onClose={() => setManaging(false)} />
    </Page>
  );
}

// ------------------------------------------------------------------------------------- accounts

const TYPE_ORDER = ["asset", "liability", "equity", "income", "expense"];

export function AccountsPage() {
  const money = useMoney();
  const canManage = useCan("accounting.manage");
  const accounts = useCommand("accounts_list", {});
  const [editing, setEditing] = useState<Partial<Api.MoneyAccountInput> | null>(null);
  const save = useAction("accounts_save", {
    invalidates: ["accounts_list"],
    success: "Account saved",
    onSuccess: () => setEditing(null),
  });
  return (
    <Page
      title="Accounts"
      description="The chart of accounts with balances. Cash drawers and bank accounts can be added; the rest are built in."
      actions={
        canManage && (
          <Button onClick={() => setEditing({ kind: "bank", name: "", isActive: true })}>
            <Plus /> Add cash or bank account
          </Button>
        )
      }
    >
      <QueryView query={accounts}>
        {(list) => (
          <div className="grid gap-4">
            {TYPE_ORDER.map((type) => {
              const rows = list.filter((a) => a.accountType === type);
              if (rows.length === 0) return null;
              return (
                <Section
                  key={type}
                  title={
                    label(type) +
                    (type === "asset" || type === "expense"
                      ? "s"
                      : type === "liability"
                        ? " accounts"
                        : "")
                  }
                >
                  <table className="w-full text-sm">
                    <tbody>
                      {rows.map((a) => (
                        <tr key={a.id} className="border-t border-border/40 first:border-0">
                          <td className="w-16 py-1.5 text-muted-foreground">{a.code}</td>
                          <td className="font-medium">
                            {a.name}
                            {!a.isActive && (
                              <span className="ml-2 text-xs text-muted-foreground">(inactive)</span>
                            )}
                          </td>
                          <td className="text-muted-foreground">
                            {a.bankName ? `${a.bankName} ${a.bankAccountNo ?? ""}` : ""}
                          </td>
                          <td className="text-right tabular-nums">{money(a.balance)}</td>
                          <td className="w-12 text-right">
                            {canManage && (a.subtype === "cash" || a.subtype === "bank") && (
                              <Button
                                variant="ghost"
                                size="xs"
                                onClick={() =>
                                  setEditing({
                                    id: a.id,
                                    kind: a.subtype,
                                    name: a.name,
                                    bankName: a.bankName ?? null,
                                    bankAccountNo: a.bankAccountNo ?? null,
                                    isActive: a.isActive,
                                  })
                                }
                              >
                                Edit
                              </Button>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </Section>
              );
            })}
          </div>
        )}
      </QueryView>
      <Dialog open={editing !== null} onOpenChange={(o) => !o && setEditing(null)}>
        <DialogContent>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (editing?.name) {
                save.mutate({
                  input: {
                    id: editing.id ?? null,
                    kind: editing.kind ?? "bank",
                    name: editing.name,
                    bankName: editing.bankName ?? null,
                    bankAccountNo: editing.bankAccountNo ?? null,
                    bankIfsc: editing.bankIfsc ?? null,
                    isActive: editing.isActive ?? true,
                  },
                });
              }
            }}
          >
            <DialogHeader>
              <DialogTitle>{editing?.id ? "Edit account" : "Add cash or bank account"}</DialogTitle>
            </DialogHeader>
            {!editing?.id && (
              <Field>
                <FieldLabel htmlFor="acc-kind">Type</FieldLabel>
                <SelectField
                  id="acc-kind"
                  className="w-full"
                  value={editing?.kind ?? "bank"}
                  onValueChange={(v) => setEditing((x) => ({ ...x, kind: v }))}
                  options={[
                    { value: "bank", label: "Bank account" },
                    { value: "cash", label: "Cash drawer" },
                  ]}
                />
              </Field>
            )}
            <Field>
              <FieldLabel htmlFor="acc-name">Name</FieldLabel>
              <Input
                id="acc-name"
                autoFocus
                required
                value={editing?.name ?? ""}
                onChange={(e) => setEditing((x) => ({ ...x, name: e.target.value }))}
              />
            </Field>
            {editing?.kind === "bank" && (
              <div className="grid grid-cols-2 gap-3">
                <Field>
                  <FieldLabel htmlFor="acc-bank">Bank</FieldLabel>
                  <Input
                    id="acc-bank"
                    value={editing.bankName ?? ""}
                    onChange={(e) => setEditing((x) => ({ ...x, bankName: e.target.value }))}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="acc-no">Account no.</FieldLabel>
                  <Input
                    id="acc-no"
                    value={editing.bankAccountNo ?? ""}
                    onChange={(e) => setEditing((x) => ({ ...x, bankAccountNo: e.target.value }))}
                  />
                </Field>
              </div>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setEditing(null)}>
                Cancel
              </Button>
              <Button type="submit" disabled={!editing?.name || save.isPending}>
                Save
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </Page>
  );
}

// ------------------------------------------------------------------------------------- day close

export function DayClosePage() {
  const money = useMoney();
  const settings = usePublicSettings();
  const queryClient = useQueryClient();
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const [day, setDay] = useState(today);
  const [notes, setNotes] = useState("");
  const summary = useList("day_summary", { businessDate: day });
  const closes = useCommand("day_closes_list", {});
  const close = useAction("day_close", {
    invalidates: ["day_summary", "day_closes_list"],
    success: "Day closed",
  });
  return (
    <Page
      title="Day close"
      description="Check the day's figures against the tills, then close the day."
    >
      <Section>
        <Field className="max-w-xs">
          <FieldLabel htmlFor="dc-day">Day</FieldLabel>
          <DatePicker id="dc-day" value={day} max={today} onChange={(v) => v && setDay(v)} />
        </Field>
      </Section>
      <QueryView query={summary}>
        {(s) => (
          <>
            <Section
              title={
                s.closedAt
                  ? `Closed ${showDateTime(s.closedAt)} by ${s.closedBy ?? ""}`
                  : "Today so far"
              }
            >
              <Facts
                items={[
                  ["Bills", s.bills],
                  ["Sales", money(s.salesTotal)],
                  ["Tax collected", money(s.taxTotal)],
                  ["Discounts given", money(s.discountTotal)],
                  ["Returns", money(s.returnsTotal)],
                  ["Purchases", money(s.purchasesTotal)],
                  ["Expenses", money(s.expensesTotal)],
                  ["Tills still open", s.openTills],
                ]}
              />
            </Section>
            <div className="grid gap-4 md:grid-cols-2">
              <Section title="Money received">
                <ModeList items={s.received} />
              </Section>
              <Section title="Money paid out">
                <ModeList items={s.paid} />
              </Section>
            </div>
            <Section title="Tills counted today">
              {s.tills.length === 0 ? (
                <p className="text-sm text-muted-foreground">No till was closed on this day.</p>
              ) : (
                <table className="w-full text-sm">
                  <thead className="text-xs text-muted-foreground">
                    <tr>
                      <th className="py-1 text-left font-medium">Till</th>
                      <th className="py-1 text-left font-medium">Counted by</th>
                      <th className="py-1 text-right font-medium">Expected</th>
                      <th className="py-1 text-right font-medium">Counted</th>
                      <th className="py-1 text-right font-medium">Difference</th>
                    </tr>
                  </thead>
                  <tbody>
                    {s.tills.map((t, i) => (
                      <tr key={i} className="border-t border-border/40">
                        <td className="py-1.5 font-medium">{t.register}</td>
                        <td>{t.countedBy}</td>
                        <td className="text-right tabular-nums">{money(t.expectedCash)}</td>
                        <td className="text-right tabular-nums">{money(t.countedCash)}</td>
                        <td
                          className={`text-right tabular-nums ${t.difference !== 0 ? "font-medium text-destructive" : ""}`}
                        >
                          {money(t.difference)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </Section>
            {!s.closedAt && (
              <Section>
                <div className="flex flex-wrap items-end gap-3">
                  <Field className="min-w-64 flex-1">
                    <FieldLabel htmlFor="dc-notes">Note (optional)</FieldLabel>
                    <Textarea
                      id="dc-notes"
                      className="min-h-10"
                      value={notes}
                      onChange={(e) => setNotes(e.target.value)}
                    />
                  </Field>
                  <Button
                    disabled={s.openTills > 0 || close.isPending}
                    onClick={run(async () => {
                      const ok = await confirm({
                        title: `Close ${showDate(day)}?`,
                        description: "The day's figures are saved as they are now.",
                        confirmLabel: "Close the day",
                      });
                      if (ok !== false)
                        close.mutate(
                          { input: { businessDate: day, notes: notes || null } },
                          {
                            onSuccess: () =>
                              void queryClient.invalidateQueries({ queryKey: ["day_summary"] }),
                          },
                        );
                    })}
                  >
                    Close the day
                  </Button>
                </div>
                {s.openTills > 0 && (
                  <p className="mt-2 text-sm text-warning">
                    Close every till first (Point of sale → Close till).
                  </p>
                )}
              </Section>
            )}
          </>
        )}
      </QueryView>
      <Section title="Earlier days">
        <QueryView query={closes}>
          {(list) =>
            list.length === 0 ? (
              <p className="text-sm text-muted-foreground">No day has been closed yet.</p>
            ) : (
              <table className="w-full text-sm">
                <tbody>
                  {list.map((c) => (
                    <tr
                      key={c.businessDate}
                      className="cursor-pointer border-t border-border/40 first:border-0 hover:bg-muted/40"
                      onClick={() => setDay(c.businessDate)}
                    >
                      <td className="py-1.5 font-medium">{showDate(c.businessDate)}</td>
                      <td className="text-muted-foreground">{c.bills} bills</td>
                      <td className="text-right tabular-nums">{money(c.salesTotal)}</td>
                      <td className="text-right text-muted-foreground">{c.closedBy}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </QueryView>
      </Section>
    </Page>
  );
}

function ModeList({ items }: { items: Api.ModeAmount[] }) {
  const money = useMoney();
  if (items.length === 0) return <p className="text-sm text-muted-foreground">Nothing.</p>;
  return (
    <ul className="text-sm">
      {items.map((m) => (
        <li
          key={m.mode}
          className="flex justify-between border-t border-border/40 py-1.5 first:border-0"
        >
          <span>{label(m.mode)}</span>
          <span className="tabular-nums">{money(m.amount)}</span>
        </li>
      ))}
    </ul>
  );
}
