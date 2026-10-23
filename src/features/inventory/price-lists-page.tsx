import { useEffect, useState } from "react";
import type { Api } from "@inventory/shared";
import { Pencil, Percent, Plus, Tags, Trash2 } from "lucide-react";
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
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput, QtyInput, RateInput } from "@/components/app/inputs";
import { EmptyState, Page, QueryView, Section } from "@/components/app/page";
import { ProductPicker } from "@/components/app/pickers";
import { SearchInput } from "@/components/app/search-input";
import { useMoney } from "@/lib/business";
import { label, showDate, showQty } from "@/lib/format";
import { fieldError, useAction, useCommand, useList } from "@/lib/queries";
import { useDebounced } from "@/lib/use-debounced";
import { useCan } from "@/stores/session";

const KINDS = [
  { value: "retail", label: "Retail" },
  { value: "wholesale", label: "Wholesale" },
  { value: "custom", label: "Custom" },
];

function newList(): Api.PriceListInput {
  return { id: null, name: "", kind: "wholesale", isDefault: false, isActive: true, notes: null };
}

/** Price lists: retail, wholesale and custom prices, for customers and tills. */
export default function PriceListsPage() {
  const canManage = useCan("price_lists.manage");
  const canChangeBase = useCan("products.price_change");
  const lists = useCommand("price_lists_list", {});
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<Api.PriceListInput | null>(null);
  const [bulk, setBulk] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    const first = lists.data?.[0];
    if (!selected && first) setSelected(first.id);
  }, [lists.data, selected]);
  const current = lists.data?.find((l) => l.id === selected);
  const remove = useAction("price_lists_delete", {
    invalidates: ["price_lists_list"],
    success: "Price list deleted",
    onSuccess: () => setSelected(null),
  });
  return (
    <Page
      title="Price lists"
      description="Prices by customer and till: per unit or pack, by quantity, from a date. A customer's list wins over the till's, the till's over the default list."
      actions={
        <>
          {canChangeBase && (
            <Button variant="outline" onClick={() => setBulk(null)}>
              <Percent /> Change product prices
            </Button>
          )}
          {canManage && (
            <Button onClick={() => setEditing(newList())}>
              <Plus /> New price list
            </Button>
          )}
        </>
      }
    >
      <div className="grid gap-4 lg:grid-cols-[18rem_1fr]">
        <div className="flex flex-col gap-4">
          <Section title="Lists">
            <QueryView query={lists}>
              {(rows) =>
                rows.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    No price lists yet: every bill uses the product's own price.
                  </p>
                ) : (
                  <ul className="flex flex-col gap-1" aria-label="Price lists">
                    {rows.map((l) => (
                      <li key={l.id}>
                        <button
                          type="button"
                          aria-current={l.id === selected}
                          onClick={() => setSelected(l.id)}
                          className={`flex w-full flex-col rounded-lg px-3 py-2 text-left text-sm hover:bg-muted ${
                            l.id === selected ? "bg-muted" : ""
                          }`}
                        >
                          <span className="flex items-center gap-1.5 font-medium">
                            {l.name}
                            {l.isDefault && <Badge variant="default">Default</Badge>}
                            {!l.isActive && <Badge variant="outline">Off</Badge>}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {label(l.kind)} · {l.itemCount} prices · {l.customerCount} customers ·{" "}
                            {l.tillCount} tills
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )
              }
            </QueryView>
          </Section>
          <TillsSection lists={lists.data ?? []} canManage={canManage} />
        </div>
        {current ? (
          <PricesSection
            list={current}
            canManage={canManage}
            onEdit={() =>
              setEditing({
                id: current.id,
                expectedUpdatedAt: current.updatedAt,
                name: current.name,
                kind: current.kind,
                isDefault: current.isDefault,
                isActive: current.isActive,
                notes: current.notes ?? null,
              })
            }
            onBulk={() => setBulk(current.id)}
            onDelete={() =>
              void confirm({
                title: `Delete ${current.name}?`,
                description:
                  "Its prices are removed too. Customers and tills must use another list first.",
                confirmLabel: "Delete",
                destructive: true,
              }).then((ok) => ok !== false && remove.mutate({ input: { id: current.id } }))
            }
          />
        ) : (
          <EmptyState
            title="Choose or create a price list"
            description="Wholesale customers, a second shop counter or a festival offer can each have their own prices."
          />
        )}
      </div>
      <ListDialog
        value={editing}
        onClose={() => setEditing(null)}
        onSaved={(id) => setSelected(id)}
      />
      {bulk !== undefined && <BulkDialog priceListId={bulk} onClose={() => setBulk(undefined)} />}
    </Page>
  );
}

function ListDialog({
  value,
  onClose,
  onSaved,
}: {
  value: Api.PriceListInput | null;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const [shown, setShown] = useState(value);
  if (value !== shown) {
    setShown(value);
    if (value) setDraft(value);
  }
  const save = useAction("price_lists_save", {
    invalidates: ["price_lists_list"],
    success: "Price list saved",
    onSuccess: (l) => {
      onSaved(l.id);
      onClose();
    },
  });
  const set = (patch: Partial<Api.PriceListInput>) => draft && setDraft({ ...draft, ...patch });
  return (
    <Dialog open={value !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{value?.id ? "Edit price list" : "New price list"}</DialogTitle>
          <DialogDescription>Prices are added on the list itself after saving.</DialogDescription>
        </DialogHeader>
        {draft && (
          <form
            id="price-list-form"
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate({ input: draft });
            }}
          >
            <Field>
              <FieldLabel htmlFor="pl-name">Name</FieldLabel>
              <Input
                id="pl-name"
                value={draft.name}
                maxLength={80}
                autoFocus
                onChange={(e) => set({ name: e.target.value })}
              />
              <FieldError>{fieldError(save.error, "name")}</FieldError>
            </Field>
            <Field>
              <FieldLabel htmlFor="pl-kind">Kind</FieldLabel>
              <SelectField
                id="pl-kind"
                value={draft.kind}
                onValueChange={(kind) => set({ kind })}
                options={KINDS}
              />
            </Field>
            <label className="flex items-start gap-2 text-sm">
              <Checkbox
                checked={draft.isDefault ?? false}
                onCheckedChange={(v) => set({ isDefault: v })}
              />
              <span>
                Default list
                <span className="block text-xs text-muted-foreground">
                  Used for walk-in customers at tills without a list of their own.
                </span>
              </span>
            </label>
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={draft.isActive ?? true}
                onCheckedChange={(v) => set({ isActive: v })}
              />
              In use
            </label>
            <FieldError>
              {fieldError(save.error, "isActive") ?? fieldError(save.error, "isDefault")}
            </FieldError>
          </form>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form="price-list-form" disabled={save.isPending}>
            {save.isPending ? "Saving…" : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface PriceDraft {
  id: string | null;
  variantId: string;
  name: string;
  unitId: string | null;
  unitCode: string;
  minQty: number | null;
  price: number | null;
  validFrom: string;
  validTo: string;
}

function PricesSection({
  list,
  canManage,
  onEdit,
  onBulk,
  onDelete,
}: {
  list: Api.PriceListSummary;
  canManage: boolean;
  onEdit: () => void;
  onBulk: () => void;
  onDelete: () => void;
}) {
  const money = useMoney();
  const [search, setSearch] = useState("");
  const term = useDebounced(search.trim(), 200);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(25);
  const [draft, setDraft] = useState<PriceDraft | null>(null);
  const items = useList("price_lists_items", {
    priceListId: list.id,
    search: term || null,
    page,
    pageSize,
  });
  const save = useAction("price_lists_set_prices", {
    invalidates: ["price_lists_items", "price_lists_list", "sales_preview"],
    success: "Price saved",
    onSuccess: () => setDraft(null),
  });
  const removePrice = (id: string) =>
    save.mutate({ input: { priceListId: list.id, upsert: [], remove: [id] } });
  const columns: ColumnDef<Api.PriceListItemRow>[] = [
    {
      id: "name",
      header: "Item",
      cell: ({ row }) => (
        <span className="flex flex-col">
          <span className="font-medium">{row.original.name}</span>
          <span className="text-xs text-muted-foreground">{row.original.sku}</span>
        </span>
      ),
    },
    { id: "unit", header: "Per", cell: ({ row }) => row.original.unitCode },
    {
      id: "minQty",
      header: "From qty",
      meta: { align: "right" },
      cell: ({ row }) => (row.original.minQty > 0 ? showQty(row.original.minQty) : "any"),
    },
    {
      id: "price",
      header: "Price",
      meta: { align: "right" },
      cell: ({ row }) => <span className="font-medium">{money(row.original.price)}</span>,
    },
    {
      id: "base",
      header: "Product's price",
      meta: { align: "right" },
      cell: ({ row }) => (
        <span className="text-muted-foreground">{money(row.original.basePrice)}</span>
      ),
    },
    {
      id: "valid",
      header: "Valid",
      cell: ({ row }) =>
        row.original.validFrom || row.original.validTo
          ? `${row.original.validFrom ? showDate(row.original.validFrom) : "…"} – ${
              row.original.validTo ? showDate(row.original.validTo) : "…"
            }`
          : "always",
    },
    {
      id: "actions",
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) =>
        canManage ? (
          <span className="flex justify-end gap-1">
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={`Edit price of ${row.original.name}`}
              onClick={() =>
                setDraft({
                  id: row.original.id,
                  variantId: row.original.variantId,
                  name: row.original.name,
                  unitId: row.original.unitId ?? null,
                  unitCode: row.original.unitCode,
                  minQty: row.original.minQty,
                  price: row.original.price,
                  validFrom: row.original.validFrom ?? "",
                  validTo: row.original.validTo ?? "",
                })
              }
            >
              <Pencil />
            </Button>
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={`Remove price of ${row.original.name}`}
              onClick={() => removePrice(row.original.id)}
            >
              <Trash2 />
            </Button>
          </span>
        ) : null,
    },
  ];
  return (
    <Section
      title={
        <span className="flex items-center gap-2">
          <Tags className="size-4" aria-hidden /> {list.name}
        </span>
      }
      actions={
        canManage && (
          <span className="flex gap-1">
            <Button size="sm" variant="outline" onClick={onBulk}>
              <Percent /> Change by %
            </Button>
            <Button size="sm" variant="ghost" onClick={onEdit} aria-label="Edit list">
              <Pencil />
            </Button>
            <Button size="sm" variant="ghost" onClick={onDelete} aria-label="Delete list">
              <Trash2 />
            </Button>
          </span>
        )
      }
    >
      <div className="mb-3 grid gap-2 sm:grid-cols-2">
        <SearchInput
          aria-label="Search this list"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setPage(1);
          }}
          placeholder="Search this list"
        />
        {canManage && (
          <ProductPicker
            placeholder="Add a price: search or scan a product"
            onSelect={(i) =>
              setDraft({
                id: null,
                variantId: i.variantId,
                name: i.name,
                unitId: i.factorNum === 1 && i.factorDen === 1 ? null : i.unitId,
                unitCode: i.unitCode,
                minQty: 0,
                price: i.salePrice,
                validFrom: "",
                validTo: "",
              })
            }
          />
        )}
      </div>
      <QueryView query={items}>
        {(data) => (
          <DataTable
            columns={columns}
            data={data.items}
            rowKey={(r) => r.id}
            empty={{
              title: "No prices on this list yet",
              description: "Products without a price here sell at their own price.",
            }}
            paging={{
              page: data.page,
              pageSize: data.pageSize,
              total: data.total,
              onPage: setPage,
              onPageSize: (n: number) => {
                setPageSize(n);
                setPage(1);
              },
            }}
          />
        )}
      </QueryView>
      <Dialog open={draft !== null} onOpenChange={(o) => !o && setDraft(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{draft?.name}</DialogTitle>
            <DialogDescription>
              Price per {draft?.unitCode} on {list.name}.
            </DialogDescription>
          </DialogHeader>
          {draft && (
            <form
              id="price-form"
              className="grid gap-4 sm:grid-cols-2"
              onSubmit={(e) => {
                e.preventDefault();
                if (draft.price === null) return;
                save.mutate({
                  input: {
                    priceListId: list.id,
                    remove: [],
                    upsert: [
                      {
                        id: draft.id,
                        variantId: draft.variantId,
                        unitId: draft.unitId,
                        minQty: draft.minQty ?? 0,
                        price: draft.price,
                        validFrom: draft.validFrom || null,
                        validTo: draft.validTo || null,
                      },
                    ],
                  },
                });
              }}
            >
              <Field>
                <FieldLabel htmlFor="price-value">Price</FieldLabel>
                <MoneyInput
                  id="price-value"
                  autoFocus
                  value={draft.price}
                  onValueChange={(price) => setDraft({ ...draft, price })}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="price-min">From quantity</FieldLabel>
                <QtyInput
                  id="price-min"
                  value={draft.minQty}
                  onValueChange={(minQty) => setDraft({ ...draft, minQty })}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="price-from">Valid from</FieldLabel>
                <DatePicker
                  id="price-from"
                  clearable
                  placeholder="Now"
                  value={draft.validFrom}
                  onChange={(validFrom) => setDraft({ ...draft, validFrom })}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="price-to">Valid until</FieldLabel>
                <DatePicker
                  id="price-to"
                  clearable
                  placeholder="No end"
                  value={draft.validTo}
                  onChange={(validTo) => setDraft({ ...draft, validTo })}
                />
                <FieldError>{fieldError(save.error, "upsert.0.validTo")}</FieldError>
              </Field>
            </form>
          )}
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDraft(null)}>
              Cancel
            </Button>
            <Button
              type="submit"
              form="price-form"
              disabled={save.isPending || draft?.price === null}
            >
              Save price
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Section>
  );
}

function TillsSection({ lists, canManage }: { lists: Api.PriceListSummary[]; canManage: boolean }) {
  const tills = useCommand("price_lists_tills", {});
  const assign = useAction("price_lists_set_till", {
    invalidates: ["price_lists_tills", "price_lists_list", "sales_preview"],
    success: "Till's price list changed",
  });
  return (
    <Section title="Tills">
      <QueryView query={tills}>
        {(rows) => (
          <div className="flex flex-col gap-3">
            {rows.map((t) => (
              <Field key={t.registerId}>
                <FieldLabel htmlFor={`till-${t.registerId}`}>{t.registerName}</FieldLabel>
                <SelectField
                  id={`till-${t.registerId}`}
                  value={t.priceListId ?? ""}
                  disabled={!canManage}
                  onValueChange={(v) =>
                    assign.mutate({ input: { registerId: t.registerId, priceListId: v || null } })
                  }
                  options={[
                    { value: "", label: "Default list / product prices" },
                    ...lists.filter((l) => l.isActive).map((l) => ({ value: l.id, label: l.name })),
                  ]}
                />
              </Field>
            ))}
          </div>
        )}
      </QueryView>
    </Section>
  );
}

/** A percentage or amount change to many prices, shown before it is applied. */
function BulkDialog({ priceListId, onClose }: { priceListId: string | null; onClose: () => void }) {
  const money = useMoney();
  const categories = useCommand("categories_list", {});
  const [category, setCategory] = useState("");
  const [mode, setMode] = useState<"percent" | "amount">("percent");
  const [direction, setDirection] = useState<"up" | "down">("up");
  const [rate, setRate] = useState<number | null>(50_000);
  const [amount, setAmount] = useState<number | null>(null);
  const [roundTo, setRoundTo] = useState("100");
  const [preview, setPreview] = useState<Api.BulkUpdateResult | null>(null);
  const magnitude = mode === "percent" ? rate : amount;
  const input = (apply: boolean): Api.BulkUpdateInput => ({
    priceListId,
    categoryId: category || null,
    variantIds: [],
    mode,
    value: (direction === "up" ? 1 : -1) * (magnitude ?? 0),
    roundTo: Number(roundTo),
    apply,
  });
  const run = useAction("price_lists_bulk_update", {
    invalidates: ["price_lists_items", "products_list", "products_get", "sales_preview"],
    onSuccess: (r) => (r.applied ? onClose() : setPreview(r)),
    success: (r) =>
      r.applied ? `${r.changed} prices changed` : `${r.changed} prices would change`,
  });
  const changed = preview?.rows.filter((r) => r.oldPrice !== r.newPrice) ?? [];
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {priceListId ? "Change this list's prices" : "Change products' own prices"}
          </DialogTitle>
          <DialogDescription>
            Look at the preview, then apply. Every change is in the audit log.
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field>
            <FieldLabel htmlFor="bulk-category">Products</FieldLabel>
            <SelectField
              id="bulk-category"
              value={category}
              onValueChange={(v) => {
                setCategory(v);
                setPreview(null);
              }}
              options={[
                { value: "", label: "All products" },
                ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="bulk-direction">Change</FieldLabel>
            <div className="flex gap-2">
              <SelectField
                id="bulk-direction"
                value={direction}
                onValueChange={(v) => {
                  setDirection(v as "up" | "down");
                  setPreview(null);
                }}
                options={[
                  { value: "up", label: "Increase" },
                  { value: "down", label: "Decrease" },
                ]}
              />
              <SelectField
                aria-label="By"
                value={mode}
                onValueChange={(v) => {
                  setMode(v as "percent" | "amount");
                  setPreview(null);
                }}
                options={[
                  { value: "percent", label: "by %" },
                  { value: "amount", label: "by amount" },
                ]}
              />
            </div>
          </Field>
          <Field>
            <FieldLabel htmlFor="bulk-value">
              {mode === "percent" ? "Percent" : "Amount"}
            </FieldLabel>
            {mode === "percent" ? (
              <RateInput
                id="bulk-value"
                value={rate}
                onValueChange={(v) => {
                  setRate(v);
                  setPreview(null);
                }}
              />
            ) : (
              <MoneyInput
                id="bulk-value"
                value={amount}
                onValueChange={(v) => {
                  setAmount(v);
                  setPreview(null);
                }}
              />
            )}
          </Field>
          <Field>
            <FieldLabel htmlFor="bulk-round">Round new prices to</FieldLabel>
            <SelectField
              id="bulk-round"
              value={roundTo}
              onValueChange={(v) => {
                setRoundTo(v);
                setPreview(null);
              }}
              options={[
                { value: "0", label: "No rounding" },
                { value: "100", label: "₹1" },
                { value: "500", label: "₹5" },
                { value: "1000", label: "₹10" },
              ]}
            />
          </Field>
        </div>
        {preview && (
          <div className="max-h-64 overflow-y-auto rounded-lg border border-border">
            <table className="w-full text-sm">
              <thead className="sticky top-0 bg-card text-left text-xs text-muted-foreground">
                <tr>
                  <th className="px-2 py-1.5 font-medium">Item</th>
                  <th className="text-right font-medium">Now</th>
                  <th className="pr-2 text-right font-medium">New</th>
                </tr>
              </thead>
              <tbody>
                {changed.slice(0, 300).map((r, i) => (
                  <tr key={`${r.variantId}-${i}`} className="border-t border-border/50">
                    <td className="px-2 py-1">
                      {r.name} <span className="text-xs text-muted-foreground">{r.unitCode}</span>
                    </td>
                    <td className="text-right text-muted-foreground tabular-nums">
                      {money(r.oldPrice)}
                    </td>
                    <td className="pr-2 text-right font-medium tabular-nums">
                      {money(r.newPrice)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {changed.length === 0 && (
              <p className="p-3 text-sm text-muted-foreground">Nothing would change.</p>
            )}
          </div>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="outline"
            disabled={magnitude === null || run.isPending}
            onClick={() => run.mutate({ input: input(false) })}
          >
            Preview
          </Button>
          <Button
            disabled={!preview || changed.length === 0 || run.isPending}
            onClick={() => run.mutate({ input: input(true) })}
          >
            Apply {changed.length > 0 ? `${changed.length} changes` : ""}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
