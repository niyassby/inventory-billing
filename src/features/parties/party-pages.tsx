import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { gstinStateCode, isValidGstin } from "@inventory/shared";
import { HandCoins, Pencil, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Textarea } from "@/components/ui/textarea";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput } from "@/components/app/inputs";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { useDebounced } from "@/components/app/pickers";
import { call, requestId } from "@/api/v2";
import { useMoney } from "@/lib/business";
import { label, showDate } from "@/lib/format";
import { errorText, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";

type PartyType = "customer" | "supplier";

// ------------------------------------------------------------------------------------- editor

interface PartyDraft {
  id: string | null;
  updatedAt: string | null;
  partyType: string;
  name: string;
  phone: string;
  email: string;
  gstin: string;
  stateCode: string;
  billingAddress: string;
  creditLimit: number | null;
  creditDays: string;
  openingBalance: number | null;
  notes: string;
  isActive: boolean;
}

function draftOf(p: Api.PartyDetail | null, partyType: PartyType): PartyDraft {
  return {
    id: p?.id ?? null,
    updatedAt: p?.updatedAt ?? null,
    partyType: p?.partyType ?? partyType,
    name: p?.name ?? "",
    phone: p?.phone ?? "",
    email: p?.email ?? "",
    gstin: p?.gstin ?? "",
    stateCode: p?.stateCode ?? "",
    billingAddress: p?.billingAddress ?? "",
    creditLimit: p?.creditLimit ?? null,
    creditDays: p?.creditDays != null ? String(p.creditDays) : "",
    openingBalance: null,
    notes: p?.notes ?? "",
    isActive: p?.isActive ?? true,
  };
}

export function PartyDialog({
  open,
  party,
  partyType,
  onClose,
  onSaved,
}: {
  open: boolean;
  party: Api.PartyDetail | null;
  partyType: PartyType;
  onClose: () => void;
  onSaved?: (p: Api.PartyDetail) => void;
}) {
  const queryClient = useQueryClient();
  const [d, setD] = useState<PartyDraft>(() => draftOf(party, partyType));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setD(draftOf(party, partyType));
      setError(null);
    }
  }, [open, party, partyType]);
  const gstinBad = d.gstin.trim() !== "" && !isValidGstin(d.gstin.trim().toUpperCase());
  const set = (change: Partial<PartyDraft>) => setD((x) => ({ ...x, ...change }));
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const gstin = d.gstin.trim().toUpperCase();
      const saved = await call("parties_save", {
        id: d.id,
        expectedUpdatedAt: d.updatedAt,
        partyType: d.partyType,
        name: d.name,
        phone: d.phone || null,
        email: d.email || null,
        gstin: gstin || null,
        stateCode: d.stateCode || (gstin ? gstinStateCode(gstin) : null),
        billingAddress: d.billingAddress || null,
        creditLimit: d.creditLimit,
        creditDays: d.creditDays ? Number(d.creditDays) : null,
        openingBalance: d.id ? null : d.openingBalance,
        notes: d.notes || null,
        isActive: d.isActive,
      });
      void queryClient.invalidateQueries({ queryKey: ["parties_list"] });
      void queryClient.invalidateQueries({ queryKey: ["parties_get"] });
      void queryClient.invalidateQueries({ queryKey: ["parties_statement"] });
      toast.success(d.id ? "Saved" : `${saved.name} added`);
      onSaved?.(saved);
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const who = partyType === "customer" ? "customer" : "supplier";
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <DialogHeader>
            <DialogTitle>{party ? `Edit ${party.name}` : `New ${who}`}</DialogTitle>
          </DialogHeader>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field className="sm:col-span-2">
              <FieldLabel htmlFor="p-name">Name</FieldLabel>
              <Input
                id="p-name"
                autoFocus
                required
                value={d.name}
                onChange={(e) => set({ name: e.target.value })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="p-phone">Phone</FieldLabel>
              <Input
                id="p-phone"
                inputMode="tel"
                value={d.phone}
                onChange={(e) => set({ phone: e.target.value })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="p-email">E-mail</FieldLabel>
              <Input
                id="p-email"
                type="email"
                value={d.email}
                onChange={(e) => set({ email: e.target.value })}
              />
            </Field>
            <Field data-invalid={gstinBad || undefined}>
              <FieldLabel htmlFor="p-gstin">GSTIN</FieldLabel>
              <Input
                id="p-gstin"
                className="uppercase"
                value={d.gstin}
                onChange={(e) => set({ gstin: e.target.value })}
              />
              {gstinBad && <FieldError>This GSTIN is not valid.</FieldError>}
            </Field>
            <Field>
              <FieldLabel htmlFor="p-type">Type</FieldLabel>
              <NativeSelect
                id="p-type"
                className="w-full"
                value={d.partyType}
                onChange={(e) => set({ partyType: e.target.value })}
              >
                <NativeSelectOption value="customer">Customer</NativeSelectOption>
                <NativeSelectOption value="supplier">Supplier</NativeSelectOption>
                <NativeSelectOption value="both">Both</NativeSelectOption>
              </NativeSelect>
            </Field>
            <Field className="sm:col-span-2">
              <FieldLabel htmlFor="p-address">Address</FieldLabel>
              <Textarea
                id="p-address"
                className="min-h-14"
                value={d.billingAddress}
                onChange={(e) => set({ billingAddress: e.target.value })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="p-limit">Credit limit</FieldLabel>
              <MoneyInput
                id="p-limit"
                placeholder="No limit"
                value={d.creditLimit}
                onValueChange={(creditLimit) => set({ creditLimit })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="p-days">Credit days</FieldLabel>
              <Input
                id="p-days"
                inputMode="numeric"
                value={d.creditDays}
                onChange={(e) => set({ creditDays: e.target.value.replace(/\D/g, "") })}
              />
            </Field>
            {!party && (
              <Field className="sm:col-span-2">
                <FieldLabel htmlFor="p-opening">
                  Opening balance ({partyType === "customer" ? "they owe you" : "you owe them"})
                </FieldLabel>
                <MoneyInput
                  id="p-opening"
                  value={d.openingBalance}
                  onValueChange={(openingBalance) => set({ openingBalance })}
                />
              </Field>
            )}
          </div>
          {party && (
            <label className="flex items-center gap-2 text-sm">
              <Checkbox checked={d.isActive} onCheckedChange={(isActive) => set({ isActive })} />{" "}
              Active
            </label>
          )}
          {error && (
            <p className="text-sm text-destructive" role="alert">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !d.name.trim() || gstinBad}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ------------------------------------------------------------------------------------- list

export function PartiesPage({ partyType }: { partyType: PartyType }) {
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("parties.manage");
  const [search, setSearch] = useState("");
  const [withBalanceOnly, setWithBalanceOnly] = useState(false);
  const [page, setPage] = useState(1);
  const [creating, setCreating] = useState(false);
  const term = useDebounced(search.trim(), 200);
  const query = useList("parties_list", {
    partyType,
    search: term || null,
    withBalanceOnly,
    page,
    pageSize: 100,
  });
  const title = partyType === "customer" ? "Customers" : "Suppliers";
  const columns = useMemo<ColumnDef<Api.PartyListItem>[]>(
    () => [
      {
        header: "Name",
        accessorKey: "name",
        cell: ({ row }) => <span className="font-medium">{row.original.name}</span>,
      },
      { header: "Phone", accessorFn: (r) => r.phone ?? "" },
      { header: "GSTIN", accessorFn: (r) => r.gstin ?? "" },
      {
        header: partyType === "customer" ? "They owe" : "You owe",
        accessorKey: "balance",
        meta: { align: "right" },
        cell: ({ row }) => {
          const b = partyType === "customer" ? row.original.balance : -row.original.balance;
          return (
            <span className={b > 0 ? "font-medium" : "text-muted-foreground"}>{money(b)}</span>
          );
        },
      },
      {
        header: "Credit limit",
        accessorFn: (r) => r.creditLimit ?? 0,
        meta: { align: "right" },
        cell: ({ row }) =>
          row.original.creditLimit != null ? money(row.original.creditLimit) : "—",
      },
    ],
    [money, partyType],
  );
  return (
    <Page
      title={title}
      actions={
        canManage && (
          <Button onClick={() => setCreating(true)}>
            <Plus /> New {partyType}
          </Button>
        )
      }
    >
      <div className="flex flex-wrap items-center gap-3">
        <Input
          aria-label={`Search ${title.toLowerCase()}`}
          placeholder="Search name, phone or GSTIN"
          className="max-w-xs"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setPage(1);
          }}
        />
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={withBalanceOnly}
            onCheckedChange={(v) => {
              setWithBalanceOnly(v);
              setPage(1);
            }}
          />
          With a balance only
        </label>
      </div>
      <DataTable
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) => void navigate(`/parties/${r.id}`)}
        paging={
          query.data ? { page, pageSize: 100, total: query.data.total, onPage: setPage } : undefined
        }
        empty={{ title: term ? "No one matches" : `No ${title.toLowerCase()} yet` }}
      />
      <PartyDialog
        open={creating}
        party={null}
        partyType={partyType}
        onClose={() => setCreating(false)}
        onSaved={(p) => void navigate(`/parties/${p.id}`)}
      />
    </Page>
  );
}

// ------------------------------------------------------------------------------------- payment

/** Receive from a customer or pay a supplier, allocated to open bills (oldest first by default). */
export function PaymentDialog({
  open,
  party,
  onClose,
}: {
  open: boolean;
  party: Api.PartyDetail;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const money = useMoney();
  const isCustomer = party.partyType !== "supplier";
  const owed = isCustomer ? party.balance : -party.balance;
  const [key, setKey] = useState(requestId);
  const [amount, setAmount] = useState<number | null>(null);
  const [mode, setMode] = useState("cash");
  const [reference, setReference] = useState("");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setKey(requestId());
      setAmount(owed > 0 ? owed : null);
      setReference("");
      setNotes("");
    }
  }, [open, owed]);
  const save = async () => {
    if (!amount) return;
    setBusy(true);
    try {
      const p = await call("payments_record", {
        idempotencyKey: key,
        partyId: party.id,
        direction: isCustomer ? "in" : "out",
        payment: { mode, amount, tendered: null, accountId: null, reference: reference || null },
        autoAllocate: true,
        notes: notes || null,
      });
      toast.success(`${p.docNo}: ${money(p.amount)} ${isCustomer ? "received" : "paid"}`);
      for (const k of [
        "parties_get",
        "parties_list",
        "parties_statement",
        "parties_open_documents",
        "payments_list",
        "sales_list_invoices",
      ]) {
        void queryClient.invalidateQueries({ queryKey: [k] });
      }
      onClose();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {isCustomer ? `Receive from ${party.name}` : `Pay ${party.name}`}
            </DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            {owed > 0
              ? `${isCustomer ? "They owe" : "You owe"} ${money(owed)}.`
              : "Nothing is outstanding; this will be an advance."}{" "}
            The amount is applied to the oldest open bills first.
          </p>
          <div className="grid grid-cols-2 gap-3">
            <Field>
              <FieldLabel htmlFor="pay-amount">Amount</FieldLabel>
              <MoneyInput id="pay-amount" autoFocus value={amount} onValueChange={setAmount} />
            </Field>
            <Field>
              <FieldLabel htmlFor="pay-mode">Mode</FieldLabel>
              <NativeSelect
                id="pay-mode"
                className="w-full"
                value={mode}
                onChange={(e) => setMode(e.target.value)}
              >
                {["cash", "upi", "card", "bank_transfer", "cheque", "wallet", "other"].map((m) => (
                  <NativeSelectOption key={m} value={m}>
                    {label(m)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </Field>
            <Field>
              <FieldLabel htmlFor="pay-ref">Reference</FieldLabel>
              <Input
                id="pay-ref"
                placeholder="Cheque / UTR no."
                value={reference}
                onChange={(e) => setReference(e.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="pay-notes">Note</FieldLabel>
              <Input id="pay-notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
            </Field>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !amount}>
              {isCustomer ? "Receive" : "Pay"} {amount ? money(amount) : ""}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ------------------------------------------------------------------------------------- detail

export function PartyPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const money = useMoney();
  const queryClient = useQueryClient();
  const canManage = useCan("parties.manage");
  const canReceive = useCan("payments.receive");
  const canPay = useCan("payments.pay");
  const party = useCommand("parties_get", { id });
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const statement = useList("parties_statement", {
    partyId: id,
    from: from || null,
    to: to || null,
  });
  const open = useCommand("parties_open_documents", { id });
  const [editing, setEditing] = useState(false);
  const [paying, setPaying] = useState(false);
  const docLink = (type: string, docId: string): string | null =>
    ({
      sales_invoice: `/sales/invoices/${docId}`,
      sales_return: `/sales/returns/${docId}`,
      purchase_bill: `/purchases/bills/${docId}`,
      purchase_return: `/purchases/returns/${docId}`,
      payment: `/payments?id=${docId}`,
    })[type] ?? null;
  return (
    <QueryView query={party}>
      {(p) => {
        const isCustomer = p.partyType !== "supplier";
        const owed = isCustomer ? p.balance : -p.balance;
        return (
          <Page
            title={p.name}
            back={{
              to: isCustomer ? "/parties/customers" : "/parties/suppliers",
              label: isCustomer ? "Customers" : "Suppliers",
            }}
            actions={
              <>
                {canManage && (
                  <Button variant="outline" onClick={() => setEditing(true)}>
                    <Pencil /> Edit
                  </Button>
                )}
                {canManage && p.balance === 0 && (
                  <Button
                    variant="outline"
                    onClick={run(async () => {
                      const ok = await confirm({
                        title: `Delete ${p.name}?`,
                        confirmLabel: "Delete",
                        destructive: true,
                      });
                      if (ok === false) return;
                      try {
                        await call("parties_delete", { id: p.id });
                        void queryClient.invalidateQueries({ queryKey: ["parties_list"] });
                        void navigate(isCustomer ? "/parties/customers" : "/parties/suppliers");
                      } catch (e) {
                        toast.error(errorText(e));
                      }
                    })}
                  >
                    <Trash2 /> Delete
                  </Button>
                )}
                {(isCustomer ? canReceive : canPay) && (
                  <Button onClick={() => setPaying(true)}>
                    <HandCoins /> {isCustomer ? "Receive payment" : "Pay"}
                  </Button>
                )}
              </>
            }
          >
            <Section>
              <Facts
                items={[
                  [
                    isCustomer ? "They owe" : "You owe",
                    <span key="b" className="text-lg">
                      {money(owed)}
                    </span>,
                  ],
                  ["Phone", p.phone],
                  ["GSTIN", p.gstin],
                  ["Credit limit", p.creditLimit != null ? money(p.creditLimit) : "No limit"],
                ]}
              />
              {p.billingAddress && (
                <p className="mt-3 text-sm text-muted-foreground">{p.billingAddress}</p>
              )}
            </Section>
            <Section title="Open bills">
              <QueryView query={open}>
                {(docs) =>
                  docs.length === 0 ? (
                    <p className="text-sm text-muted-foreground">Nothing outstanding.</p>
                  ) : (
                    <table className="w-full text-sm">
                      <tbody>
                        {docs.map((d) => {
                          const href = docLink(d.targetType, d.id);
                          return (
                            <tr
                              key={d.id}
                              className="cursor-pointer border-t border-border/40 first:border-0 hover:bg-muted/40"
                              onClick={() => href && void navigate(href)}
                            >
                              <td className="py-1.5 font-medium">{d.docNo}</td>
                              <td>{showDate(d.businessDate)}</td>
                              <td className="text-right tabular-nums">{money(d.total)}</td>
                              <td className="text-right font-medium tabular-nums">
                                {money(d.balanceDue)} due
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  )
                }
              </QueryView>
            </Section>
            <Section
              title="Statement"
              actions={
                <div className="flex items-center gap-2 text-sm">
                  <Input
                    type="date"
                    aria-label="From"
                    className="h-8 w-36"
                    value={from}
                    onChange={(e) => setFrom(e.target.value)}
                  />
                  <span>–</span>
                  <Input
                    type="date"
                    aria-label="To"
                    className="h-8 w-36"
                    value={to}
                    onChange={(e) => setTo(e.target.value)}
                  />
                </div>
              }
            >
              <QueryView query={statement}>
                {(s) => (
                  <table className="w-full text-sm">
                    <thead className="text-xs text-muted-foreground">
                      <tr>
                        <th className="py-1 text-left font-medium">Date</th>
                        <th className="py-1 text-left font-medium">Entry</th>
                        <th className="py-1 text-left font-medium">Document</th>
                        <th className="py-1 text-right font-medium">Debit</th>
                        <th className="py-1 text-right font-medium">Credit</th>
                        <th className="py-1 text-right font-medium">Balance</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr className="text-muted-foreground">
                        <td colSpan={5} className="py-1">
                          Opening balance
                        </td>
                        <td className="text-right tabular-nums">{money(s.openingBalance)}</td>
                      </tr>
                      {s.lines.map((l, i) => {
                        const href = docLink(l.sourceType, l.sourceId);
                        return (
                          <tr key={i} className="border-t border-border/40">
                            <td className="py-1.5">{showDate(l.businessDate)}</td>
                            <td>{label(l.entryType)}</td>
                            <td>
                              {href ? (
                                <button
                                  type="button"
                                  className="hover:underline"
                                  onClick={() => void navigate(href)}
                                >
                                  {l.docNo ?? "—"}
                                </button>
                              ) : (
                                (l.docNo ?? l.narration ?? "—")
                              )}
                            </td>
                            <td className="text-right tabular-nums">
                              {l.debit ? money(l.debit) : ""}
                            </td>
                            <td className="text-right tabular-nums">
                              {l.credit ? money(l.credit) : ""}
                            </td>
                            <td className="text-right font-medium tabular-nums">
                              {money(l.balance)}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                )}
              </QueryView>
            </Section>
            <PartyDialog
              open={editing}
              party={p}
              partyType={isCustomer ? "customer" : "supplier"}
              onClose={() => setEditing(false)}
            />
            <PaymentDialog open={paying} party={p} onClose={() => setPaying(false)} />
          </Page>
        );
      }}
    </QueryView>
  );
}
