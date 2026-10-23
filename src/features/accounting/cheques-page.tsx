import { useMemo, useState } from "react";
import type { Api } from "@inventory/shared";
import { businessDate } from "@inventory/shared";
import { Ban, CheckCheck, Plus, Undo2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
import { Page } from "@/components/app/page";
import { PartyPicker } from "@/components/app/pickers";
import { requestId } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showDate } from "@/lib/format";
import { fieldError, useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useTableState } from "@/lib/table-state";
import { useCan } from "@/stores/session";

const REFRESH = ["cheques_list", "payments_list", "parties_get", "parties_list"] as const;

/** Post-dated cheques: received or given, waiting for their date, then cleared or bounced. */
export default function ChequesPage() {
  const money = useMoney();
  const settings = usePublicSettings();
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const canReceive = useCan("payments.receive");
  const canPay = useCan("payments.pay");
  const table = useTableState("cheques", {
    filters: { status: "pending", direction: "", due: "" },
  });
  const [adding, setAdding] = useState(false);
  const [clearing, setClearing] = useState<Api.Cheque | null>(null);
  const query = useList("cheques_list", {
    status: table.filter("status") || null,
    direction: table.filter("direction") || null,
    dueBy: table.filter("due") === "due" ? today : null,
    ...table.input,
  });
  const finish = useAction("cheques_bounce", {
    invalidates: [...REFRESH],
    success: "Cheque marked bounced",
  });
  const cancel = useAction("cheques_cancel", {
    invalidates: [...REFRESH],
    success: "Cheque cancelled",
  });
  const columns = useMemo<ColumnDef<Api.Cheque>[]>(
    () => [
      {
        header: "Date on cheque",
        accessorKey: "chequeDate",
        meta: { sortKey: "date" },
        cell: ({ row }) => (
          <span className="flex flex-col">
            {showDate(row.original.chequeDate)}
            {row.original.status === "pending" && (
              <span className="text-xs">
                {row.original.daysLeft > 0 ? (
                  <span className="text-muted-foreground">in {row.original.daysLeft} days</span>
                ) : (
                  <Badge variant="warning">
                    Due
                    {row.original.daysLeft < 0 ? ` ${-row.original.daysLeft} days ago` : " today"}
                  </Badge>
                )}
              </span>
            )}
          </span>
        ),
      },
      {
        header: "Cheque",
        accessorKey: "chequeNo",
        cell: ({ row }) => (
          <span className="flex flex-col">
            <span className="font-medium">
              {row.original.direction === "in" ? "From " : "To "}
              {row.original.partyName}
            </span>
            <span className="text-xs text-muted-foreground">
              No. {row.original.chequeNo}
              {row.original.bankName && ` · ${row.original.bankName}`} · {row.original.accountName}
            </span>
          </span>
        ),
      },
      {
        header: "Status",
        accessorKey: "status",
        meta: { sortKey: "status" },
        cell: ({ row }) => (
          <span className="flex flex-col gap-0.5">
            <StatusBadge status={row.original.status} />
            {row.original.paymentDocNo && (
              <span className="text-xs text-muted-foreground">{row.original.paymentDocNo}</span>
            )}
            {row.original.statusNote && (
              <span className="text-xs text-muted-foreground">{row.original.statusNote}</span>
            )}
          </span>
        ),
      },
      {
        header: "Amount",
        accessorKey: "amount",
        meta: { align: "right", sortKey: "amount" },
        cell: ({ row }) => money(row.original.amount),
      },
      {
        header: "",
        id: "actions",
        enableSorting: false,
        cell: ({ row }) => {
          const c = row.original;
          const allowed = c.direction === "in" ? canReceive : canPay;
          if (!allowed) return null;
          return (
            <span className="flex justify-end gap-1">
              {c.status === "pending" && (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={c.daysLeft > 0}
                  onClick={() => setClearing(c)}
                >
                  <CheckCheck /> Cleared
                </Button>
              )}
              {(c.status === "pending" || c.status === "cleared") && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={run(async () => {
                    const reason = await confirm({
                      title: `Cheque ${c.chequeNo} bounced?`,
                      description:
                        c.status === "cleared"
                          ? `Its payment ${c.paymentDocNo ?? ""} is voided: ${c.partyName} owes the amount again.`
                          : "It is marked bounced; nothing was booked for it.",
                      confirmLabel: "Mark bounced",
                      destructive: true,
                      reason: { label: "Bank's reason", required: true },
                    });
                    if (reason !== false) finish.mutate({ input: { id: c.id, reason } });
                  })}
                >
                  <Undo2 /> Bounced
                </Button>
              )}
              {c.status === "pending" && (
                <Button
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Cancel cheque ${c.chequeNo}`}
                  onClick={run(async () => {
                    const reason = await confirm({
                      title: `Cancel cheque ${c.chequeNo}?`,
                      description: "Returned, replaced or lost: it won't be deposited.",
                      confirmLabel: "Cancel cheque",
                      destructive: true,
                      reason: { label: "Reason", required: true },
                    });
                    if (reason !== false) cancel.mutate({ input: { id: c.id, reason } });
                  })}
                >
                  <Ban />
                </Button>
              )}
            </span>
          );
        },
      },
    ],
    [money, canReceive, canPay, finish, cancel],
  );
  return (
    <Page
      title="Cheques"
      description="Post-dated cheques you received or gave. A cheque is booked as a payment only when it clears."
      actions={
        (canReceive || canPay) && (
          <Button onClick={() => setAdding(true)}>
            <Plus /> Record a cheque
          </Button>
        )
      }
    >
      <div className="flex flex-wrap gap-3">
        <SelectField
          aria-label="Status"
          className="w-44"
          value={table.filter("status")}
          onValueChange={(v) => table.setFilter("status", v)}
          options={[
            { value: "pending", label: "Pending" },
            { value: "cleared", label: "Cleared" },
            { value: "bounced", label: "Bounced" },
            { value: "cancelled", label: "Cancelled" },
            { value: "", label: "All cheques" },
          ]}
        />
        <SelectField
          aria-label="Direction"
          className="w-44"
          value={table.filter("direction")}
          onValueChange={(v) => table.setFilter("direction", v)}
          options={[
            { value: "", label: "Received and given" },
            { value: "in", label: "Received" },
            { value: "out", label: "Given" },
          ]}
        />
        <SelectField
          aria-label="Due"
          className="w-44"
          value={table.filter("due")}
          onValueChange={(v) => table.setFilter("due", v)}
          options={[
            { value: "", label: "Any date" },
            { value: "due", label: "Due now" },
          ]}
        />
      </div>
      <DataTable
        viewKey="cheques"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{
          title: "No cheques here",
          description: "Record a post-dated cheque when you get or give one.",
        }}
      />
      {adding && <ChequeDialog onClose={() => setAdding(false)} />}
      {clearing && <ClearDialog cheque={clearing} onClose={() => setClearing(null)} />}
    </Page>
  );
}

function ChequeDialog({ onClose }: { onClose: () => void }) {
  const canReceive = useCan("payments.receive");
  const accounts = useCommand("payments_accounts", {});
  const banks = (accounts.data ?? []).filter((a) => a.subtype === "bank");
  const [key] = useState(requestId);
  const [direction, setDirection] = useState<"in" | "out">(canReceive ? "in" : "out");
  const [party, setParty] = useState<{ id: string; name: string } | null>(null);
  const [accountId, setAccountId] = useState("");
  const [chequeNo, setChequeNo] = useState("");
  const [bankName, setBankName] = useState("");
  const [chequeDate, setChequeDate] = useState("");
  const [amount, setAmount] = useState<number | null>(null);
  const save = useAction("cheques_save", {
    invalidates: [...REFRESH],
    success: (c) => `Cheque ${c.chequeNo} recorded`,
    onSuccess: onClose,
  });
  const account = accountId || banks[0]?.id || "";
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Record a cheque</DialogTitle>
          <DialogDescription>Nothing is booked until it clears.</DialogDescription>
        </DialogHeader>
        <form
          id="cheque-form"
          className="grid gap-4 sm:grid-cols-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!party || amount === null) return;
            save.mutate({
              input: {
                id: null,
                expectedUpdatedAt: null,
                idempotencyKey: key,
                direction,
                partyId: party.id,
                accountId: account,
                chequeNo,
                bankName: bankName || null,
                chequeDate,
                amount,
                notes: null,
              },
            });
          }}
        >
          <Field>
            <FieldLabel htmlFor="chq-direction">Cheque</FieldLabel>
            <SelectField
              id="chq-direction"
              value={direction}
              onValueChange={(v) => {
                setDirection(v as "in" | "out");
                setParty(null);
              }}
              options={[
                { value: "in", label: "Received from a customer" },
                { value: "out", label: "Given to a supplier" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel>{direction === "in" ? "Customer" : "Supplier"}</FieldLabel>
            {party ? (
              <span className="flex h-9 items-center gap-2 text-sm">
                <span className="truncate font-medium">{party.name}</span>
                <Button size="xs" variant="ghost" type="button" onClick={() => setParty(null)}>
                  Change
                </Button>
              </span>
            ) : (
              <PartyPicker
                partyType={direction === "in" ? "customer" : "supplier"}
                onSelect={(p) => setParty({ id: p.id, name: p.name })}
              />
            )}
          </Field>
          <Field>
            <FieldLabel htmlFor="chq-no">Cheque number</FieldLabel>
            <Input
              id="chq-no"
              value={chequeNo}
              maxLength={20}
              onChange={(e) => setChequeNo(e.target.value)}
            />
            <span className="text-xs text-danger">{fieldError(save.error, "chequeNo")}</span>
          </Field>
          <Field>
            <FieldLabel htmlFor="chq-bank">Drawn on (bank)</FieldLabel>
            <Input
              id="chq-bank"
              value={bankName}
              maxLength={80}
              onChange={(e) => setBankName(e.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="chq-date">Date on the cheque</FieldLabel>
            <DatePicker id="chq-date" value={chequeDate} onChange={setChequeDate} />
          </Field>
          <Field>
            <FieldLabel htmlFor="chq-amount">Amount</FieldLabel>
            <MoneyInput id="chq-amount" value={amount} onValueChange={setAmount} />
          </Field>
          <Field className="sm:col-span-2">
            <FieldLabel htmlFor="chq-account">
              {direction === "in" ? "Deposit into" : "Paid from"}
            </FieldLabel>
            <SelectField
              id="chq-account"
              value={account}
              onValueChange={setAccountId}
              options={banks.map((a) => ({ value: a.id, label: a.name }))}
            />
          </Field>
        </form>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="cheque-form"
            disabled={
              save.isPending ||
              !party ||
              amount === null ||
              !chequeDate ||
              !chequeNo.trim() ||
              !account
            }
          >
            Record cheque
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ClearDialog({ cheque, onClose }: { cheque: Api.Cheque; onClose: () => void }) {
  const money = useMoney();
  const settings = usePublicSettings();
  const today = businessDate(new Date(), settings.data?.business.timezone ?? "Asia/Kolkata");
  const [day, setDay] = useState(today);
  const [allocate, setAllocate] = useState(true);
  const clear = useAction("cheques_clear", {
    invalidates: [...REFRESH],
    success: (c) => `Cheque cleared as ${c.paymentDocNo ?? "a payment"}`,
    onSuccess: onClose,
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Cheque {cheque.chequeNo} cleared</DialogTitle>
          <DialogDescription>
            {cheque.direction === "in" ? "A receipt from" : "A payment to"} {cheque.partyName} of{" "}
            {money(cheque.amount)} is recorded through {cheque.accountName}.
          </DialogDescription>
        </DialogHeader>
        <Field>
          <FieldLabel htmlFor="clr-date">Cleared on</FieldLabel>
          <DatePicker
            id="clr-date"
            value={day}
            min={cheque.chequeDate}
            max={today}
            onChange={(v) => v && setDay(v)}
          />
        </Field>
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={allocate} onCheckedChange={setAllocate} />
          Apply it to the oldest open bills
        </label>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={clear.isPending}
            onClick={() =>
              clear.mutate({ input: { id: cheque.id, clearedDate: day, autoAllocate: allocate } })
            }
          >
            Record the payment
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
