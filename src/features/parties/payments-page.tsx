import { useMemo, useState } from "react";
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
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { Facts, Page, QueryView } from "@/components/app/page";
import { useDebounced } from "@/components/app/pickers";
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
  const [direction, setDirection] = useState("");
  const [mode, setMode] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const term = useDebounced(search.trim(), 200);
  const query = useList("payments_list", {
    direction: direction || null,
    mode: mode || null,
    search: term || null,
    page,
    pageSize: 100,
  });
  const columns = useMemo<ColumnDef<Api.PaymentListItem>[]>(
    () => [
      {
        header: "Number",
        accessorKey: "docNo",
        cell: ({ row }) => <span className="font-medium">{row.original.docNo}</span>,
      },
      {
        header: "Date",
        accessorKey: "businessDate",
        cell: ({ row }) => showDate(row.original.businessDate),
      },
      { header: "Party", accessorFn: (r) => r.partyName ?? "" },
      { header: "Mode", accessorKey: "mode", cell: ({ row }) => label(row.original.mode) },
      {
        header: "Status",
        accessorKey: "status",
        cell: ({ row }) => <StatusBadge status={row.original.status} />,
      },
      {
        header: "Amount",
        accessorKey: "amount",
        meta: { align: "right" },
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
          onChange={(e) => {
            setSearch(e.target.value);
            setPage(1);
          }}
        />
        <NativeSelect
          aria-label="Direction"
          value={direction}
          onChange={(e) => {
            setDirection(e.target.value);
            setPage(1);
          }}
        >
          <NativeSelectOption value="">In and out</NativeSelectOption>
          <NativeSelectOption value="in">Received</NativeSelectOption>
          <NativeSelectOption value="out">Paid</NativeSelectOption>
        </NativeSelect>
        <NativeSelect
          aria-label="Mode"
          value={mode}
          onChange={(e) => {
            setMode(e.target.value);
            setPage(1);
          }}
        >
          <NativeSelectOption value="">All modes</NativeSelectOption>
          {["cash", "upi", "card", "bank_transfer", "cheque", "wallet", "other"].map((m) => (
            <NativeSelectOption key={m} value={m}>
              {label(m)}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>
      <DataTable
        viewKey="payments"
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) => setParams({ id: r.id })}
        paging={
          query.data ? { page, pageSize: 100, total: query.data.total, onPage: setPage } : undefined
        }
        empty={{
          title: "No payments",
          description: "Receive or pay from a customer's or supplier's page.",
        }}
      />
      {openId && <PaymentView id={openId} onClose={() => setParams({})} />}
    </Page>
  );
}
