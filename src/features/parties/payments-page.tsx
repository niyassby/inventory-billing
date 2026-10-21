import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import type { Api } from "@inventory/shared";
import { Ban } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { Facts, Page, QueryView } from "@/components/app/page";
import { useMoney } from "@/lib/business";
import { label, showDate } from "@/lib/format";
import { useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";

function PaymentView({ id, onClose }: { id: string; onClose: () => void }) {
  const money = useMoney();
  const canReceive = useCan("payments.receive");
  const canPay = useCan("payments.pay");
  const canVoid = canReceive || canPay;
  const payment = useCommand("payments_get", { id });
  const voidPayment = useAction("payments_void", {
    invalidates: [
      "payments_list",
      "payments_get",
      "parties_list",
      "parties_get",
      "sales_list_invoices",
    ],
    success: "Payment voided",
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <QueryView query={payment}>
          {(p) => (
            <>
              <DialogHeader>
                <DialogTitle className="flex items-center gap-2">
                  {p.docNo} <StatusBadge status={p.status} />
                </DialogTitle>
                <DialogDescription>
                  {p.direction === "in" ? "Received from" : "Paid to"} {p.partyName ?? "—"}
                </DialogDescription>
              </DialogHeader>
              <Facts
                items={[
                  ["Amount", money(p.amount)],
                  ["Mode", label(p.mode)],
                  ["Date", showDate(p.businessDate)],
                  ["Unallocated", money(p.unallocated)],
                ]}
              />
              {p.allocations.length > 0 && (
                <ul className="text-sm">
                  {p.allocations.map((a) => (
                    <li
                      key={`${a.targetType}:${a.targetId}`}
                      className="flex justify-between border-t border-border/40 py-1"
                    >
                      <span>{a.docNo ?? label(a.targetType)}</span>
                      <span className="tabular-nums">{money(a.amount)}</span>
                    </li>
                  ))}
                </ul>
              )}
              {canVoid && p.status !== "voided" && (
                <Button
                  variant="destructive"
                  className="w-fit"
                  onClick={run(async () => {
                    const reason = await confirm({
                      title: `Void ${p.docNo}?`,
                      description: "The bills it paid become unpaid again.",
                      confirmLabel: "Void payment",
                      destructive: true,
                      reason: { label: "Reason", required: true },
                    });
                    if (reason !== false) voidPayment.mutate({ input: { id: p.id, reason } });
                  })}
                >
                  <Ban /> Void
                </Button>
              )}
            </>
          )}
        </QueryView>
      </DialogContent>
    </Dialog>
  );
}

export default function PaymentsPage() {
  const money = useMoney();
  const [params, setParams] = useSearchParams();
  const table = useTableState("payments", {
    filters: { q: "", direction: "", mode: "", from: "", to: "" },
  });
  const [search, setSearch] = useSearchFilter(table);
  const query = useList("payments_list", {
    direction: table.filter("direction") || null,
    mode: table.filter("mode") || null,
    from: table.filter("from") || null,
    to: table.filter("to") || null,
    search: table.filter("q") || null,
    ...table.input,
  });
  const columns = useMemo<ColumnDef<Api.PaymentListItem>[]>(
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
      { header: "Party", accessorFn: (r) => r.partyName ?? "", meta: { sortKey: "party" } },
      {
        header: "Mode",
        accessorKey: "mode",
        meta: { sortKey: "mode" },
        cell: ({ row }) => label(row.original.mode),
      },
      {
        header: "Status",
        accessorKey: "status",
        meta: { sortKey: "status" },
        cell: ({ row }) => <StatusBadge status={row.original.status} />,
      },
      {
        header: "Amount",
        accessorKey: "amount",
        meta: { align: "right", sortKey: "amount" },
        cell: ({ row }) => (
          <span className={row.original.direction === "out" ? "text-destructive" : ""}>
            {row.original.direction === "out" ? "−" : ""}
            {money(row.original.amount)}
          </span>
        ),
      },
    ],
    [money],
  );
  const openId = params.get("id");
  return (
    <Page title="Payments" description="Money received from customers and paid to suppliers.">
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          aria-label="Search payments"
          placeholder="Number, party or reference"
          className="max-w-xs"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <SelectField
          aria-label="Direction"
          className="w-40"
          value={table.filter("direction")}
          onValueChange={(v) => table.setFilter("direction", v)}
          options={[
            { value: "", label: "In and out" },
            { value: "in", label: "Received" },
            { value: "out", label: "Paid" },
          ]}
        />
        <SelectField
          aria-label="Mode"
          className="w-40"
          value={table.filter("mode")}
          onValueChange={(v) => table.setFilter("mode", v)}
          options={[
            { value: "", label: "All modes" },
            ...["cash", "upi", "card", "bank_transfer", "cheque", "wallet", "other"].map((m) => ({
              value: m,
              label: label(m),
            })),
          ]}
        />
        <DatePicker
          aria-label="From"
          className="w-44"
          placeholder="From"
          clearable
          value={table.filter("from")}
          max={table.filter("to") || undefined}
          onChange={(v) => table.setFilter("from", v)}
        />
        <DatePicker
          aria-label="To"
          className="w-44"
          placeholder="To"
          clearable
          value={table.filter("to")}
          min={table.filter("from") || undefined}
          onChange={(v) => table.setFilter("to", v)}
        />
      </div>
      <DataTable
        viewKey="payments"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) =>
          setParams((current) => {
            const next = new URLSearchParams(current);
            next.set("id", r.id);
            return next;
          })
        }
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{
          title: "No payments",
          description: "Receive or pay from a customer's or supplier's page.",
        }}
      />
      {openId && (
        <PaymentView
          id={openId}
          onClose={() =>
            setParams((current) => {
              const next = new URLSearchParams(current);
              next.delete("id");
              return next;
            })
          }
        />
      )}
    </Page>
  );
}
