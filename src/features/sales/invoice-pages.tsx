import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { Ban, Printer, Undo2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { OfflineBadge, StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput, QtyInput } from "@/components/app/inputs";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { Timeline } from "@/components/app/timeline";
import { call, requestId } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { label, showDate, showDateTime, showQty } from "@/lib/format";
import { errorText, useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";
import type { Paper } from "./print";
import { defaultPaper, documentHtml, printHtml, returnSlipHtml } from "./print";
import { printDocument } from "@/features/printing/print-service";
import { invoiceEvents } from "./timeline";

// ------------------------------------------------------------------------------------- invoices

export function InvoicesPage() {
  const navigate = useNavigate();
  const money = useMoney();
  const table = useTableState("invoices", {
    filters: { q: "", status: "", from: "", to: "" },
  });
  const [search, setSearch] = useSearchFilter(table);
  const query = useList("sales_list_invoices", {
    search: table.filter("q") || null,
    paymentStatus: table.filter("status") || null,
    from: table.filter("from") || null,
    to: table.filter("to") || null,
    ...table.input,
  });
  const columns = useMemo<ColumnDef<Api.InvoiceListItem>[]>(
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
      { header: "Customer", accessorKey: "customerName", meta: { sortKey: "customer" } },
      {
        header: "Status",
        meta: { sortKey: "status" },
        accessorKey: "paymentStatus",
        cell: ({ row }) => (
          <StatusBadge
            status={row.original.status === "voided" ? "voided" : row.original.paymentStatus}
          />
        ),
      },
      {
        header: "Total",
        accessorKey: "total",
        meta: { align: "right", sortKey: "total" },
        cell: ({ row }) => money(row.original.total),
      },
      {
        header: "Due",
        accessorKey: "balanceDue",
        meta: { align: "right", sortKey: "due" },
        cell: ({ row }) =>
          row.original.balanceDue > 0 && row.original.status !== "voided"
            ? money(row.original.balanceDue)
            : "",
      },
    ],
    [money],
  );
  return (
    <Page
      title="Invoices"
      actions={
        <Button render={<Link to="/pos" />} nativeButton={false}>
          New sale
        </Button>
      }
    >
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          aria-label="Search invoices"
          placeholder="Number, customer or phone"
          className="max-w-xs"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <SelectField
          aria-label="Payment status"
          className="w-44"
          value={table.filter("status")}
          onValueChange={(v) => table.setFilter("status", v)}
          options={[
            { value: "", label: "Paid and unpaid" },
            { value: "unpaid", label: "Unpaid" },
            { value: "partial", label: "Part paid" },
            { value: "paid", label: "Paid" },
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
        viewKey="invoices"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) => void navigate(`/sales/invoices/${r.id}`)}
        paging={
          query.data
            ? {
                page: table.state.page,
                pageSize: table.state.size,
                total: query.data.total,
                onPage: table.setPage,
                onPageSize: table.setSize,
              }
            : undefined
        }
        empty={{ title: "No invoices", description: "Bills made at the till appear here." }}
      />
    </Page>
  );
}

export function InvoiceLines({ invoice }: { invoice: Api.InvoiceDetail }) {
  const money = useMoney();
  return (
    <table className="w-full text-sm">
      <thead className="text-xs text-muted-foreground">
        <tr>
          <th className="py-1 text-left font-medium">Item</th>
          <th className="py-1 text-right font-medium">Qty</th>
          <th className="py-1 text-right font-medium">Price</th>
          <th className="py-1 text-right font-medium">Discount</th>
          <th className="py-1 text-right font-medium">Tax</th>
          <th className="py-1 text-right font-medium">Amount</th>
        </tr>
      </thead>
      <tbody>
        {invoice.lines.map((l) => (
          <tr key={l.id} className="border-t border-border/40">
            <td className="py-1.5">
              <span className="font-medium">{l.itemName}</span>
              {l.returnedQty > 0 && (
                <span className="ml-2 text-xs text-warning">{showQty(l.returnedQty)} returned</span>
              )}
              {(l.batches ?? []).length > 0 && (
                <span className="block text-xs text-muted-foreground">
                  {(l.batches ?? [])
                    .map(
                      (b) =>
                        `Batch ${b.batchNo}${b.expiryDate ? ` exp ${showDate(b.expiryDate)}` : ""}`,
                    )
                    .join(" · ")}
                </span>
              )}
            </td>
            <td className="text-right tabular-nums">
              {showQty(l.qty)} {l.unitName}
            </td>
            <td className="text-right tabular-nums">{money(l.unitPrice)}</td>
            <td className="text-right tabular-nums">
              {l.lineDiscount + l.billDiscountShare
                ? money(l.lineDiscount + l.billDiscountShare)
                : ""}
            </td>
            <td className="text-right tabular-nums">{money(l.taxTotal)}</td>
            <td className="text-right font-medium tabular-nums">{money(l.lineTotal)}</td>
          </tr>
        ))}
      </tbody>
      <tfoot className="text-sm">
        {(
          [
            ["Taxable value", invoice.taxable],
            ["Tax", invoice.taxTotal],
            ...(invoice.roundOff ? [["Round off", invoice.roundOff] as [string, number]] : []),
          ] as [string, number][]
        ).map(([k, v]) => (
          <tr key={k}>
            <td colSpan={5} className="pt-1 text-right text-muted-foreground">
              {k}
            </td>
            <td className="pt-1 text-right tabular-nums">{money(v)}</td>
          </tr>
        ))}
        <tr>
          <td colSpan={5} className="pt-1 text-right font-semibold">
            Total
          </td>
          <td className="pt-1 text-right text-lg font-semibold tabular-nums">
            {money(invoice.total)}
          </td>
        </tr>
      </tfoot>
    </table>
  );
}

export function PrintButtons({ invoice }: { invoice: Api.InvoiceDetail }) {
  const settings = usePublicSettings();
  const [paper, setPaper] = useState<Paper>(defaultPaper(settings.data));
  return (
    <div className="flex items-center gap-1">
      <SelectField
        aria-label="Paper"
        size="sm"
        value={paper}
        onValueChange={(v) => setPaper(v as Paper)}
        options={[
          { value: "thermal_80", label: "80 mm" },
          { value: "thermal_58", label: "58 mm" },
          { value: "a4", label: "A4 / PDF" },
        ]}
      />
      <Button
        variant="outline"
        onClick={() =>
          // A4 goes through the print dialog; a receipt to this PC's receipt printer if it has one.
          paper === "a4"
            ? printHtml(documentHtml(invoice, settings.data, paper))
            : void printDocument({
                document: { type: "receipt", invoice, reprint: true },
                fallbackHtml: () => documentHtml(invoice, settings.data, paper),
              })
        }
      >
        <Printer /> Print
      </Button>
    </div>
  );
}

const TIMELINE_TITLE = {
  issued: "Invoice issued",
  payment: "Payment received",
  payment_voided: "Payment refunded",
  return: "Items returned",
  voided: "Invoice voided",
} as const;

export function InvoicePage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const money = useMoney();
  const canReturn = useCan("sales.return");
  const invoice = useCommand("sales_get_invoice", { id });
  const returns = useCommand("sales_invoice_returns", { id });
  const voidInvoice = useAction("sales_void_invoice", {
    invalidates: ["sales_get_invoice", "sales_list_invoices", "parties_get", "products_list"],
    success: "Invoice voided",
  });
  return (
    <QueryView query={invoice}>
      {(inv) => {
        const voided = inv.status === "voided";
        const returnable = inv.lines.some((l) => l.qty > l.returnedQty);
        return (
          <Page
            title={inv.docNo}
            description={
              <span className="flex items-center gap-2">
                <StatusBadge status={voided ? "voided" : inv.paymentStatus} />
                {inv.origin === "offline" && <OfflineBadge />}
              </span>
            }
            back={{ to: "/sales/invoices", label: "Invoices" }}
            actions={
              <>
                <PrintButtons invoice={inv} />
                {!voided && canReturn && returnable && (
                  <Button
                    variant="outline"
                    onClick={() => void navigate(`/sales/returns/new?invoice=${inv.id}`)}
                  >
                    <Undo2 /> Return items
                  </Button>
                )}
                {!voided && (
                  <Button
                    variant="destructive"
                    onClick={run(async () => {
                      const reason = await confirm({
                        title: `Void ${inv.docNo}?`,
                        description:
                          "Stock comes back, the customer's balance and the books are reversed, and payments are refunded. The number stays used.",
                        confirmLabel: "Void invoice",
                        destructive: true,
                        reason: { label: "Reason", required: true },
                      });
                      if (reason !== false) voidInvoice.mutate({ input: { id: inv.id, reason } });
                    })}
                  >
                    <Ban /> Void
                  </Button>
                )}
              </>
            }
          >
            <Section>
              <Facts
                items={[
                  ["Date", showDate(inv.businessDate)],
                  [
                    "Customer",
                    inv.partyId ? (
                      <Link key="c" to={`/parties/${inv.partyId}`} className="hover:underline">
                        {inv.customerName}
                      </Link>
                    ) : (
                      inv.customerName
                    ),
                  ],
                  ["Paid", money(inv.amountPaid)],
                  ["Due", money(inv.balanceDue)],
                ]}
              />
              {voided && <p className="mt-3 text-sm text-destructive">Voided: {inv.voidReason}</p>}
              {inv.notes && <p className="mt-3 text-sm text-muted-foreground">{inv.notes}</p>}
            </Section>
            <div className="grid gap-4 lg:grid-cols-3">
              <Section title="Items" className="lg:col-span-2">
                <InvoiceLines invoice={inv} />
              </Section>
              <Section title="Timeline">
                <Timeline
                  events={invoiceEvents(inv, returns.data ?? []).map((e) => ({
                    key: e.key,
                    tone:
                      e.kind === "voided" || e.kind === "payment_voided"
                        ? "danger"
                        : e.kind === "payment"
                          ? "success"
                          : e.kind === "return"
                            ? "warning"
                            : "neutral",
                    title: TIMELINE_TITLE[e.kind],
                    when: e.kind === "issued" ? showDateTime(e.at) : showDate(e.at),
                    detail: [e.amount !== undefined ? money(e.amount) : null, e.text]
                      .filter(Boolean)
                      .join(" · "),
                  }))}
                />
              </Section>
            </div>
            {inv.payments.length > 0 && (
              <Section title="Payments">
                <ul className="text-sm">
                  {inv.payments.map((p) => (
                    <li
                      key={p.id}
                      className="flex justify-between gap-3 border-t border-border/40 py-1.5 first:border-0"
                    >
                      <span>
                        {p.docNo || "—"} · {label(p.mode)} · {showDate(p.businessDate)}
                        {p.changeGiven > 0 && (
                          <span className="text-muted-foreground">
                            {" "}
                            · change {money(p.changeGiven)}
                          </span>
                        )}
                      </span>
                      <span className="flex items-center gap-2 tabular-nums">
                        {p.status === "voided" && <StatusBadge status="voided" />}
                        {money(p.amount)}
                      </span>
                    </li>
                  ))}
                </ul>
              </Section>
            )}
            {returns.data && returns.data.length > 0 && (
              <Section title="Returns">
                <ul className="text-sm">
                  {returns.data.map((r) => (
                    <li
                      key={r.id}
                      className="flex justify-between border-t border-border/40 py-1.5 first:border-0"
                    >
                      <Link to={`/sales/returns/${r.id}`} className="hover:underline">
                        {r.docNo} · {showDate(r.businessDate)}
                      </Link>
                      <span className="tabular-nums">{money(r.total)}</span>
                    </li>
                  ))}
                </ul>
              </Section>
            )}
          </Page>
        );
      }}
    </QueryView>
  );
}

// ------------------------------------------------------------------------------------- returns

export function ReturnsPage() {
  const navigate = useNavigate();
  const money = useMoney();
  const table = useTableState("sales-returns", { filters: { q: "" } });
  const [search, setSearch] = useSearchFilter(table);
  const query = useList("sales_list_returns", {
    search: table.filter("q") || null,
    ...table.input,
  });
  const columns = useMemo<ColumnDef<Api.ReturnListItem>[]>(
    () => [
      {
        header: "Credit note",
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
      { header: "Customer", accessorKey: "customerName", meta: { sortKey: "customer" } },
      { header: "Invoice", accessorFn: (r) => r.invoiceDocNo ?? "", meta: { sortKey: "invoice" } },
      {
        header: "Total",
        accessorKey: "total",
        meta: { align: "right", sortKey: "total" },
        cell: ({ row }) => money(row.original.total),
      },
      {
        header: "Refunded",
        accessorKey: "amountRefunded",
        meta: { align: "right", sortKey: "refunded" },
        cell: ({ row }) => money(row.original.amountRefunded),
      },
    ],
    [money],
  );
  return (
    <Page
      title="Sales returns"
      description="Credit notes. Start a return from the invoice it belongs to."
    >
      <SearchInput
        aria-label="Search returns"
        placeholder="Number, customer or invoice"
        className="max-w-xs"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <DataTable
        viewKey="sales-returns"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) => void navigate(`/sales/returns/${r.id}`)}
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{ title: "No returns", description: "Open an invoice and choose “Return items”." }}
      />
    </Page>
  );
}

export function ReturnPage() {
  const { id = "" } = useParams();
  const money = useMoney();
  const ret = useCommand("sales_get_return", { id });
  const settings = usePublicSettings();
  return (
    <QueryView query={ret}>
      {(r) => (
        <Page
          title={`Credit note ${r.docNo}`}
          back={{ to: "/sales/returns", label: "Sales returns" }}
          actions={
            <Button
              variant="outline"
              onClick={() =>
                void printDocument({
                  document: { type: "return_slip", salesReturn: r, invoiceNo: null },
                  fallbackHtml: () => returnSlipHtml(r, settings.data),
                })
              }
            >
              <Printer /> Print slip
            </Button>
          }
        >
          <Section>
            <Facts
              items={[
                ["Date", showDate(r.businessDate)],
                ["Customer", r.customerName],
                ["Total", money(r.total)],
                ["Refunded", money(r.amountRefunded)],
              ]}
            />
            {r.originalInvoiceId && (
              <Link
                to={`/sales/invoices/${r.originalInvoiceId}`}
                className="mt-3 inline-block text-sm underline"
              >
                Original invoice
              </Link>
            )}
            {r.reason && <p className="mt-2 text-sm text-muted-foreground">{r.reason}</p>}
          </Section>
          <Section title="Items">
            <table className="w-full text-sm">
              <tbody>
                {r.lines.map((l, i) => (
                  <tr key={i} className="border-t border-border/40 first:border-0">
                    <td className="py-1.5 font-medium">{l.itemName}</td>
                    <td className="text-right tabular-nums">{showQty(l.qty)}</td>
                    <td className="text-right tabular-nums">{money(l.unitPrice)}</td>
                    <td className="text-right text-xs text-muted-foreground">
                      {l.restock ? "back in stock" : "not restocked"}
                    </td>
                    <td className="text-right font-medium tabular-nums">{money(l.lineTotal)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Section>
        </Page>
      )}
    </QueryView>
  );
}

interface ReturnRow {
  qty: number | null;
  restock: boolean;
}

export function NewReturnPage() {
  const [params] = useSearchParams();
  const invoiceId = params.get("invoice") ?? "";
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const money = useMoney();
  const invoice = useCommand(
    "sales_get_invoice",
    { id: invoiceId },
    { enabled: Boolean(invoiceId) },
  );
  const [key] = useState(requestId);
  const [rows, setRows] = useState<Record<string, ReturnRow>>({});
  const [reason, setReason] = useState("");
  const [refund, setRefund] = useState<number | null>(null);
  const [mode, setMode] = useState("cash");
  const [busy, setBusy] = useState(false);
  if (!invoiceId) {
    return (
      <Page title="New return" back={{ to: "/sales/returns", label: "Sales returns" }}>
        <p className="text-sm text-muted-foreground">
          Open the invoice the goods were sold on and choose “Return items”.
        </p>
      </Page>
    );
  }
  return (
    <QueryView query={invoice}>
      {(inv) => {
        const chosen = inv.lines.filter((l) => (rows[l.id]?.qty ?? 0) > 0);
        const estimate = chosen.reduce(
          (n, l) => n + Math.round((l.lineTotal * (rows[l.id]?.qty ?? 0)) / l.qty),
          0,
        );
        const walkIn = !inv.partyId;
        const save = async () => {
          setBusy(true);
          try {
            const r = await call("sales_create_return", {
              idempotencyKey: key,
              originalInvoiceId: inv.id,
              reason,
              lines: chosen.map((l) => ({
                originalItemId: l.id,
                qty: rows[l.id]?.qty ?? 0,
                restock: rows[l.id]?.restock ?? true,
              })),
              // Walk-in: the backend refunds the exact total; only the mode matters.
              refund:
                walkIn || refund
                  ? {
                      mode,
                      amount: walkIn ? estimate : (refund ?? 0),
                      tendered: null,
                      accountId: null,
                      reference: null,
                    }
                  : null,
            });
            toast.success(`Credit note ${r.docNo}: ${money(r.total)}`);
            for (const k of [
              "sales_get_invoice",
              "sales_list_returns",
              "sales_invoice_returns",
              "parties_get",
              "products_list",
            ]) {
              void queryClient.invalidateQueries({ queryKey: [k] });
            }
            void navigate(`/sales/returns/${r.id}`, { replace: true });
          } catch (e) {
            toast.error(errorText(e));
          } finally {
            setBusy(false);
          }
        };
        return (
          <Page
            title={`Return items from ${inv.docNo}`}
            description={inv.customerName}
            back={{ to: `/sales/invoices/${inv.id}`, label: inv.docNo }}
            actions={
              <Button
                disabled={busy || chosen.length === 0 || !reason.trim()}
                onClick={() => void save()}
              >
                {busy ? "Saving…" : "Save return"}
              </Button>
            }
          >
            <Section title="What is coming back?">
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1 text-left font-medium">Item</th>
                    <th className="py-1 text-right font-medium">Sold</th>
                    <th className="py-1 text-right font-medium">Already returned</th>
                    <th className="w-32 py-1 text-right font-medium">Return now</th>
                    <th className="py-1 text-center font-medium">Back in stock</th>
                  </tr>
                </thead>
                <tbody>
                  {inv.lines.map((l) => {
                    const left = l.qty - l.returnedQty;
                    const row = rows[l.id] ?? { qty: null, restock: true };
                    return (
                      <tr key={l.id} className="border-t border-border/40">
                        <td className="py-1.5 font-medium">{l.itemName}</td>
                        <td className="text-right tabular-nums">{showQty(l.qty)}</td>
                        <td className="text-right tabular-nums">
                          {l.returnedQty ? showQty(l.returnedQty) : ""}
                        </td>
                        <td className="py-1">
                          {left > 0 ? (
                            <QtyInput
                              aria-label={`Return ${l.itemName}`}
                              value={row.qty}
                              onValueChange={(qty) =>
                                setRows((r) => ({
                                  ...r,
                                  [l.id]: {
                                    ...row,
                                    qty: qty === null ? null : Math.min(qty, left),
                                  },
                                }))
                              }
                            />
                          ) : (
                            <span className="block text-right text-xs text-muted-foreground">
                              all returned
                            </span>
                          )}
                        </td>
                        <td className="text-center">
                          <Checkbox
                            aria-label={`Put ${l.itemName} back in stock`}
                            checked={row.restock}
                            onCheckedChange={(restock) =>
                              setRows((r) => ({ ...r, [l.id]: { ...row, restock } }))
                            }
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </Section>
            <Section title="Reason and refund">
              <div className="grid gap-4 md:grid-cols-3">
                <Field className="md:col-span-3">
                  <FieldLabel htmlFor="ret-reason">Reason</FieldLabel>
                  <Input
                    id="ret-reason"
                    placeholder="e.g. damaged, wrong size"
                    value={reason}
                    onChange={(e) => setReason(e.target.value)}
                  />
                </Field>
                <p className="text-sm md:col-span-3">
                  About <span className="font-semibold">{money(estimate)}</span> (the exact amount,
                  with tax and round-off, is worked out when saved).{" "}
                  {walkIn
                    ? "Walk-in customers get the money back now."
                    : "Leave the refund empty to keep it as credit on the customer's account."}
                </p>
                {!walkIn && (
                  <Field>
                    <FieldLabel htmlFor="ret-refund">Refund now</FieldLabel>
                    <MoneyInput
                      id="ret-refund"
                      placeholder="0 = keep as credit"
                      value={refund}
                      onValueChange={setRefund}
                    />
                  </Field>
                )}
                <Field>
                  <FieldLabel htmlFor="ret-mode">Refund by</FieldLabel>
                  <SelectField
                    id="ret-mode"
                    className="w-full"
                    value={mode}
                    onValueChange={setMode}
                    options={[
                      ...["cash", "upi", "card", "bank_transfer"].map((m) => ({
                        value: m,
                        label: label(m),
                      })),
                    ]}
                  />
                </Field>
              </div>
            </Section>
          </Page>
        );
      }}
    </QueryView>
  );
}
