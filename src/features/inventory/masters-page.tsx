import { useState } from "react";
import type { ReactNode } from "react";
import { Pencil, Plus, Trash2 } from "lucide-react";
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
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { confirm } from "@/components/app/dialogs";
import { RateInput } from "@/components/app/inputs";
import { Page, QueryView } from "@/components/app/page";
import { showRate } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { useCan } from "@/stores/session";

interface Row {
  id: string;
  title: ReactNode;
  detail?: ReactNode;
}

function MasterList({
  rows,
  onAdd,
  onEdit,
  onDelete,
  addLabel,
  canManage,
}: {
  rows: Row[];
  onAdd: () => void;
  onEdit: (id: string) => void;
  onDelete?: (id: string) => void;
  addLabel: string;
  canManage: boolean;
}) {
  return (
    <div className="flex flex-col gap-3">
      {canManage && (
        <Button variant="outline" size="sm" className="w-fit" onClick={onAdd}>
          <Plus /> {addLabel}
        </Button>
      )}
      <ul className="divide-y divide-border/50 rounded-2xl border border-border/60 bg-card">
        {rows.length === 0 && (
          <li className="p-4 text-sm text-muted-foreground">Nothing here yet.</li>
        )}
        {rows.map((r) => (
          <li key={r.id} className="flex items-center gap-3 px-4 py-2 text-sm">
            <span className="flex-1 font-medium">{r.title}</span>
            {r.detail && <span className="text-muted-foreground">{r.detail}</span>}
            {canManage && (
              <>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Edit"
                  onClick={() => onEdit(r.id)}
                >
                  <Pencil />
                </Button>
                {onDelete && (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Delete"
                    onClick={() => onDelete(r.id)}
                  >
                    <Trash2 />
                  </Button>
                )}
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

type Editing =
  | { kind: "categories" | "brands"; id: string | null; name: string }
  | { kind: "units"; id: string | null; code: string; name: string; decimals: number }
  | {
      kind: "tax";
      id: string | null;
      name: string;
      taxType: string;
      rate: number | null;
      cessRate: number | null;
      isActive: boolean;
    }
  | { kind: "stores"; id: string | null; code: string; name: string };

export default function MastersPage() {
  const canManage = useCan("products.manage");
  const canStores = useCan("settings.manage");
  const categories = useCommand("categories_list", {});
  const brands = useCommand("brands_list", {});
  const units = useCommand("units_list", {});
  const taxRates = useCommand("tax_rates_list", {});
  const stores = useCommand("stock_warehouses", {});
  const [editing, setEditing] = useState<Editing | null>(null);
  const done = () => setEditing(null);
  const saveCategory = useAction("categories_save", {
    invalidates: ["categories_list"],
    success: "Saved",
    onSuccess: done,
  });
  const saveBrand = useAction("brands_save", {
    invalidates: ["brands_list"],
    success: "Saved",
    onSuccess: done,
  });
  const saveUnit = useAction("units_save", {
    invalidates: ["units_list"],
    success: "Saved",
    onSuccess: done,
  });
  const saveTax = useAction("tax_rates_save", {
    invalidates: ["tax_rates_list"],
    success: "Saved",
    onSuccess: done,
  });
  const saveStore = useAction("warehouses_save", {
    invalidates: ["stock_warehouses"],
    success: "Saved",
    onSuccess: done,
  });
  const deleteMaster = useAction("catalog_delete_master", {
    invalidates: ["categories_list", "brands_list", "units_list"],
    success: "Deleted",
  });

  const remove = async (kind: "categories" | "brands" | "units", id: string) => {
    const ok = await confirm({
      title: "Delete this entry?",
      description: "Entries still used by products can't be deleted.",
      confirmLabel: "Delete",
      destructive: true,
    });
    if (ok !== false) deleteMaster.mutate({ input: { kind, id } });
  };

  const submit = () => {
    if (!editing) return;
    switch (editing.kind) {
      case "categories":
        saveCategory.mutate({ input: { id: editing.id, name: editing.name } });
        break;
      case "brands":
        saveBrand.mutate({ input: { id: editing.id, name: editing.name } });
        break;
      case "units":
        saveUnit.mutate({
          input: {
            id: editing.id,
            code: editing.code,
            name: editing.name,
            decimals: editing.decimals,
          },
        });
        break;
      case "tax":
        saveTax.mutate({
          input: {
            id: editing.id,
            name: editing.name,
            taxType: editing.taxType,
            rate: editing.rate ?? 0,
            cessRate: editing.cessRate ?? 0,
            isActive: editing.isActive,
          },
        });
        break;
      case "stores":
        saveStore.mutate({ input: { id: editing.id, code: editing.code, name: editing.name } });
        break;
    }
  };

  return (
    <Page title="Categories & units" description="The lists products are organised and taxed by.">
      <Tabs defaultValue="categories">
        <TabsList>
          <TabsTrigger value="categories">Categories</TabsTrigger>
          <TabsTrigger value="brands">Brands</TabsTrigger>
          <TabsTrigger value="units">Units</TabsTrigger>
          <TabsTrigger value="tax">Tax rates</TabsTrigger>
          <TabsTrigger value="stores">Stores</TabsTrigger>
        </TabsList>
        <TabsContent value="categories">
          <QueryView query={categories}>
            {(list) => (
              <MasterList
                canManage={canManage}
                addLabel="Add category"
                rows={list.map((c) => ({
                  id: c.id,
                  title: c.name,
                  detail: `${c.productCount} products`,
                }))}
                onAdd={() => setEditing({ kind: "categories", id: null, name: "" })}
                onEdit={(id) =>
                  setEditing({
                    kind: "categories",
                    id,
                    name: list.find((c) => c.id === id)?.name ?? "",
                  })
                }
                onDelete={(id) => void remove("categories", id)}
              />
            )}
          </QueryView>
        </TabsContent>
        <TabsContent value="brands">
          <QueryView query={brands}>
            {(list) => (
              <MasterList
                canManage={canManage}
                addLabel="Add brand"
                rows={list.map((b) => ({
                  id: b.id,
                  title: b.name,
                  detail: `${b.productCount} products`,
                }))}
                onAdd={() => setEditing({ kind: "brands", id: null, name: "" })}
                onEdit={(id) =>
                  setEditing({
                    kind: "brands",
                    id,
                    name: list.find((b) => b.id === id)?.name ?? "",
                  })
                }
                onDelete={(id) => void remove("brands", id)}
              />
            )}
          </QueryView>
        </TabsContent>
        <TabsContent value="units">
          <QueryView query={units}>
            {(list) => (
              <MasterList
                canManage={canManage}
                addLabel="Add unit"
                rows={list.map((u) => ({
                  id: u.id,
                  title: `${u.name} (${u.code})`,
                  detail: u.decimals > 0 ? `${u.decimals} decimals` : "whole numbers",
                }))}
                onAdd={() =>
                  setEditing({ kind: "units", id: null, code: "", name: "", decimals: 0 })
                }
                onEdit={(id) => {
                  const u = list.find((x) => x.id === id);
                  if (u)
                    setEditing({
                      kind: "units",
                      id,
                      code: u.code,
                      name: u.name,
                      decimals: u.decimals,
                    });
                }}
                onDelete={(id) => void remove("units", id)}
              />
            )}
          </QueryView>
        </TabsContent>
        <TabsContent value="tax">
          <QueryView query={taxRates}>
            {(list) => (
              <MasterList
                canManage={canManage}
                addLabel="Add tax rate"
                rows={list.map((t) => ({
                  id: t.id,
                  title: `${t.name}${t.isActive ? "" : " (inactive)"}`,
                  detail: `${t.taxType.toUpperCase()} ${showRate(t.rate)}${t.cessRate ? ` + cess ${showRate(t.cessRate)}` : ""}`,
                }))}
                onAdd={() =>
                  setEditing({
                    kind: "tax",
                    id: null,
                    name: "",
                    taxType: "gst",
                    rate: null,
                    cessRate: 0,
                    isActive: true,
                  })
                }
                onEdit={(id) => {
                  const t = list.find((x) => x.id === id);
                  if (t)
                    setEditing({
                      kind: "tax",
                      id,
                      name: t.name,
                      taxType: t.taxType,
                      rate: t.rate,
                      cessRate: t.cessRate,
                      isActive: t.isActive,
                    });
                }}
              />
            )}
          </QueryView>
        </TabsContent>
        <TabsContent value="stores">
          <QueryView query={stores}>
            {(list) => (
              <MasterList
                canManage={canStores}
                addLabel="Add store"
                rows={list.map((w) => ({
                  id: w.id,
                  title: `${w.name} (${w.code})`,
                  detail: w.isDefault ? "Main store" : undefined,
                }))}
                onAdd={() => setEditing({ kind: "stores", id: null, code: "", name: "" })}
                onEdit={(id) => {
                  const w = list.find((x) => x.id === id);
                  if (w) setEditing({ kind: "stores", id, code: w.code, name: w.name });
                }}
              />
            )}
          </QueryView>
        </TabsContent>
      </Tabs>

      <Dialog open={editing !== null} onOpenChange={(o) => !o && done()}>
        <DialogContent>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            <DialogHeader>
              <DialogTitle>{editing?.id ? "Edit" : "Add"}</DialogTitle>
            </DialogHeader>
            {editing && "code" in editing && (
              <Field>
                <FieldLabel htmlFor="m-code">Code</FieldLabel>
                <Input
                  id="m-code"
                  required
                  value={editing.code}
                  onChange={(e) => setEditing({ ...editing, code: e.target.value })}
                />
              </Field>
            )}
            {editing && (
              <Field>
                <FieldLabel htmlFor="m-name">Name</FieldLabel>
                <Input
                  id="m-name"
                  autoFocus
                  required
                  value={editing.name}
                  onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                />
              </Field>
            )}
            {editing?.kind === "units" && (
              <Field>
                <FieldLabel htmlFor="m-dec">Decimals allowed</FieldLabel>
                <NativeSelect
                  id="m-dec"
                  value={String(editing.decimals)}
                  onChange={(e) => setEditing({ ...editing, decimals: Number(e.target.value) })}
                >
                  <NativeSelectOption value="0">0 (pieces)</NativeSelectOption>
                  <NativeSelectOption value="1">1</NativeSelectOption>
                  <NativeSelectOption value="2">2</NativeSelectOption>
                  <NativeSelectOption value="3">3 (kg, litres)</NativeSelectOption>
                </NativeSelect>
              </Field>
            )}
            {editing?.kind === "tax" && (
              <div className="grid grid-cols-3 gap-3">
                <Field>
                  <FieldLabel htmlFor="m-type">Type</FieldLabel>
                  <NativeSelect
                    id="m-type"
                    value={editing.taxType}
                    onChange={(e) => setEditing({ ...editing, taxType: e.target.value })}
                  >
                    <NativeSelectOption value="gst">GST</NativeSelectOption>
                    <NativeSelectOption value="vat">VAT</NativeSelectOption>
                    <NativeSelectOption value="other">Other</NativeSelectOption>
                  </NativeSelect>
                </Field>
                <Field>
                  <FieldLabel htmlFor="m-rate">Rate</FieldLabel>
                  <RateInput
                    id="m-rate"
                    value={editing.rate}
                    onValueChange={(rate) => setEditing({ ...editing, rate })}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="m-cess">Cess</FieldLabel>
                  <RateInput
                    id="m-cess"
                    value={editing.cessRate}
                    onValueChange={(cessRate) => setEditing({ ...editing, cessRate })}
                  />
                </Field>
              </div>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={done}>
                Cancel
              </Button>
              <Button type="submit">Save</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </Page>
  );
}
