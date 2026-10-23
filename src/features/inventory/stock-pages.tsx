import { SelectField } from "@/components/ui/select-field";
import { DatePicker } from "@/components/ui/date-picker";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { ClipboardCheck, PackageMinus, Truck } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable, Pager } from "@/components/app/data-table";
import { Checkbox } from "@/components/ui/checkbox";
import { StatusBadge } from "@/components/app/badges";
import { confirm } from "@/components/app/dialogs";
import type { ItemLine } from "@/components/app/item-lines";
import { ItemLines, lineKey } from "@/components/app/item-lines";
import { MoneyInput, QtyInput } from "@/components/app/inputs";
import { SearchInput } from "@/components/app/search-input";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { WarehouseSelect } from "@/components/app/warehouse-select";
import { call, requestId } from "@/api/v2";
import { useMoney } from "@/lib/business";
import { label, showDate, showQty } from "@/lib/format";
import { errorText, useAction, useCommand, useList } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";

type Kind = "adjustment" | "transfer" | "count";

// ------------------------------------------------------------------------------------- list

export function StockMovesPage() {
  const navigate = useNavigate();
  const canAdjust = useCan("stock.adjust");
  const canTransfer = useCan("stock.transfer");
  const canCount = useCan("stock.count");
  const table = useTableState("stock-moves", {
    filters: { kind: "adjustment", q: "", from: "", to: "" },
  });
  const [search, setSearch] = useSearchFilter(table);
  const kind = (table.filter("kind") || "adjustment") as Kind;
  const [countOpen, setCountOpen] = useState(false);
  const query = useList("stock_documents", {
    kind,
    search: table.filter("q") || null,
    from: table.filter("from") || null,
    to: table.filter("to") || null,
    ...table.input,
  });
  const columns = useMemo<ColumnDef<Api.StockDocListItem>[]>(
    () => [
      {
        header: "Number",
        accessorFn: (r) => r.docNo ?? "",
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
        header: "Store",
        accessorKey: "warehouseName",
        meta: { sortKey: "store" },
        cell: ({ row }) =>
          row.original.toWarehouseName
            ? `${row.original.warehouseName} → ${row.original.toWarehouseName}`
            : row.original.warehouseName,
      },
      {
        header: kind === "count" ? "Scope" : "Reason",
        accessorFn: (r) => r.note ?? "",
        cell: ({ getValue }) => <span className="text-muted-foreground">{String(getValue())}</span>,
      },
      { header: "Items", accessorKey: "lineCount", meta: { align: "right" } },
      {
        header: "Status",
        accessorKey: "status",
        meta: { sortKey: "status" },
        cell: ({ row }) => <StatusBadge status={row.original.status} />,
      },
    ],
    [kind],
  );
  return (
    <Page
      title="Stock moves"
      description="Adjustments, transfers between stores and physical counts."
      actions={
        <>
          {canAdjust && (
            <Button
              variant="outline"
              render={<Link to="/inventory/adjustments/new" />}
              nativeButton={false}
            >
              <PackageMinus /> Adjust stock
            </Button>
          )}
          {canTransfer && (
            <Button
              variant="outline"
              render={<Link to="/inventory/transfers/new" />}
              nativeButton={false}
            >
              <Truck /> Transfer
            </Button>
          )}
          {canCount && (
            <Button onClick={() => setCountOpen(true)}>
              <ClipboardCheck /> Start a count
            </Button>
          )}
        </>
      }
    >
      <Tabs value={kind} onValueChange={(v) => table.setFilter("kind", String(v))}>
        <TabsList>
          <TabsTrigger value="adjustment">Adjustments</TabsTrigger>
          <TabsTrigger value="transfer">Transfers</TabsTrigger>
          <TabsTrigger value="count">Counts</TabsTrigger>
        </TabsList>
      </Tabs>
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          aria-label="Search stock documents"
          placeholder="Document number"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
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
        viewKey="stock-moves"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) =>
          void navigate(
            r.kind === "count"
              ? `/inventory/counts/${r.id}`
              : `/inventory/documents/${r.kind}/${r.id}`,
          )
        }
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{ title: `No ${kind === "count" ? "counts" : `${kind}s`} yet` }}
      />
      <StartCountDialog open={countOpen} onClose={() => setCountOpen(false)} />
    </Page>
  );
}

function StartCountDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const navigate = useNavigate();
  const categories = useCommand("categories_list", {}, { enabled: open });
  const [warehouseId, setWarehouseId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [note, setNote] = useState("");
  const start = useAction("stock_count_start", {
    invalidates: ["stock_documents"],
    onSuccess: (count) => {
      onClose();
      void navigate(`/inventory/counts/${count.id}`);
    },
  });
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            start.mutate({
              input: {
                warehouseId: warehouseId || null,
                categoryId: categoryId || null,
                scopeNote: note || null,
              },
            });
          }}
        >
          <DialogHeader>
            <DialogTitle>Start a stock count</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            The system quantities are frozen now. Count, enter what you find, then finalize:
            differences are posted as one adjustment.
          </p>
          <Field>
            <FieldLabel htmlFor="count-store">Store</FieldLabel>
            <WarehouseSelect id="count-store" value={warehouseId} onChange={setWarehouseId} />
          </Field>
          <Field>
            <FieldLabel htmlFor="count-category">Only this category (optional)</FieldLabel>
            <SelectField
              id="count-category"
              className="w-full"
              value={categoryId}
              onValueChange={setCategoryId}
              options={[
                { value: "", label: "Everything" },
                ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="count-note">Note</FieldLabel>
            <Input
              id="count-note"
              placeholder="e.g. Shelf A, month end"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={start.isPending}>
              Start
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ------------------------------------------------------------------------------------- adjustment

interface AdjustRow extends ItemLine {
  direction: "out" | "in";
  unitCost: number | null;
  /** Stock out of a batch product: a chosen batch ("" = first to expire). */
  batchId: string;
  /** Stock in of a batch product: its batch and expiry. */
  batchNo: string;
  expiryDate: string;
}

/** The batch of an adjustment line: a choice for stock out, a number and expiry for stock in. */
function BatchCell({
  line,
  warehouseId,
  update,
}: {
  line: AdjustRow;
  warehouseId: string | null;
  update: (patch: Partial<AdjustRow>) => void;
}) {
  const batches = useCommand(
    "stock_batches",
    { variantId: line.item.variantId, warehouseId },
    { enabled: line.item.trackBatches && line.direction === "out" },
  );
  if (!line.item.trackBatches) return <span className="text-xs text-muted-foreground">—</span>;
  if (line.direction === "in") {
    return (
      <div className="flex min-w-48 gap-1">
        <Input
          aria-label="Batch"
          placeholder="Batch"
          value={line.batchNo}
          onChange={(e) => update({ batchNo: e.target.value })}
        />
        <DatePicker
          aria-label="Expiry"
          placeholder="Expiry"
          clearable
          value={line.expiryDate}
          onChange={(expiryDate) => update({ expiryDate })}
        />
      </div>
    );
  }
  return (
    <SelectField
      aria-label="Batch"
      className="min-w-48"
      value={line.batchId}
      onValueChange={(batchId) => update({ batchId })}
      options={[
        { value: "", label: "First to expire" },
        ...(batches.data ?? [])
          .filter((b) => b.batchId)
          .map((b) => ({
            value: b.batchId ?? "",
            label: `${b.batchNo ?? ""} · ${showQty(b.qty)}${b.expiryDate ? ` · exp ${showDate(b.expiryDate)}` : ""}${b.expired ? " (expired)" : ""}`,
          })),
      ]}
    />
  );
}

const REASONS = [
  "Damaged",
  "Expired",
  "Lost or stolen",
  "Found",
  "Own use",
  "Sample",
  "Correction",
];

export function AdjustmentPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [key] = useState(requestId);
  const [warehouseId, setWarehouseId] = useState("");
  const [reason, setReason] = useState(REASONS[0] ?? "");
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<AdjustRow[]>([]);
  const [busy, setBusy] = useState(false);
  const valid =
    lines.length > 0 && lines.every((l) => l.qty !== null && l.qty > 0) && reason.trim() !== "";

  const save = async () => {
    setBusy(true);
    try {
      const result = await call("stock_adjust", {
        idempotencyKey: key,
        warehouseId: warehouseId || null,
        reason,
        notes: notes || null,
        lines: lines.map((l) => ({
          variantId: l.item.variantId,
          qtyChange:
            Math.round(((l.qty ?? 0) * l.item.factorNum) / l.item.factorDen) *
            (l.direction === "out" ? -1 : 1),
          unitCost: l.direction === "in" ? l.unitCost : null,
          batchId: l.direction === "out" && l.batchId ? l.batchId : null,
          batchNo: l.direction === "in" && l.batchNo.trim() ? l.batchNo.trim() : null,
          expiryDate: l.direction === "in" && l.expiryDate ? l.expiryDate : null,
        })),
      });
      toast.success(`Saved ${result.docNo}`);
      if (result.warnings.length > 0)
        toast.warning(`Stock below zero: ${result.warnings.map((w) => w.name).join(", ")}`);
      void queryClient.invalidateQueries({ queryKey: ["stock_documents"] });
      void queryClient.invalidateQueries({ queryKey: ["products_list"] });
      void navigate(`/inventory/documents/adjustment/${result.id}`, { replace: true });
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Page
      title="Adjust stock"
      description="Write off damaged or lost goods, or add stock that was found."
      back={{ to: "/inventory/stock", label: "Stock moves" }}
      actions={
        <Button disabled={!valid || busy} onClick={() => void save()}>
          {busy ? "Saving…" : "Save adjustment"}
        </Button>
      }
    >
      <Section>
        <div className="grid gap-4 md:grid-cols-3">
          <Field>
            <FieldLabel htmlFor="adj-store">Store</FieldLabel>
            <WarehouseSelect id="adj-store" value={warehouseId} onChange={setWarehouseId} />
          </Field>
          <Field>
            <FieldLabel htmlFor="adj-reason">Reason</FieldLabel>
            <Input
              id="adj-reason"
              list="adj-reasons"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
            />
            <datalist id="adj-reasons">
              {REASONS.map((r) => (
                <option key={r} value={r} />
              ))}
            </datalist>
          </Field>
          <Field>
            <FieldLabel htmlFor="adj-notes">Notes</FieldLabel>
            <Input id="adj-notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
          </Field>
        </div>
      </Section>
      <Section title="Items">
        <ItemLines<AdjustRow>
          lines={lines}
          onChange={setLines}
          warehouseId={warehouseId || null}
          showStock
          makeLine={(item) => ({
            key: lineKey(),
            item,
            qty: 1000,
            direction: "out",
            unitCost: null,
            batchId: "",
            batchNo: "",
            expiryDate: "",
          })}
          columns={[
            {
              header: "Batch",
              cell: (l, update) => (
                <BatchCell line={l} warehouseId={warehouseId || null} update={update} />
              ),
            },
            {
              header: "Change",
              cell: (l, update) => (
                <SelectField
                  aria-label="In or out"
                  value={l.direction}
                  onValueChange={(v) => update({ direction: v as "in" | "out" })}
                  options={[
                    { value: "out", label: "Remove (−)" },
                    { value: "in", label: "Add (+)" },
                  ]}
                />
              ),
            },
            {
              header: "Cost each (for stock added)",
              align: "right",
              cell: (l, update) =>
                l.direction === "in" ? (
                  <MoneyInput
                    aria-label="Cost each"
                    placeholder="average cost"
                    value={l.unitCost}
                    onValueChange={(unitCost) => update({ unitCost })}
                  />
                ) : (
                  <span className="text-xs text-muted-foreground">at average cost</span>
                ),
            },
          ]}
        />
        <p className="mt-2 text-xs text-muted-foreground">
          Removed stock is valued at its average cost; the value is posted to the stock-adjustments
          account.
        </p>
      </Section>
    </Page>
  );
}

// ------------------------------------------------------------------------------------- transfer

export function TransferPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const stores = useCommand("stock_warehouses", {});
  const [key] = useState(requestId);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<ItemLine[]>([]);
  const [busy, setBusy] = useState(false);
  const fromId = from || stores.data?.find((w) => w.isDefault)?.id || "";
  const valid =
    fromId && to && fromId !== to && lines.length > 0 && lines.every((l) => (l.qty ?? 0) > 0);
  const save = async () => {
    setBusy(true);
    try {
      const result = await call("stock_transfer", {
        idempotencyKey: key,
        fromWarehouseId: fromId,
        toWarehouseId: to,
        notes: notes || null,
        lines: lines.map((l) => ({
          variantId: l.item.variantId,
          qty: Math.round(((l.qty ?? 0) * l.item.factorNum) / l.item.factorDen),
        })),
      });
      toast.success(`Moved: ${result.docNo}`);
      void queryClient.invalidateQueries({ queryKey: ["stock_documents"] });
      void navigate(`/inventory/documents/transfer/${result.id}`, { replace: true });
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Page
      title="Transfer stock"
      description="Move goods from one store to another (both stores update at once)."
      back={{ to: "/inventory/stock", label: "Stock moves" }}
      actions={
        <Button disabled={!valid || busy} onClick={() => void save()}>
          {busy ? "Saving…" : "Transfer"}
        </Button>
      }
    >
      <Section>
        <div className="grid gap-4 md:grid-cols-3">
          <Field>
            <FieldLabel htmlFor="tr-from">From</FieldLabel>
            <SelectField
              id="tr-from"
              className="w-full"
              value={fromId}
              onValueChange={setFrom}
              options={[...(stores.data ?? []).map((w) => ({ value: w.id, label: w.name }))]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="tr-to">To</FieldLabel>
            <WarehouseSelect
              id="tr-to"
              value={to}
              onChange={setTo}
              allowDefault={false}
              exclude={fromId}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="tr-notes">Notes</FieldLabel>
            <Input id="tr-notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
          </Field>
        </div>
        {stores.data && stores.data.length < 2 && (
          <p className="mt-3 text-sm text-muted-foreground">
            Add a second store under{" "}
            <Link className="underline" to="/inventory/masters">
              Categories & units → Stores
            </Link>{" "}
            first.
          </p>
        )}
      </Section>
      <Section title="Items">
        <ItemLines
          lines={lines}
          onChange={setLines}
          warehouseId={fromId || null}
          showStock
          makeLine={(item) => ({ key: lineKey(), item, qty: 1000 })}
        />
      </Section>
    </Page>
  );
}

// ------------------------------------------------------------------------------------- views

export function StockDocumentPage() {
  const { kind = "adjustment", id = "" } = useParams();
  const money = useMoney();
  const doc = useCommand("stock_document_get", { kind, id });
  return (
    <Page
      title={doc.data?.docNo ?? label(kind)}
      back={{ to: "/inventory/stock", label: "Stock moves" }}
    >
      <QueryView query={doc}>
        {(d) => (
          <>
            <Section>
              <Facts
                items={[
                  ["Type", label(kind)],
                  ["Date", showDate(d.businessDate)],
                  ["Items", d.lines.length],
                ]}
              />
            </Section>
            <Section title="Items">
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1 text-left font-medium">Item</th>
                    <th className="py-1 text-right font-medium">Qty</th>
                    <th className="py-1 text-right font-medium">Cost each</th>
                    <th className="py-1 text-right font-medium">Value</th>
                  </tr>
                </thead>
                <tbody>
                  {d.lines.map((l) => (
                    <tr key={l.variantId} className="border-t border-border/40">
                      <td className="py-1.5">
                        <Link
                          to={`/inventory/stock-card/${l.variantId}`}
                          className="font-medium hover:underline"
                        >
                          {l.name}
                        </Link>
                        <span className="ml-2 text-xs text-muted-foreground">{l.sku}</span>
                      </td>
                      <td className="text-right tabular-nums">{showQty(l.qty)}</td>
                      <td className="text-right tabular-nums">{money(l.unitCost)}</td>
                      <td className="text-right tabular-nums">{money(l.value)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Section>
          </>
        )}
      </QueryView>
    </Page>
  );
}

export function StockCardPage() {
  const { variantId = "" } = useParams();
  const money = useMoney();
  const table = useTableState(`stock-card-${variantId}`, {
    size: 50,
    filters: { store: "", newest: "" },
  });
  const warehouseId = table.filter("store");
  const card = useList("stock_card", {
    variantId,
    warehouseId: warehouseId || null,
    page: table.state.page,
    pageSize: table.state.size,
    descending: table.filter("newest") === "1",
  });
  const sourceLink = (l: Api.StockCardLine): string | null => {
    switch (l.sourceType) {
      case "sales_invoice":
        return `/sales/invoices/${l.sourceId}`;
      case "sales_return":
        return `/sales/returns/${l.sourceId}`;
      case "stock_adjustment":
        return `/inventory/documents/adjustment/${l.sourceId}`;
      case "stock_transfer":
        return `/inventory/documents/transfer/${l.sourceId}`;
      case "stock_count":
        return `/inventory/counts/${l.sourceId}`;
      case "goods_receipt":
        return `/purchases/receipts/${l.sourceId}`;
      case "purchase_bill":
        return `/purchases/bills/${l.sourceId}`;
      case "purchase_return":
        return `/purchases/returns/${l.sourceId}`;
      default:
        return null;
    }
  };
  return (
    <Page
      title={card.data ? `Stock card · ${card.data.name}` : "Stock card"}
      back={{ to: "/inventory/products", label: "Products" }}
    >
      <div className="flex flex-wrap items-end gap-4">
        <div className="w-64">
          <WarehouseSelect
            value={warehouseId}
            onChange={(v) => table.setFilter("store", v)}
            label="Store"
          />
        </div>
        <label className="flex items-center gap-2 pb-2 text-sm">
          <Checkbox
            checked={table.filter("newest") === "1"}
            onCheckedChange={(v) => table.setFilter("newest", v ? "1" : "")}
          />
          Newest first
        </label>
      </div>
      <QueryView query={card}>
        {(c) => (
          <Section
            title={`${c.sku} · opening ${showQty(c.openingQty)} · now ${showQty(c.closingQty)}`}
          >
            {c.lines.length === 0 ? (
              <p className="text-sm text-muted-foreground">No movements.</p>
            ) : (
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1 text-left font-medium">Date</th>
                    <th className="py-1 text-left font-medium">Movement</th>
                    <th className="py-1 text-left font-medium">Document</th>
                    <th className="py-1 text-left font-medium">Store</th>
                    <th className="py-1 text-right font-medium">Qty</th>
                    <th className="py-1 text-right font-medium">Cost</th>
                    <th className="py-1 text-right font-medium">Balance</th>
                  </tr>
                </thead>
                <tbody>
                  {c.lines.map((l, i) => {
                    const to = sourceLink(l);
                    return (
                      <tr key={i} className="border-t border-border/40">
                        <td className="py-1.5">{showDate(l.businessDate)}</td>
                        <td>{label(l.movementType)}</td>
                        <td>
                          {to ? (
                            <Link to={to} className="hover:underline">
                              {l.docNo ?? "—"}
                            </Link>
                          ) : (
                            (l.docNo ?? "—")
                          )}
                        </td>
                        <td className="text-muted-foreground">{l.warehouseName}</td>
                        <td
                          className={`text-right tabular-nums ${l.qty < 0 ? "text-destructive" : ""}`}
                        >
                          {showQty(l.qty)}
                        </td>
                        <td className="text-right tabular-nums">{money(l.unitCost)}</td>
                        <td className="text-right font-medium tabular-nums">
                          {showQty(l.balance)}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
            <div className="mt-3">
              <Pager {...table.paging(c.total)} />
            </div>
          </Section>
        )}
      </QueryView>
    </Page>
  );
}

// ------------------------------------------------------------------------------------- count

export function CountPage() {
  const { id = "" } = useParams();
  const money = useMoney();
  const queryClient = useQueryClient();
  const count = useCommand("stock_count_get", { id });
  const [entered, setEntered] = useState<Record<string, number | null>>({});
  const [filter, setFilter] = useState("");
  const [finalizeKey] = useState(requestId);
  const save = useAction("stock_count_enter", {
    success: "Counts saved",
    onSuccess: (d) => {
      queryClient.setQueryData(["stock_count_get", { id }], d);
      setEntered({});
    },
  });
  const finalize = useAction("stock_count_finalize", {
    invalidates: ["stock_documents", "products_list"],
    success: "Count finalized; differences posted",
    onSuccess: (d) => queryClient.setQueryData(["stock_count_get", { id }], d),
  });
  const pending = Object.keys(entered).length;
  return (
    <Page
      title={count.data ? `Count ${count.data.docNo}` : "Stock count"}
      back={{ to: "/inventory/stock", label: "Stock moves" }}
    >
      <QueryView query={count}>
        {(c) => {
          const open = c.status !== "finalized" && c.status !== "voided";
          const rows = c.lines.filter(
            (l) => !filter || `${l.name} ${l.sku}`.toLowerCase().includes(filter.toLowerCase()),
          );
          return (
            <>
              <Section>
                <Facts
                  items={[
                    ["Status", <StatusBadge key="s" status={c.status} />],
                    ["Date", showDate(c.businessDate)],
                    ["Counted", `${c.countedLines} of ${c.lines.length}`],
                    ["Difference in value", money(c.totalVarianceValue)],
                  ]}
                />
                {c.scopeNote && <p className="mt-3 text-sm text-muted-foreground">{c.scopeNote}</p>}
              </Section>
              <Section
                title="Items"
                actions={
                  open && (
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        disabled={pending === 0 || save.isPending}
                        onClick={() =>
                          save.mutate({
                            input: {
                              countId: c.id,
                              entries: Object.entries(entered).map(([lineId, countedQty]) => ({
                                lineId,
                                countedQty,
                              })),
                            },
                          })
                        }
                      >
                        Save counts{pending > 0 ? ` (${pending})` : ""}
                      </Button>
                      <Button
                        disabled={pending > 0 || c.countedLines === 0 || finalize.isPending}
                        onClick={run(async () => {
                          const ok = await confirm({
                            title: "Finalize this count?",
                            description: `Items not counted are left unchanged. Differences worth ${money(c.totalVarianceValue)} are posted as an adjustment. This can't be undone.`,
                            confirmLabel: "Finalize",
                          });
                          if (ok !== false)
                            finalize.mutate({
                              input: { countId: c.id, idempotencyKey: finalizeKey },
                            });
                        })}
                      >
                        Finalize
                      </Button>
                    </div>
                  )
                }
              >
                <Input
                  aria-label="Filter items"
                  placeholder="Filter items"
                  className="mb-3 max-w-xs"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                />
                <table className="w-full text-sm">
                  <thead className="text-xs text-muted-foreground">
                    <tr>
                      <th className="py-1 text-left font-medium">Item</th>
                      <th className="py-1 text-right font-medium">System</th>
                      <th className="w-32 py-1 text-right font-medium">Counted</th>
                      <th className="py-1 text-right font-medium">Difference</th>
                      <th className="py-1 text-right font-medium">Value</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((l) => {
                      const value =
                        l.id in entered ? (entered[l.id] ?? null) : (l.countedQty ?? null);
                      const variance = value === null ? null : value - l.systemQty;
                      return (
                        <tr key={l.id} className="border-t border-border/40">
                          <td className="py-1.5">
                            <span className="font-medium">{l.name}</span>
                            <span className="ml-2 text-xs text-muted-foreground">{l.sku}</span>
                            {l.batchNo && (
                              <span className="block text-xs text-muted-foreground">
                                Batch {l.batchNo}
                                {l.expiryDate && ` · exp ${showDate(l.expiryDate)}`}
                              </span>
                            )}
                          </td>
                          <td className="text-right tabular-nums">{showQty(l.systemQty)}</td>
                          <td className="py-1">
                            {open ? (
                              <QtyInput
                                aria-label={`Counted ${l.name}`}
                                value={value}
                                onValueChange={(v) => setEntered((e) => ({ ...e, [l.id]: v }))}
                              />
                            ) : (
                              <span className="block text-right tabular-nums">
                                {value === null ? "—" : showQty(value)}
                              </span>
                            )}
                          </td>
                          <td
                            className={`text-right tabular-nums ${variance && variance < 0 ? "text-destructive" : ""}`}
                          >
                            {variance === null ? "" : showQty(variance)}
                          </td>
                          <td className="text-right tabular-nums">
                            {l.varianceValue != null ? money(l.varianceValue) : ""}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </Section>
            </>
          );
        }}
      </QueryView>
    </Page>
  );
}
