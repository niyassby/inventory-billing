import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { Ban, Barcode, PackageCheck, Plus, Receipt, Trash2, Undo2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import type { ItemLine } from "@/components/app/item-lines";
import { ItemLines, lineKey } from "@/components/app/item-lines";
import { MoneyInput, QtyInput, RateInput } from "@/components/app/inputs";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { PartyPicker } from "@/components/app/pickers";
import { WarehouseSelect } from "@/components/app/warehouse-select";
import { call, requestId } from "@/api/v2";
import { useMoney } from "@/lib/business";
import { label, showDate, showQty } from "@/lib/format";
import { errorText, useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";

type Tab = "purchase_order" | "goods_receipt" | "purchase_bill" | "purchase_return";

const TABS: [Tab, string][] = [
  ["purchase_order", "Orders"],
  ["goods_receipt", "Goods received"],
  ["purchase_bill", "Supplier bills"],
  ["purchase_return", "Returns"],
];

const PATHS: Record<Tab, string> = {
  purchase_order: "orders",
  goods_receipt: "receipts",
  purchase_bill: "bills",
  purchase_return: "returns",
};

// ------------------------------------------------------------------------------------- list

export function PurchasesPage() {
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("purchases.manage");
  const canReceive = useCan("purchases.receive");
  const table = useTableState("purchases", { filters: { tab: "purchase_order", q: "" } });
  const [search, setSearch] = useSearchFilter(table);
  const tab = (table.filter("tab") || "purchase_order") as Tab;
  const term = table.filter("q");
  const docs = useList(
    "purchases_list",
    { kind: tab, search: term || null, ...table.input },
    { enabled: tab !== "goods_receipt" },
  );
  const receipts = useList(
    "purchases_list_receipts",
    { search: term || null, ...table.input },
    { enabled: tab === "goods_receipt" },
  );
  const docColumns = useMemo<ColumnDef<Api.PurchaseListItem>[]>(
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
      { header: "Supplier", accessorKey: "supplierName", meta: { sortKey: "supplier" } },
      {
        header: "Their bill no.",
        accessorFn: (r) => r.supplierInvoiceNo ?? "",
        meta: { sortKey: "supplierInvoiceNo" },
      },
      {
        header: "Status",
        accessorKey: "status",
        meta: { sortKey: "status" },
        cell: ({ row }) => <StatusBadge status={row.original.status} />,
      },
      {
        header: "Total",
        accessorKey: "total",
        meta: { align: "right", sortKey: "total" },
        cell: ({ row }) => money(row.original.total),
      },
      {
        header: tab === "purchase_return" ? "Refunded" : "Paid",
        accessorKey: "amountPaid",
        meta: { align: "right", sortKey: "paid" },
        cell: ({ row }) => (tab === "purchase_order" ? "" : money(row.original.amountPaid)),
      },
    ],
    [money, tab],
  );
  const receiptColumns = useMemo<ColumnDef<Api.ReceiptListItem>[]>(
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
      { header: "Supplier", accessorKey: "supplierName", meta: { sortKey: "supplier" } },
      { header: "Order", accessorFn: (r) => r.orderDocNo ?? "", meta: { sortKey: "order" } },
      {
        header: "Bill",
        accessorFn: (r) => r.billDocNo ?? "",
        cell: ({ row }) =>
          row.original.billDocNo ?? <span className="text-warning">not billed yet</span>,
      },
      {
        header: "Value",
        accessorKey: "value",
        meta: { align: "right" },
        cell: ({ row }) => money(row.original.value),
      },
    ],
    [money],
  );
  const empty = { title: `No ${TABS.find((t) => t[0] === tab)?.[1].toLowerCase() ?? ""} yet` };
  return (
    <Page
      title="Purchases"
      description="Order from suppliers, receive goods, enter their bills."
      actions={
        <>
          {canManage && (
            <Button
              variant="outline"
              render={<Link to="/purchases/orders/new" />}
              nativeButton={false}
            >
              <Plus /> New order
            </Button>
          )}
          {canReceive && (
            <Button
              variant="outline"
              render={<Link to="/purchases/receipts/new" />}
              nativeButton={false}
            >
              <PackageCheck /> Receive goods
            </Button>
          )}
          {canManage && (
            <Button render={<Link to="/purchases/bills/new" />} nativeButton={false}>
              <Receipt /> Enter a bill
            </Button>
          )}
        </>
      }
    >
      <Tabs value={tab} onValueChange={(v) => table.setFilter("tab", String(v))}>
        <TabsList>
          {TABS.map(([value, text]) => (
            <TabsTrigger key={value} value={value}>
              {text}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      <SearchInput
        aria-label="Search"
        placeholder="Number, supplier or their bill no."
        className="max-w-xs"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      {tab === "goods_receipt" ? (
        <DataTable
          viewKey="purchase-receipts"
          sort={table.sortProps}
          columns={receiptColumns}
          data={receipts.data?.items}
          loading={receipts.isPending}
          error={receipts.error}
          rowKey={(r) => r.id}
          onRowClick={(r) => void navigate(`/purchases/receipts/${r.id}`)}
          paging={receipts.data ? table.paging(receipts.data.total) : undefined}
          empty={empty}
        />
      ) : (
        <DataTable
          viewKey="purchases"
          sort={table.sortProps}
          columns={docColumns}
          data={docs.data?.items}
          loading={docs.isPending}
          error={docs.error}
          rowKey={(r) => r.id}
          onRowClick={(r) => void navigate(`/purchases/${PATHS[tab]}/${r.id}`)}
          paging={docs.data ? table.paging(docs.data.total) : undefined}
          empty={empty}
        />
      )}
    </Page>
  );
}

// ------------------------------------------------------------------------------------- editor

type Mode = "order" | "receipt" | "bill";

interface BuyRow extends ItemLine {
  unitCost: number | null;
  discountRate: number | null;
  batchNo: string;
  expiryDate: string;
  poItemId: string | null;
}

interface Charge {
  description: string;
  amount: number | null;
  allocation: "value" | "qty" | "none";
}

function rowFromDoc(l: Api.PurchaseDocLine, qty: number, poItemId: string | null): BuyRow {
  return {
    key: lineKey(),
    item: {
      variantId: l.variantId,
      productId: "",
      name: l.itemName,
      sku: "",
      barcode: null,
      unitId: l.unitId ?? "",
      unitCode: "",
      factorNum: 1,
      factorDen: 1,
      salePrice: 0,
      mrp: null,
      taxRateId: null,
      taxRate: 0,
      taxType: null,
      cessRate: 0,
      hsnSac: null,
      stock: 0,
      isService: false,
      trackBatches: false,
    },
    qty,
    unitCost: l.unitCost,
    discountRate: null,
    batchNo: "",
    expiryDate: "",
    poItemId,
  };
}

function lineInputs(rows: BuyRow[]): Api.PurchaseLineInput[] {
  return rows
    .filter((r) => (r.qty ?? 0) > 0)
    .map((r) => ({
      variantId: r.item.variantId,
      unitId: r.item.unitId || null,
      qty: r.qty ?? 0,
      unitCost: r.unitCost ?? 0,
      discount: r.discountRate ? { kind: "percent", rate: r.discountRate } : null,
      batchNo: r.batchNo || null,
      expiryDate: r.expiryDate || null,
      poItemId: r.poItemId,
    }));
}

const TITLES: Record<Mode, string> = {
  order: "New purchase order",
  receipt: "Receive goods",
  bill: "Enter a supplier bill",
};

export function PurchaseEditor({ mode }: { mode: Mode }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const money = useMoney();
  const [params] = useSearchParams();
  const orderId = params.get("order");
  const receiptId = params.get("receipt");
  const order = useCommand(
    "purchases_get_order",
    { id: orderId ?? "" },
    { enabled: Boolean(orderId) },
  );
  const receipt = useCommand(
    "purchases_get_receipt",
    { id: receiptId ?? "" },
    { enabled: Boolean(receiptId) },
  );
  const [key] = useState(requestId);
  const [supplier, setSupplier] = useState<{ id: string; name: string } | null>(null);
  const [warehouseId, setWarehouseId] = useState("");
  const [expectedDate, setExpectedDate] = useState("");
  const [reference, setReference] = useState("");
  const [invoiceNo, setInvoiceNo] = useState("");
  const [invoiceDate, setInvoiceDate] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [pricesIncludeTax, setPricesIncludeTax] = useState(false);
  const [notes, setNotes] = useState("");
  const [rows, setRows] = useState<BuyRow[]>([]);
  const [charges, setCharges] = useState<Charge[]>([]);
  const [payNow, setPayNow] = useState<number | null>(null);
  const [payMode, setPayMode] = useState("cash");
  const [busy, setBusy] = useState(false);

  // Prefill from the order (remaining quantities) or from the goods receipt.
  useEffect(() => {
    if (order.data) {
      setSupplier({ id: order.data.partyId, name: order.data.supplierName });
      setRows(
        order.data.lines
          .filter((l) => l.qty > l.progressQty || mode !== "receipt")
          .map((l) =>
            rowFromDoc(
              l,
              mode === "receipt" ? l.qty - l.progressQty : l.qty,
              mode === "receipt" ? l.id : null,
            ),
          ),
      );
    }
  }, [order.data, mode]);
  useEffect(() => {
    if (receipt.data) {
      setRows(receipt.data.lines.map((l) => rowFromDoc(l, l.qty, null)));
    }
  }, [receipt.data]);
  const supplierQuery = useCommand(
    "parties_get",
    { id: receipt.data?.partyId ?? "" },
    { enabled: Boolean(receipt.data) },
  );
  useEffect(() => {
    if (supplierQuery.data)
      setSupplier({ id: supplierQuery.data.id, name: supplierQuery.data.name });
  }, [supplierQuery.data]);

  const estimate =
    rows.reduce((n, r) => {
      const gross = Math.round(((r.qty ?? 0) * (r.unitCost ?? 0)) / 1000);
      return n + gross - Math.round((gross * (r.discountRate ?? 0)) / 1_000_000);
    }, 0) + charges.reduce((n, c) => n + (c.amount ?? 0), 0);
  const valid =
    supplier !== null &&
    rows.length > 0 &&
    rows.every((r) => (r.qty ?? 0) > 0 && r.unitCost !== null);

  const save = async () => {
    if (!supplier) return;
    setBusy(true);
    try {
      const lines = lineInputs(rows);
      let path: string;
      if (mode === "order") {
        const doc = await call("purchases_save_order", {
          partyId: supplier.id,
          warehouseId: warehouseId || null,
          expectedDate: expectedDate || null,
          lines,
          notes: notes || null,
        });
        path = `/purchases/orders/${doc.id}`;
        toast.success(`Order ${doc.docNo} saved`);
      } else if (mode === "receipt") {
        const doc = await call("purchases_receive_goods", {
          idempotencyKey: key,
          partyId: supplier.id,
          warehouseId: warehouseId || null,
          purchaseOrderId: orderId,
          supplierReference: reference || null,
          lines,
          notes: notes || null,
        });
        path = `/purchases/receipts/${doc.id}`;
        toast.success(`${doc.docNo}: goods received into stock`);
      } else {
        const doc = await call("purchases_create_bill", {
          idempotencyKey: key,
          partyId: supplier.id,
          supplierInvoiceNo: invoiceNo || null,
          supplierInvoiceDate: invoiceDate || null,
          dueDate: dueDate || null,
          warehouseId: warehouseId || null,
          pricesIncludeTax,
          purchaseOrderId: receiptId ? null : orderId,
          goodsReceiptId: receiptId,
          lines,
          charges: charges
            .filter((c) => c.description && c.amount)
            .map((c) => ({
              description: c.description,
              amount: c.amount ?? 0,
              allocation: c.allocation,
            })),
          payments: payNow
            ? [{ mode: payMode, amount: payNow, tendered: null, accountId: null, reference: null }]
            : [],
          notes: notes || null,
        });
        path = `/purchases/bills/${doc.id}`;
        toast.success(`Bill ${doc.docNo} saved: ${money(doc.total)}`);
        if (doc.warnings && doc.warnings.length > 0)
          toast.warning("Some items went below zero stock.");
      }
      for (const k of [
        "purchases_list",
        "purchases_list_receipts",
        "purchases_get_order",
        "products_list",
        "parties_get",
      ]) {
        void queryClient.invalidateQueries({ queryKey: [k] });
      }
      void navigate(path, { replace: true });
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Page
      title={TITLES[mode]}
      description={
        order.data
          ? `From order ${order.data.docNo}`
          : receipt.data
            ? `For goods received ${receipt.data.docNo}`
            : undefined
      }
      back={{ to: "/purchases", label: "Purchases" }}
      actions={
        <Button disabled={!valid || busy} onClick={() => void save()}>
          {busy
            ? "Saving…"
            : mode === "order"
              ? "Save order"
              : mode === "receipt"
                ? "Receive into stock"
                : "Save bill"}
        </Button>
      }
    >
      <Section>
        <div className="grid gap-4 md:grid-cols-3">
          <Field>
            <FieldLabel>Supplier</FieldLabel>
            {supplier ? (
              <div className="flex items-center gap-2">
                <span className="font-medium">{supplier.name}</span>
                {!orderId && !receiptId && (
                  <Button variant="ghost" size="xs" onClick={() => setSupplier(null)}>
                    Change
                  </Button>
                )}
              </div>
            ) : (
              <PartyPicker
                partyType="supplier"
                autoFocus
                onSelect={(p) => setSupplier({ id: p.id, name: p.name })}
              />
            )}
          </Field>
          {!receiptId && (
            <Field>
              <FieldLabel htmlFor="buy-store">Store</FieldLabel>
              <WarehouseSelect id="buy-store" value={warehouseId} onChange={setWarehouseId} />
            </Field>
          )}
          {mode === "order" && (
            <Field>
              <FieldLabel htmlFor="buy-expected">Expected by</FieldLabel>
              <DatePicker id="buy-expected" value={expectedDate} onChange={setExpectedDate} />
            </Field>
          )}
          {mode === "receipt" && (
            <Field>
              <FieldLabel htmlFor="buy-ref">Supplier's delivery note</FieldLabel>
              <Input
                id="buy-ref"
                value={reference}
                onChange={(e) => setReference(e.target.value)}
              />
            </Field>
          )}
          {mode === "bill" && (
            <>
              <Field>
                <FieldLabel htmlFor="buy-invno">Their bill number</FieldLabel>
                <Input
                  id="buy-invno"
                  value={invoiceNo}
                  onChange={(e) => setInvoiceNo(e.target.value)}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="buy-invdate">Their bill date</FieldLabel>
                <DatePicker id="buy-invdate" value={invoiceDate} onChange={setInvoiceDate} />
              </Field>
              <Field>
                <FieldLabel htmlFor="buy-due">Pay by</FieldLabel>
                <DatePicker id="buy-due" value={dueDate} onChange={setDueDate} />
              </Field>
              <label className="flex items-center gap-2 self-end pb-2 text-sm">
                <Checkbox checked={pricesIncludeTax} onCheckedChange={setPricesIncludeTax} /> Costs
                include tax
              </label>
            </>
          )}
        </div>
      </Section>
      <Section title="Items">
        <ItemLines<BuyRow>
          lines={rows}
          onChange={setRows}
          warehouseId={warehouseId || null}
          disabled={Boolean(receiptId)}
          makeLine={(item) => ({
            key: lineKey(),
            item,
            qty: 1000,
            unitCost: null,
            discountRate: null,
            batchNo: "",
            expiryDate: "",
            poItemId: null,
          })}
          columns={[
            {
              header: "Cost each",
              align: "right",
              className: "w-36",
              cell: (l, update) => (
                <MoneyInput
                  aria-label="Cost each"
                  value={l.unitCost}
                  onValueChange={(unitCost) => update({ unitCost })}
                />
              ),
            },
            ...(mode === "receipt"
              ? []
              : [
                  {
                    header: "Discount",
                    align: "right" as const,
                    className: "w-28",
                    cell: (l: BuyRow, update: (c: Partial<BuyRow>) => void) => (
                      <RateInput
                        aria-label="Discount"
                        value={l.discountRate}
                        onValueChange={(discountRate) => update({ discountRate })}
                      />
                    ),
                  },
                ]),
            ...(mode === "order"
              ? []
              : [
                  {
                    header: "Batch / expiry",
                    className: "w-56",
                    cell: (l: BuyRow, update: (c: Partial<BuyRow>) => void) =>
                      l.item.trackBatches ? (
                        <div className="flex gap-1">
                          <Input
                            aria-label="Batch number"
                            placeholder="Batch"
                            value={l.batchNo}
                            onChange={(e) => update({ batchNo: e.target.value })}
                          />
                          <DatePicker
                            aria-label="Expiry date"
                            value={l.expiryDate}
                            onChange={(v) => update({ expiryDate: v })}
                          />
                        </div>
                      ) : null,
                  },
                ]),
          ]}
        />
      </Section>
      {mode === "bill" && (
        <Section
          title="Freight and other charges"
          actions={
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                setCharges((c) => [
                  ...c,
                  { description: "Freight", amount: null, allocation: "value" },
                ])
              }
            >
              <Plus /> Add charge
            </Button>
          }
        >
          {charges.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Freight, loading or other costs on this bill are added to the items' cost (landed
              cost).
            </p>
          ) : (
            charges.map((c, i) => (
              <div key={i} className="mb-2 flex flex-wrap items-center gap-2">
                <Input
                  aria-label="Charge"
                  className="w-48"
                  value={c.description}
                  onChange={(e) =>
                    setCharges((all) =>
                      all.map((x, j) => (j === i ? { ...x, description: e.target.value } : x)),
                    )
                  }
                />
                <MoneyInput
                  aria-label="Charge amount"
                  className="w-32"
                  value={c.amount}
                  onValueChange={(amount) =>
                    setCharges((all) => all.map((x, j) => (j === i ? { ...x, amount } : x)))
                  }
                />
                <SelectField
                  aria-label="Spread over items by"
                  value={c.allocation}
                  onValueChange={(v) =>
                    setCharges((all) =>
                      all.map((x, j) =>
                        j === i ? { ...x, allocation: v as Charge["allocation"] } : x,
                      ),
                    )
                  }
                  options={[
                    { value: "value", label: "spread by value" },
                    { value: "qty", label: "spread by quantity" },
                    { value: "none", label: "expense, not item cost" },
                  ]}
                />
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Remove charge"
                  onClick={() => setCharges((all) => all.filter((_, j) => j !== i))}
                >
                  <Trash2 />
                </Button>
              </div>
            ))
          )}
        </Section>
      )}
      <Section>
        <div className="grid gap-4 md:grid-cols-3">
          <Field className="md:col-span-2">
            <FieldLabel htmlFor="buy-notes">Notes</FieldLabel>
            <Textarea
              id="buy-notes"
              className="min-h-10"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
          </Field>
          <div className="flex flex-col justify-end gap-2 text-sm">
            <p className="flex justify-between">
              <span className="text-muted-foreground">Before tax (about)</span>
              <span className="font-semibold tabular-nums">{money(estimate)}</span>
            </p>
            <p className="text-xs text-muted-foreground">
              Tax and the exact total are worked out when saved.
            </p>
          </div>
          {mode === "bill" && (
            <>
              <Field>
                <FieldLabel htmlFor="buy-pay">Pay now (optional)</FieldLabel>
                <MoneyInput id="buy-pay" value={payNow} onValueChange={setPayNow} />
              </Field>
              <Field>
                <FieldLabel htmlFor="buy-paymode">Paid by</FieldLabel>
                <SelectField
                  id="buy-paymode"
                  className="w-full"
                  value={payMode}
                  onValueChange={setPayMode}
                  options={[
                    ...["cash", "bank_transfer", "upi", "cheque"].map((m) => ({
                      value: m,
                      label: label(m),
                    })),
                  ]}
                />
              </Field>
            </>
          )}
        </div>
      </Section>
    </Page>
  );
}

// ------------------------------------------------------------------------------------- views

function DocLines({ lines, progress }: { lines: Api.PurchaseDocLine[]; progress?: string }) {
  const money = useMoney();
  return (
    <table className="w-full text-sm">
      <thead className="text-xs text-muted-foreground">
        <tr>
          <th className="py-1 text-left font-medium">Item</th>
          <th className="py-1 text-right font-medium">Qty</th>
          {progress && <th className="py-1 text-right font-medium">{progress}</th>}
          <th className="py-1 text-right font-medium">Cost each</th>
          <th className="py-1 text-right font-medium">Tax</th>
          <th className="py-1 text-right font-medium">Amount</th>
        </tr>
      </thead>
      <tbody>
        {lines.map((l, i) => (
          <tr key={l.id || i} className="border-t border-border/40">
            <td className="py-1.5 font-medium">{l.itemName}</td>
            <td className="text-right tabular-nums">{showQty(l.qty)}</td>
            {progress && <td className="text-right tabular-nums">{showQty(l.progressQty)}</td>}
            <td className="text-right tabular-nums">{money(l.unitCost)}</td>
            <td className="text-right tabular-nums">{l.taxTotal ? money(l.taxTotal) : ""}</td>
            <td className="text-right font-medium tabular-nums">
              {l.lineTotal ? money(l.lineTotal) : ""}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function PurchaseDocPage({
  kind,
}: {
  kind: "purchase_order" | "purchase_bill" | "purchase_return";
}) {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("purchases.manage");
  const canReceive = useCan("purchases.receive");
  const command =
    kind === "purchase_order"
      ? "purchases_get_order"
      : kind === "purchase_bill"
        ? "purchases_get_bill"
        : "purchases_get_return";
  const doc = useCommand(command, { id });
  const cancel = useAction("purchases_cancel_order", {
    invalidates: ["purchases_get_order", "purchases_list"],
    success: "Order cancelled",
  });
  const voidBill = useAction("purchases_void_bill", {
    invalidates: ["purchases_get_bill", "purchases_list", "products_list", "parties_get"],
    success: "Bill voided",
  });
  return (
    <QueryView query={doc}>
      {(d) => {
        const isOrder = kind === "purchase_order";
        const isBill = kind === "purchase_bill";
        const openOrder =
          isOrder && !["cancelled", "received", "closed", "billed"].includes(d.status);
        return (
          <Page
            title={d.docNo}
            description={<StatusBadge status={d.status} />}
            back={{ to: `/purchases?tab=${kind}`, label: "Purchases" }}
            actions={
              <>
                {openOrder && canReceive && (
                  <Button
                    variant="outline"
                    onClick={() => void navigate(`/purchases/receipts/new?order=${d.id}`)}
                  >
                    <PackageCheck /> Receive goods
                  </Button>
                )}
                {isOrder && canManage && d.status !== "cancelled" && (
                  <Button
                    variant="outline"
                    onClick={() => void navigate(`/purchases/bills/new?order=${d.id}`)}
                  >
                    <Receipt /> Enter bill
                  </Button>
                )}
                {openOrder && canManage && (
                  <Button
                    variant="destructive"
                    onClick={run(async () => {
                      if (
                        (await confirm({
                          title: `Cancel ${d.docNo}?`,
                          confirmLabel: "Cancel order",
                          destructive: true,
                        })) !== false
                      ) {
                        cancel.mutate({ input: { id: d.id } });
                      }
                    })}
                  >
                    <Ban /> Cancel order
                  </Button>
                )}
                {isBill && canManage && d.status === "finalized" && (
                  <>
                    <Button
                      variant="outline"
                      onClick={() => void navigate(`/purchases/returns/new?bill=${d.id}`)}
                    >
                      <Undo2 /> Return to supplier
                    </Button>
                    <Button
                      variant="destructive"
                      onClick={run(async () => {
                        const reason = await confirm({
                          title: `Void ${d.docNo}?`,
                          description:
                            "Stock, the supplier balance and the books are reversed. A manager must approve.",
                          confirmLabel: "Void bill",
                          destructive: true,
                          reason: { label: "Reason", required: true },
                        });
                        if (reason !== false) voidBill.mutate({ input: { id: d.id, reason } });
                      })}
                    >
                      <Ban /> Void
                    </Button>
                  </>
                )}
              </>
            }
          >
            <Section>
              <Facts
                items={[
                  [
                    "Supplier",
                    <Link key="s" to={`/parties/${d.partyId}`} className="hover:underline">
                      {d.supplierName}
                    </Link>,
                  ],
                  ["Date", showDate(d.businessDate)],
                  ["Total", money(d.total)],
                  [
                    kind === "purchase_return" ? "Refunded" : "Paid",
                    isOrder ? "—" : money(d.amountPaid),
                  ],
                ]}
              />
              {d.supplierInvoiceNo && (
                <p className="mt-3 text-sm">Their bill no. {d.supplierInvoiceNo}</p>
              )}
              {d.notes && <p className="mt-2 text-sm text-muted-foreground">{d.notes}</p>}
            </Section>
            <Section title="Items">
              <DocLines
                lines={d.lines}
                {...(isOrder ? { progress: "Received" } : isBill ? { progress: "Returned" } : {})}
              />
              {d.chargesTotal > 0 && (
                <p className="mt-2 text-right text-sm">Charges {money(d.chargesTotal)}</p>
              )}
            </Section>
          </Page>
        );
      }}
    </QueryView>
  );
}

export function ReceiptPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("purchases.manage");
  const canLabel = useCan("labels.print");
  const receipt = useCommand("purchases_get_receipt", { id });
  return (
    <QueryView query={receipt}>
      {(r) => (
        <Page
          title={r.docNo}
          description="Goods received"
          back={{ to: "/purchases?tab=goods_receipt", label: "Purchases" }}
          actions={
            <>
              {canLabel && (
                <Button
                  variant="outline"
                  onClick={() => void navigate(`/inventory/labels?receipt=${r.id}`)}
                >
                  <Barcode /> Print labels
                </Button>
              )}
              {canManage && (
                <Button onClick={() => void navigate(`/purchases/bills/new?receipt=${r.id}`)}>
                  <Receipt /> Enter the bill
                </Button>
              )}
            </>
          }
        >
          <Section>
            <Facts
              items={[
                ["Date", showDate(r.businessDate)],
                ["Value at cost", money(r.value)],
                ["Items", r.lines.length],
              ]}
            />
            {r.purchaseOrderId && (
              <Link
                to={`/purchases/orders/${r.purchaseOrderId}`}
                className="mt-3 inline-block text-sm underline"
              >
                Purchase order
              </Link>
            )}
          </Section>
          <Section title="Items">
            <DocLines lines={r.lines} />
          </Section>
        </Page>
      )}
    </QueryView>
  );
}

export function NewPurchaseReturnPage() {
  const [params] = useSearchParams();
  const billId = params.get("bill") ?? "";
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const bill = useCommand("purchases_get_bill", { id: billId }, { enabled: Boolean(billId) });
  const [key] = useState(requestId);
  const [qty, setQty] = useState<Record<string, number | null>>({});
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  if (!billId) {
    return (
      <Page
        title="Return to supplier"
        back={{ to: "/purchases?tab=purchase_return", label: "Purchases" }}
      >
        <p className="text-sm text-muted-foreground">
          Open the supplier's bill and choose “Return to supplier”.
        </p>
      </Page>
    );
  }
  return (
    <QueryView query={bill}>
      {(b) => {
        const chosen = b.lines.filter((l) => (qty[l.id] ?? 0) > 0);
        const save = async () => {
          setBusy(true);
          try {
            const r = await call("purchases_create_return", {
              idempotencyKey: key,
              partyId: b.partyId,
              originalBillId: b.id,
              reason,
              lines: chosen.map((l) => ({ originalItemId: l.id, qty: qty[l.id] ?? 0 })),
            });
            toast.success(`Debit note ${r.docNo}`);
            for (const k of [
              "purchases_get_bill",
              "purchases_list",
              "products_list",
              "parties_get",
            ])
              void queryClient.invalidateQueries({ queryKey: [k] });
            void navigate(`/purchases/returns/${r.id}`, { replace: true });
          } catch (e) {
            toast.error(errorText(e));
          } finally {
            setBusy(false);
          }
        };
        return (
          <Page
            title={`Return items from ${b.docNo}`}
            description={b.supplierName}
            back={{ to: `/purchases/bills/${b.id}`, label: b.docNo }}
            actions={
              <Button
                disabled={busy || chosen.length === 0 || !reason.trim()}
                onClick={() => void save()}
              >
                {busy ? "Saving…" : "Save return"}
              </Button>
            }
          >
            <Section title="What goes back?">
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1 text-left font-medium">Item</th>
                    <th className="py-1 text-right font-medium">Billed</th>
                    <th className="py-1 text-right font-medium">Already returned</th>
                    <th className="w-32 py-1 text-right font-medium">Return now</th>
                  </tr>
                </thead>
                <tbody>
                  {b.lines.map((l) => {
                    const left = l.qty - l.progressQty;
                    return (
                      <tr key={l.id} className="border-t border-border/40">
                        <td className="py-1.5 font-medium">{l.itemName}</td>
                        <td className="text-right tabular-nums">{showQty(l.qty)}</td>
                        <td className="text-right tabular-nums">
                          {l.progressQty ? showQty(l.progressQty) : ""}
                        </td>
                        <td className="py-1">
                          {left > 0 ? (
                            <QtyInput
                              aria-label={`Return ${l.itemName}`}
                              value={qty[l.id] ?? null}
                              onValueChange={(v) =>
                                setQty((q) => ({
                                  ...q,
                                  [l.id]: v === null ? null : Math.min(v, left),
                                }))
                              }
                            />
                          ) : (
                            <span className="block text-right text-xs text-muted-foreground">
                              all returned
                            </span>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </Section>
            <Section>
              <Field>
                <FieldLabel htmlFor="pret-reason">Reason</FieldLabel>
                <Input
                  id="pret-reason"
                  placeholder="e.g. damaged on arrival"
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                />
              </Field>
              <p className="mt-2 text-sm text-muted-foreground">
                The value is taken off what you owe this supplier (a debit note).
              </p>
            </Section>
          </Page>
        );
      }}
    </QueryView>
  );
}
