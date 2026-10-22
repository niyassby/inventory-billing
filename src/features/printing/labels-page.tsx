import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { Barcode, FileText, Pencil, Plus, Printer, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { EmptyState, Page, QueryView, Section } from "@/components/app/page";
import { ProductPicker } from "@/components/app/pickers";
import { call } from "@/api/v2";
import { useDebounced } from "@/lib/use-debounced";
import { useMoney } from "@/lib/business";
import { showDate } from "@/lib/format";
import { errorText, useCommand } from "@/lib/queries";
import { printHtml } from "@/features/sales/print";
import { useCan } from "@/stores/session";
import { LabelTemplateDialog, newTemplate } from "./label-template-dialog";
import { labelSheetHtml, MAX_LABELS, mergeItems, totalLabels } from "./labels";
import { printDocument } from "./print-service";

/** Barcode labels: pick products or a goods receipt, set how many, preview, print. */
export default function LabelsPage() {
  const [params, setParams] = useSearchParams();
  const money = useMoney();
  const canEditProducts = useCan("products.manage");
  const templates = useCommand("labels_templates_list", {});
  const [templateId, setTemplateId] = useState("");
  const [editing, setEditing] = useState<Api.LabelTemplate | null>(null);
  const [items, setItems] = useState<Api.LabelItem[]>([]);
  const [previews, setPreviews] = useState<Api.LabelPreview[]>([]);
  const [printing, setPrinting] = useState(false);
  const receipts = useCommand("purchases_list_receipts", { page: 1, pageSize: 20 });

  const template = templates.data?.find((t) => t.id === templateId) ?? templates.data?.[0];
  useEffect(() => {
    if (!templateId && templates.data?.[0]?.id) setTemplateId(templates.data[0].id);
  }, [templateId, templates.data]);

  const add = async (input: Api.LabelSourceInput) => {
    try {
      const found = await call("labels_items", input);
      setItems((list) => mergeItems(list, found));
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  // A goods receipt opened from Purchases (?receipt=<id>): its items, one label per unit.
  const receiptId = params.get("receipt");
  useEffect(() => {
    if (!receiptId) return;
    void add({ variantIds: [], receiptId });
    setParams({}, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per receipt in the address
  }, [receiptId]);

  const request = useMemo(
    () => (template && items.length > 0 ? { template, items } : null),
    [template, items],
  );
  const debounced = useDebounced(request, 250);
  useEffect(() => {
    if (!debounced) {
      setPreviews([]);
      return;
    }
    let stale = false;
    call("labels_preview", debounced)
      .then((p) => !stale && setPreviews(p))
      .catch((e: unknown) => !stale && toast.error(errorText(e)));
    return () => {
      stale = true;
    };
  }, [debounced]);

  const count = totalLabels(items);
  const tooMany = count > MAX_LABELS;
  const setCopies = (index: number, copies: number) =>
    setItems((list) => list.map((it, i) => (i === index ? { ...it, copies } : it)));

  const generate = async (index: number) => {
    const item = items[index];
    if (!item?.variantId) return;
    try {
      const [made] = await call("products_generate_barcodes", { variantIds: [item.variantId] });
      if (made) {
        setItems((list) =>
          list.map((it) =>
            it.variantId === made.variantId ? { ...it, barcode: made.barcode } : it,
          ),
        );
        toast.success(`Barcode ${made.barcode} given to ${item.name}`);
      }
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const sheet = () =>
    template
      ? labelSheetHtml(
          template,
          previews.map((p, i) => ({ svg: p.svg, copies: items[i]?.copies ?? 0 })),
        )
      : "";

  const print = async () => {
    if (!template || count === 0 || tooMany) return;
    setPrinting(true);
    await printDocument({
      document: { type: "labels", template, items },
      fallbackHtml: sheet,
    });
    setPrinting(false);
  };

  return (
    <Page
      title="Barcode labels"
      description="Choose products or a goods receipt, set how many labels, check the preview and print."
      actions={
        <>
          <Button
            variant="outline"
            onClick={() => printHtml(sheet())}
            disabled={count === 0 || tooMany || previews.length === 0}
          >
            <FileText /> A4 sheet
          </Button>
          <Button onClick={() => void print()} disabled={count === 0 || tooMany || printing}>
            <Printer /> {printing ? "Printing…" : `Print ${count} label${count === 1 ? "" : "s"}`}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 xl:grid-cols-[1fr_24rem]">
        <div className="flex min-w-0 flex-col gap-4">
          <Section title="What to label">
            <div className="grid gap-3 sm:grid-cols-2">
              <Field>
                <FieldLabel>Add a product</FieldLabel>
                <ProductPicker
                  onSelect={(i) => void add({ variantIds: [i.variantId], receiptId: null })}
                  onNotFound={(t) => toast.error(`No item matches ${t}`)}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="label-receipt">Or everything from a goods receipt</FieldLabel>
                <SelectField
                  id="label-receipt"
                  value=""
                  placeholder="Choose a recent receipt"
                  onValueChange={(id) => id && void add({ variantIds: [], receiptId: id })}
                  options={(receipts.data?.items ?? []).map((r) => ({
                    value: r.id,
                    label: `${r.docNo} · ${r.supplierName} · ${showDate(r.businessDate)}`,
                  }))}
                />
              </Field>
            </div>
          </Section>
          <Section
            title={`Labels (${count})`}
            actions={
              items.length > 0 && (
                <Button size="sm" variant="ghost" onClick={() => setItems([])}>
                  Clear
                </Button>
              )
            }
          >
            {items.length === 0 ? (
              <EmptyState
                title="Nothing to label yet"
                description="Search for a product, scan its barcode, or pick a goods receipt."
              />
            ) : (
              <table className="w-full text-sm">
                <thead className="text-left text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1.5 font-medium">Item</th>
                    <th className="font-medium">Barcode</th>
                    <th className="text-right font-medium">Price</th>
                    <th className="w-24 text-right font-medium">Labels</th>
                    <th className="w-10" />
                  </tr>
                </thead>
                <tbody>
                  {items.map((it, index) => (
                    <tr
                      key={`${it.variantId}-${it.batchNo ?? ""}-${index}`}
                      className="border-t border-border/50"
                    >
                      <td className="py-1.5">
                        <span className="font-medium">{it.name}</span>
                        {(it.batchNo ?? it.expiryDate) && (
                          <span className="block text-xs text-muted-foreground">
                            {it.batchNo && `Batch ${it.batchNo}`}
                            {it.expiryDate && ` · Exp ${showDate(it.expiryDate)}`}
                          </span>
                        )}
                        {previews[index]?.warning ? (
                          <span className="block text-xs text-warning">
                            {previews[index].warning}
                          </span>
                        ) : null}
                      </td>
                      <td>
                        {it.barcode ? (
                          <span className="font-mono text-xs">{it.barcode}</span>
                        ) : canEditProducts && it.variantId ? (
                          <Button size="xs" variant="outline" onClick={() => void generate(index)}>
                            <Barcode /> Make one
                          </Button>
                        ) : (
                          <span className="text-xs text-muted-foreground">SKU {it.sku}</span>
                        )}
                      </td>
                      <td className="text-right tabular-nums">{money(it.price)}</td>
                      <td className="text-right">
                        <Input
                          aria-label={`Labels for ${it.name}`}
                          className="ml-auto h-8 w-20 text-right tabular-nums"
                          inputMode="numeric"
                          value={String(it.copies)}
                          onChange={(e) =>
                            setCopies(
                              index,
                              Math.min(MAX_LABELS, Number(e.target.value.replace(/\D/g, "")) || 0),
                            )
                          }
                        />
                      </td>
                      <td className="text-right">
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          aria-label={`Remove ${it.name}`}
                          onClick={() => setItems((list) => list.filter((_, i) => i !== index))}
                        >
                          <Trash2 />
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {tooMany && (
              <p role="alert" className="mt-2 text-sm text-destructive">
                At most {MAX_LABELS} labels in one go.
              </p>
            )}
          </Section>
        </div>

        <div className="flex flex-col gap-4">
          <Section
            title="Template"
            actions={
              canEditProducts && (
                <span className="flex gap-1">
                  {template && (
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label="Edit template"
                      onClick={() => setEditing(template)}
                    >
                      <Pencil />
                    </Button>
                  )}
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    aria-label="New template"
                    onClick={() => setEditing(newTemplate())}
                  >
                    <Plus />
                  </Button>
                </span>
              )
            }
          >
            <QueryView query={templates}>
              {(list) => (
                <SelectField
                  aria-label="Label template"
                  value={template?.id ?? ""}
                  onValueChange={setTemplateId}
                  options={list.map((t) => ({ value: t.id ?? "", label: t.name }))}
                />
              )}
            </QueryView>
            {template && (
              <p className="mt-2 flex flex-wrap gap-1 text-xs text-muted-foreground">
                <Badge variant="secondary">
                  {template.widthMm} × {template.heightMm} mm
                </Badge>
                <Badge variant="secondary">{(template.symbology ?? "code128").toUpperCase()}</Badge>
                {(template.columns ?? 1) > 1 && (
                  <Badge variant="secondary">{template.columns} across</Badge>
                )}
              </p>
            )}
          </Section>
          <Section title="Preview">
            {previews.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                The labels show here as they will print.
              </p>
            ) : (
              <ul className="flex max-h-[32rem] flex-col items-center gap-3 overflow-y-auto">
                {previews.slice(0, 30).map((p, i) => (
                  <li key={i} className="flex flex-col items-center gap-1">
                    <img
                      alt={`Label for ${items[i]?.name ?? ""}`}
                      src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(p.svg)}`}
                      className="max-w-full rounded-sm border border-border bg-white shadow-xs"
                      style={{ width: `${Math.min(320, (template?.widthMm ?? 50) * 5)}px` }}
                    />
                    <span className="text-xs text-muted-foreground">× {items[i]?.copies ?? 0}</span>
                  </li>
                ))}
              </ul>
            )}
          </Section>
        </div>
      </div>
      <LabelTemplateDialog
        template={editing}
        onClose={() => setEditing(null)}
        onSaved={(saved) => {
          setEditing(null);
          if (saved.id) setTemplateId(saved.id);
        }}
      />
    </Page>
  );
}
