import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { FileText, Pencil, Plus, Printer, Receipt } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { StatusBadge } from "@/components/app/badges";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import type { ItemLine } from "@/components/app/item-lines";
import { ItemLines, lineKey } from "@/components/app/item-lines";
import { MoneyInput, RateInput } from "@/components/app/inputs";
import { Facts, Page, QueryView, Section } from "@/components/app/page";
import { PartyPicker, useDebounced } from "@/components/app/pickers";
import { call, requestId } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { label, showDate, showQty } from "@/lib/format";
import { errorText, useAction, useCommand, useList } from "@/lib/queries";
import { useCan } from "@/stores/session";
import { PaymentDialog } from "@/features/pos/payment-dialog";
import { printHtml, salesDocHtml } from "./print";

const TYPES = [
  ["quotation", "Quotations"],
  ["sales_order", "Sales orders"],
  ["delivery_challan", "Delivery challans"],
] as const;

type DocType = (typeof TYPES)[number][0];

function typeName(t: string): string {
  return (
    { quotation: "quotation", sales_order: "sales order", delivery_challan: "delivery challan" }[
      t
    ] ?? t
  );
}

// ------------------------------------------------------------------------------------- list

export function SalesDocsPage() {
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("quotations.manage");
  const [params, setParams] = useSearchParams();
  const docType = (params.get("type") ?? "quotation") as DocType;
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const term = useDebounced(search.trim(), 200);
  const query = useList("sales_docs_list", { docType, search: term || null, page, pageSize: 100 });
  const columns = useMemo<ColumnDef<Api.SalesDocListItem>[]>(
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
      { header: "Customer", accessorKey: "customerName" },
      {
        header: "Valid until",
        accessorFn: (r) => r.validUntil ?? "",
        cell: ({ row }) => showDate(row.original.validUntil),
      },
      {
        header: "Status",
        accessorKey: "status",
        cell: ({ row }) => <StatusBadge status={row.original.status} />,
      },
      {
        header: "Total",
        accessorKey: "total",
        meta: { align: "right" },
        cell: ({ row }) => money(row.original.total),
      },
    ],
    [money],
  );
  return (
    <Page
      title="Quotations & orders"
      actions={
        canManage && (
          <Button
            render={<Link to={`/sales/documents/new?type=${docType}`} />}
            nativeButton={false}
          >
            <Plus /> New {typeName(docType)}
          </Button>
        )
      }
    >
      <Tabs
        value={docType}
        onValueChange={(v) => {
          setParams({ type: String(v) });
          setPage(1);
        }}
      >
        <TabsList>
          {TYPES.map(([value, text]) => (
            <TabsTrigger key={value} value={value}>
              {text}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      <SearchInput
        aria-label="Search"
        placeholder="Number or customer"
        className="max-w-xs"
        value={search}
        onChange={(e) => {
          setSearch(e.target.value);
          setPage(1);
        }}
      />
      <DataTable
        viewKey="sales-documents"
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        onRowClick={(r) => void navigate(`/sales/documents/${r.id}`)}
        paging={
          query.data ? { page, pageSize: 100, total: query.data.total, onPage: setPage } : undefined
        }
        empty={{ title: `No ${typeName(docType)}s yet` }}
      />
    </Page>
  );
}

// ------------------------------------------------------------------------------------- editor

interface DocRow extends ItemLine {
  unitPrice: number | null;
  discountRate: number | null;
}

/** Search item rebuilt from a saved line (enough to edit and resend it). */
function itemOf(l: Api.SalesDocLine): Api.SearchItem {
  return {
    variantId: l.variantId,
    productId: "",
    name: l.itemName,
    sku: "",
    barcode: null,
    unitId: l.unitId,
    unitCode: "",
    factorNum: 1,
    factorDen: 1,
    salePrice: l.unitPrice,
    mrp: null,
    taxRateId: null,
    taxRate: 0,
    taxType: null,
    cessRate: 0,
    hsnSac: null,
    stock: 0,
    isService: false,
    trackBatches: false,
  };
}

function rowsOf(doc: Api.SalesDocDetail | null): DocRow[] {
  return (doc?.lines ?? []).map((l) => ({
    key: lineKey(),
    item: itemOf(l),
    qty: l.qty,
    unitPrice: l.unitPrice,
    discountRate:
      l.discount > 0 && l.unitPrice > 0
        ? Math.round((l.discount * 1_000_000) / (l.unitPrice * (l.qty / 1000)))
        : null,
  }));
}

function saleLines(rows: DocRow[]): Api.SaleLineInput[] {
  return rows
    .filter((r) => (r.qty ?? 0) > 0)
    .map((r) => ({
      variantId: r.item.variantId,
      unitId: r.item.unitId,
      qty: r.qty ?? 0,
      unitPrice: r.unitPrice,
      discount: r.discountRate ? { kind: "percent", rate: r.discountRate } : null,
      batchId: null,
    }));
}

function Editor({ doc, docType }: { doc: Api.SalesDocDetail | null; docType: string }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const money = useMoney();
  const [party, setParty] = useState<{ id: string; name: string } | null>(
    doc?.partyId ? { id: doc.partyId, name: doc.customerName } : null,
  );
  const [customerName, setCustomerName] = useState(doc && !doc.partyId ? doc.customerName : "");
  const [validUntil, setValidUntil] = useState(doc?.validUntil ?? "");
  const [notes, setNotes] = useState(doc?.notes ?? "");
  const [terms, setTerms] = useState(doc?.terms ?? "");
  const [rows, setRows] = useState<DocRow[]>(() => rowsOf(doc));
  const [busy, setBusy] = useState(false);
  const lines = useDebounced(saleLines(rows), 150);
  const preview = useCommand(
    "sales_preview",
    { lines, partyId: party?.id ?? null },
    { enabled: lines.length > 0, retry: false },
  );
  const save = async () => {
    setBusy(true);
    try {
      const saved = await call("sales_docs_save", {
        id: doc?.id ?? null,
        docType,
        partyId: party?.id ?? null,
        customerName: party ? null : customerName || null,
        validUntil: validUntil || null,
        notes: notes || null,
        terms: terms || null,
        lines: saleLines(rows),
      });
      toast.success(`${saved.docNo} saved`);
      void queryClient.invalidateQueries({ queryKey: ["sales_docs_list"] });
      void queryClient.invalidateQueries({ queryKey: ["sales_docs_get"] });
      void navigate(`/sales/documents/${saved.id}`, { replace: true });
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Page
      title={doc ? `Edit ${doc.docNo}` : `New ${typeName(docType)}`}
      back={{ to: `/sales/documents?type=${docType}`, label: "Quotations & orders" }}
      actions={
        <Button disabled={busy || rows.length === 0} onClick={() => void save()}>
          {busy ? "Saving…" : "Save"}
        </Button>
      }
    >
      <Section>
        <div className="grid gap-4 md:grid-cols-3">
          <Field>
            <FieldLabel>Customer</FieldLabel>
            {party ? (
              <div className="flex items-center gap-2">
                <span className="font-medium">{party.name}</span>
                <Button variant="ghost" size="xs" onClick={() => setParty(null)}>
                  Change
                </Button>
              </div>
            ) : (
              <PartyPicker
                partyType="customer"
                onSelect={(p) => setParty({ id: p.id, name: p.name })}
              />
            )}
          </Field>
          {!party && (
            <Field>
              <FieldLabel htmlFor="doc-name">Or a name for a one-off customer</FieldLabel>
              <Input
                id="doc-name"
                value={customerName}
                onChange={(e) => setCustomerName(e.target.value)}
              />
            </Field>
          )}
          {docType === "quotation" && (
            <Field>
              <FieldLabel htmlFor="doc-valid">Valid until</FieldLabel>
              <Input
                id="doc-valid"
                type="date"
                value={validUntil}
                onChange={(e) => setValidUntil(e.target.value)}
              />
            </Field>
          )}
        </div>
      </Section>
      <Section title="Items">
        <ItemLines<DocRow>
          lines={rows}
          onChange={setRows}
          makeLine={(item) => ({
            key: lineKey(),
            item,
            qty: 1000,
            unitPrice: item.salePrice,
            discountRate: null,
          })}
          columns={[
            {
              header: "Price",
              align: "right",
              className: "w-36",
              cell: (l, update) => (
                <MoneyInput
                  aria-label="Price"
                  value={l.unitPrice}
                  onValueChange={(unitPrice) => update({ unitPrice })}
                />
              ),
            },
            {
              header: "Discount",
              align: "right",
              className: "w-28",
              cell: (l, update) => (
                <RateInput
                  aria-label="Discount"
                  value={l.discountRate}
                  onValueChange={(discountRate) => update({ discountRate })}
                />
              ),
            },
            {
              header: "Amount",
              align: "right",
              cell: (l) => {
                const i = rows.indexOf(l);
                const pl = preview.data?.lines[i];
                return pl ? money(pl.total) : "…";
              },
            },
          ]}
        />
        {preview.data && (
          <dl className="mt-3 ml-auto grid w-64 grid-cols-2 gap-1 text-sm">
            <dt className="text-muted-foreground">Taxable</dt>
            <dd className="text-right tabular-nums">{money(preview.data.taxable)}</dd>
            <dt className="text-muted-foreground">Tax</dt>
            <dd className="text-right tabular-nums">{money(preview.data.tax)}</dd>
            <dt className="font-semibold">Total</dt>
            <dd className="text-right font-semibold tabular-nums">{money(preview.data.total)}</dd>
          </dl>
        )}
        {preview.error && (
          <p className="mt-2 text-sm text-destructive">{errorText(preview.error)}</p>
        )}
      </Section>
      <Section>
        <div className="grid gap-4 md:grid-cols-2">
          <Field>
            <FieldLabel htmlFor="doc-notes">Notes</FieldLabel>
            <Textarea id="doc-notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
          </Field>
          <Field>
            <FieldLabel htmlFor="doc-terms">Terms</FieldLabel>
            <Textarea id="doc-terms" value={terms} onChange={(e) => setTerms(e.target.value)} />
          </Field>
        </div>
      </Section>
    </Page>
  );
}

export function SalesDocEditor() {
  const { id } = useParams();
  const [params] = useSearchParams();
  const isNew = !id || id === "new";
  const doc = useCommand("sales_docs_get", { id: id ?? "" }, { enabled: !isNew });
  if (isNew) return <Editor doc={null} docType={params.get("type") ?? "quotation"} />;
  return <QueryView query={doc}>{(d) => <Editor doc={d} docType={d.docType} />}</QueryView>;
}

// ------------------------------------------------------------------------------------- view

export function SalesDocPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const money = useMoney();
  const settings = usePublicSettings();
  const canManage = useCan("quotations.manage");
  const canSell = useCan("sales.create");
  const doc = useCommand("sales_docs_get", { id });
  const register = useCommand("register_current", {}, { enabled: canSell });
  const [paying, setPaying] = useState(false);
  const [busy, setBusy] = useState(false);
  const [convertKey, setConvertKey] = useState(requestId);
  useEffect(() => setConvertKey(requestId()), [id]);
  const setStatus = useAction("sales_docs_set_status", {
    invalidates: ["sales_docs_get", "sales_docs_list"],
    success: "Status changed",
  });
  return (
    <QueryView query={doc}>
      {(d) => {
        const open = d.status === "open" || d.status === "accepted";
        const convert = async (payments: Api.PaymentInput[]) => {
          setBusy(true);
          try {
            const invoice = await call("sales_docs_convert", {
              documentId: d.id,
              idempotencyKey: convertKey,
              payments,
              expectedTotal: d.total,
              registerSessionId: register.data?.id ?? null,
            });
            toast.success(`Invoice ${invoice.docNo} created`);
            for (const k of ["sales_docs_get", "sales_docs_list", "sales_list_invoices"])
              void queryClient.invalidateQueries({ queryKey: [k] });
            void navigate(`/sales/invoices/${invoice.id}`);
          } catch (e) {
            toast.error(errorText(e));
          } finally {
            setBusy(false);
            setPaying(false);
          }
        };
        return (
          <Page
            title={d.docNo}
            description={<StatusBadge status={d.status} />}
            back={{ to: `/sales/documents?type=${d.docType}`, label: "Quotations & orders" }}
            actions={
              <>
                <Button variant="outline" onClick={() => printHtml(salesDocHtml(d, settings.data))}>
                  <Printer /> Print
                </Button>
                {open && canManage && (
                  <Button
                    variant="outline"
                    render={<Link to={`/sales/documents/${d.id}/edit`} />}
                    nativeButton={false}
                  >
                    <Pencil /> Edit
                  </Button>
                )}
                {open && canManage && (
                  <NativeSelect
                    aria-label="Change status"
                    value=""
                    onChange={(e) =>
                      e.target.value &&
                      setStatus.mutate({ input: { id: d.id, status: e.target.value } })
                    }
                  >
                    <NativeSelectOption value="">Mark as…</NativeSelectOption>
                    {["open", "accepted", "declined", "cancelled"]
                      .filter((s) => s !== d.status)
                      .map((s) => (
                        <NativeSelectOption key={s} value={s}>
                          {label(s)}
                        </NativeSelectOption>
                      ))}
                  </NativeSelect>
                )}
                {open && canSell && (
                  <Button onClick={() => setPaying(true)}>
                    <Receipt /> Make invoice
                  </Button>
                )}
              </>
            }
          >
            <Section>
              <Facts
                items={[
                  ["Type", label(d.docType)],
                  ["Date", showDate(d.businessDate)],
                  [
                    "Customer",
                    d.partyId ? (
                      <Link key="c" to={`/parties/${d.partyId}`} className="hover:underline">
                        {d.customerName}
                      </Link>
                    ) : (
                      d.customerName
                    ),
                  ],
                  ["Total", money(d.total)],
                ]}
              />
              {d.convertedInvoiceId && (
                <Link
                  to={`/sales/invoices/${d.convertedInvoiceId}`}
                  className="mt-3 inline-flex items-center gap-1 text-sm underline"
                >
                  <FileText className="size-4" aria-hidden /> Invoiced
                </Link>
              )}
            </Section>
            <Section title="Items">
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
                  {d.lines.map((l, i) => (
                    <tr key={i} className="border-t border-border/40">
                      <td className="py-1.5 font-medium">{l.itemName}</td>
                      <td className="text-right tabular-nums">{showQty(l.qty)}</td>
                      <td className="text-right tabular-nums">{money(l.unitPrice)}</td>
                      <td className="text-right tabular-nums">
                        {l.discount ? money(l.discount) : ""}
                      </td>
                      <td className="text-right tabular-nums">{money(l.taxTotal)}</td>
                      <td className="text-right font-medium tabular-nums">{money(l.lineTotal)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {d.notes && <p className="mt-3 text-sm text-muted-foreground">{d.notes}</p>}
            </Section>
            <PaymentDialog
              open={paying}
              total={d.total}
              creditAllowed={Boolean(d.partyId)}
              busy={busy}
              onClose={() => setPaying(false)}
              onPay={(payments) => void convert(payments)}
            />
          </Page>
        );
      }}
    </QueryView>
  );
}
