import { SelectField } from "@/components/ui/select-field";
import { useEffect, useMemo } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { Controller, useFieldArray, useForm } from "react-hook-form";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { History, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { confirm } from "@/components/app/dialogs";
import { MoneyInput, QtyInput } from "@/components/app/inputs";
import { ErrorState, LoadingRows, Page, Section } from "@/components/app/page";
import { SaveBar } from "@/components/app/save-bar";
import { ProductPicture } from "./product-picture";
import { call } from "@/api/v2";
import { showQty, showRate } from "@/lib/format";
import { errorText, useCommand } from "@/lib/queries";
import { useCan } from "@/stores/session";

interface VariantRow {
  /** The saved variant (RHF uses `id` for its own row keys). */
  variantId: string | null;
  variantName: string;
  sku: string;
  barcode: string;
  salePrice: number | null;
  mrp: number | null;
  costPrice: number | null;
  reorderLevel: number | null;
  openingStock: number | null;
  isActive: boolean;
}

interface PackRow {
  fromUnitId: string;
  /** Base units in one pack, thousandths. */
  baseQty: number | null;
  barcode: string;
}

interface ProductForm {
  name: string;
  description: string;
  categoryId: string;
  brandId: string;
  baseUnitId: string;
  taxRateId: string;
  hsnSac: string;
  isService: boolean;
  trackBatches: boolean;
  isActive: boolean;
  variants: VariantRow[];
  packs: PackRow[];
}

const emptyVariant = (): VariantRow => ({
  variantId: null,
  variantName: "",
  sku: "",
  barcode: "",
  salePrice: null,
  mrp: null,
  costPrice: null,
  reorderLevel: 0,
  openingStock: null,
  isActive: true,
});

function gcd(a: number, b: number): number {
  return b === 0 ? a : gcd(b, a % b);
}

/** 12.5 base units (12500 thousandths) → 25/2. */
export function toFactor(baseQtyMilli: number): { factorNum: number; factorDen: number } {
  const g = gcd(baseQtyMilli, 1000);
  return { factorNum: baseQtyMilli / g, factorDen: 1000 / g };
}

function fromDetail(p: Api.ProductDetail): ProductForm {
  return {
    name: p.name,
    description: p.description ?? "",
    categoryId: p.categoryId ?? "",
    brandId: p.brandId ?? "",
    baseUnitId: p.baseUnitId,
    taxRateId: p.taxRateId ?? "",
    hsnSac: p.hsnSac ?? "",
    isService: p.isService,
    trackBatches: p.trackBatches,
    isActive: p.isActive,
    variants: p.variants.map((v) => ({
      variantId: v.id,
      variantName: v.variantName ?? "",
      sku: v.sku,
      barcode: v.barcode ?? "",
      salePrice: v.salePrice,
      mrp: v.mrp ?? null,
      costPrice: v.costPrice,
      reorderLevel: v.reorderLevel,
      openingStock: null,
      isActive: v.isActive,
    })),
    packs: p.unitConversions.map((c) => ({
      fromUnitId: c.fromUnitId,
      baseQty: Math.round((c.factorNum * 1000) / c.factorDen),
      barcode: c.barcode ?? "",
    })),
  };
}

function Editor({ product }: { product: Api.ProductDetail | null }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const canManage = useCan("products.manage");
  const categories = useCommand("categories_list", {});
  const brands = useCommand("brands_list", {});
  const units = useCommand("units_list", {});
  const taxRates = useCommand("tax_rates_list", {});
  const isNew = product === null;

  const defaults = useMemo<ProductForm>(
    () =>
      product
        ? fromDetail(product)
        : {
            name: "",
            description: "",
            categoryId: "",
            brandId: "",
            baseUnitId: "",
            taxRateId: "",
            hsnSac: "",
            isService: false,
            trackBatches: false,
            isActive: true,
            variants: [emptyVariant()],
            packs: [],
          },
    [product],
  );
  const form = useForm<ProductForm>({ defaultValues: defaults });
  const { register, control, handleSubmit, formState, setError, reset, watch } = form;
  const variants = useFieldArray({ control, name: "variants" });
  const packs = useFieldArray({ control, name: "packs" });
  useEffect(() => reset(defaults), [defaults, reset]);
  useEffect(() => {
    const pcs = units.data?.find((u) => u.code === "pcs") ?? units.data?.[0];
    if (isNew && pcs && !form.getValues("baseUnitId")) form.setValue("baseUnitId", pcs.id);
  }, [units.data, isNew, form]);

  const baseUnit = units.data?.find((u) => u.id === watch("baseUnitId"));

  const save = handleSubmit(async (v) => {
    if (!v.baseUnitId) {
      setError("baseUnitId", { message: "Choose a unit" });
      return;
    }
    const missingPrice = v.variants.findIndex((x) => x.salePrice === null);
    if (missingPrice >= 0) {
      setError(`variants.${missingPrice}.salePrice`, { message: "Enter a price" });
      return;
    }
    const input: Api.ProductInput = {
      id: product?.id ?? null,
      expectedUpdatedAt: product?.updatedAt ?? null,
      name: v.name,
      description: v.description || null,
      categoryId: v.categoryId || null,
      brandId: v.brandId || null,
      baseUnitId: v.baseUnitId,
      taxRateId: v.taxRateId || null,
      hsnSac: v.hsnSac || null,
      isService: v.isService,
      trackBatches: v.trackBatches,
      trackExpiry: v.trackBatches,
      isActive: v.isActive,
      variants: v.variants.map((x) => ({
        id: x.variantId,
        variantName: x.variantName || null,
        sku: x.sku,
        barcode: x.barcode || null,
        salePrice: x.salePrice ?? 0,
        mrp: x.mrp,
        costPrice: x.costPrice,
        reorderLevel: x.reorderLevel ?? 0,
        reorderQty: 0,
        isActive: x.isActive,
        openingStock: x.variantId ? null : x.openingStock,
      })),
      unitConversions: v.packs
        .filter((p) => p.fromUnitId && p.baseQty)
        .map((p) => ({
          fromUnitId: p.fromUnitId,
          barcode: p.barcode || null,
          ...toFactor(p.baseQty ?? 1000),
        })),
    };
    try {
      const saved = await call("products_save", input);
      toast.success(isNew ? "Product added" : "Product saved");
      void queryClient.invalidateQueries({ queryKey: ["products_list"] });
      void queryClient.invalidateQueries({ queryKey: ["products_get"] });
      void queryClient.invalidateQueries({ queryKey: ["products_search"] });
      if (isNew) void navigate(`/inventory/products/${saved.id}`, { replace: true });
    } catch (e) {
      toast.error(errorText(e));
    }
  });

  const remove = async () => {
    if (!product) return;
    const ok = await confirm({
      title: `Delete ${product.name}?`,
      description:
        "Only products without stock can be deleted. Past bills keep their copy of the item.",
      confirmLabel: "Delete",
      destructive: true,
    });
    if (ok === false) return;
    try {
      await call("products_delete", { id: product.id });
      void queryClient.invalidateQueries({ queryKey: ["products_list"] });
      toast.success("Product deleted");
      void navigate("/inventory/products");
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  const err = (name: string) =>
    (formState.errors as Record<string, { message?: string } | undefined>)[name]?.message;

  return (
    <Page
      title={product ? product.name : "New product"}
      back={{ to: "/inventory/products", label: "Products" }}
      actions={
        canManage && (
          <>
            {!isNew && (
              <Button variant="outline" onClick={() => void remove()}>
                <Trash2 /> Delete
              </Button>
            )}
            <Button disabled={formState.isSubmitting} onClick={() => void save()}>
              {formState.isSubmitting ? "Saving…" : "Save (Ctrl+S)"}
            </Button>
          </>
        )
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => void save(e)}
        onKeyDown={(e) => {
          if (e.ctrlKey && e.key.toLowerCase() === "s") {
            e.preventDefault();
            void save();
          }
        }}
      >
        <fieldset disabled={!canManage} className="contents">
          <Section title="Details">
            <div className="grid gap-4 md:grid-cols-3">
              <Field className="md:col-span-2" data-invalid={Boolean(err("name")) || undefined}>
                <FieldLabel htmlFor="name" required>
                  Name
                </FieldLabel>
                <Input
                  id="name"
                  autoFocus={isNew}
                  {...register("name", { required: "Enter a name" })}
                />
                <FieldError>{err("name")}</FieldError>
              </Field>
              <Field>
                <FieldLabel htmlFor="hsnSac">HSN / SAC code</FieldLabel>
                <Input id="hsnSac" {...register("hsnSac")} />
              </Field>
              <Field>
                <FieldLabel htmlFor="categoryId">Category</FieldLabel>
                <Controller
                  control={control}
                  name="categoryId"
                  render={({ field }) => (
                    <SelectField
                      id="categoryId"
                      value={field.value}
                      onValueChange={field.onChange}
                      options={[
                        { value: "", label: "None" },
                        ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
                      ]}
                    />
                  )}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="brandId">Brand</FieldLabel>
                <Controller
                  control={control}
                  name="brandId"
                  render={({ field }) => (
                    <SelectField
                      id="brandId"
                      value={field.value}
                      onValueChange={field.onChange}
                      options={[
                        { value: "", label: "None" },
                        ...(brands.data ?? []).map((b) => ({ value: b.id, label: b.name })),
                      ]}
                    />
                  )}
                />
              </Field>
              <Field data-invalid={Boolean(err("baseUnitId")) || undefined}>
                <FieldLabel htmlFor="baseUnitId" required>
                  Unit
                </FieldLabel>
                <Controller
                  control={control}
                  name="baseUnitId"
                  render={({ field }) => (
                    <SelectField
                      id="baseUnitId"
                      required
                      invalid={Boolean(err("baseUnitId"))}
                      value={field.value}
                      onValueChange={field.onChange}
                      options={[
                        { value: "", label: "Choose…" },
                        ...(units.data ?? []).map((u) => ({
                          value: u.id,
                          label: `${u.name} (${u.code})`,
                        })),
                      ]}
                    />
                  )}
                />
                <FieldError>{err("baseUnitId")}</FieldError>
              </Field>
              <Field>
                <FieldLabel htmlFor="taxRateId">Tax</FieldLabel>
                <Controller
                  control={control}
                  name="taxRateId"
                  render={({ field }) => (
                    <SelectField
                      id="taxRateId"
                      value={field.value}
                      onValueChange={field.onChange}
                      options={[
                        { value: "", label: "No tax" },
                        ...(taxRates.data ?? [])
                          .filter((t) => t.isActive)
                          .map((t) => ({ value: t.id, label: `${t.name} (${showRate(t.rate)})` })),
                      ]}
                    />
                  )}
                />
              </Field>
              <Field className="md:col-span-2">
                <FieldLabel htmlFor="description">Description</FieldLabel>
                <Textarea id="description" className="min-h-10" {...register("description")} />
              </Field>
            </div>
            <div className="mt-4 flex flex-wrap gap-6 text-sm">
              {(
                [
                  ["isService", "Service (no stock)"],
                  ["trackBatches", "Track batches and expiry"],
                  ["isActive", "Active"],
                ] as const
              ).map(([name, text]) => (
                <Controller
                  key={name}
                  control={control}
                  name={name}
                  render={({ field }) => (
                    <label className="flex items-center gap-2">
                      <Checkbox checked={field.value} onCheckedChange={field.onChange} />
                      {text}
                    </label>
                  )}
                />
              ))}
            </div>
          </Section>
          <ProductPicture product={product} canManage={canManage} />

          <Section
            title="Variants, prices and stock"
            actions={
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => variants.append(emptyVariant())}
              >
                <Plus /> Add variant
              </Button>
            }
          >
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="px-1 py-1 text-left font-medium">Variant</th>
                    <th className="px-1 py-1 text-left font-medium">
                      SKU{" "}
                      <span aria-hidden className="text-danger">
                        *
                      </span>
                    </th>
                    <th className="px-1 py-1 text-left font-medium">Barcode</th>
                    <th className="px-1 py-1 text-right font-medium">
                      Sale price{" "}
                      <span aria-hidden className="text-danger">
                        *
                      </span>
                    </th>
                    <th className="px-1 py-1 text-right font-medium">MRP</th>
                    <th className="px-1 py-1 text-right font-medium">Cost</th>
                    <th className="px-1 py-1 text-right font-medium">Reorder at</th>
                    <th className="px-1 py-1 text-right font-medium">
                      {isNew ? "Opening stock" : "In stock"}
                    </th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {variants.fields.map((f, i) => {
                    const existing = product?.variants.find((v) => v.id === f.variantId);
                    const priceError = formState.errors.variants?.[i]?.salePrice?.message;
                    return (
                      <tr key={f.id} className="align-top">
                        <td className="min-w-36 p-1">
                          <Input
                            aria-label="Variant name"
                            placeholder="e.g. 1 kg, Red"
                            {...register(`variants.${i}.variantName`)}
                          />
                        </td>
                        <td className="min-w-32 p-1">
                          <Input
                            aria-label="SKU"
                            aria-invalid={Boolean(formState.errors.variants?.[i]?.sku) || undefined}
                            {...register(`variants.${i}.sku`, { required: true })}
                          />
                        </td>
                        <td className="min-w-36 p-1">
                          <Input aria-label="Barcode" {...register(`variants.${i}.barcode`)} />
                        </td>
                        {(["salePrice", "mrp", "costPrice"] as const).map((name) => (
                          <td key={name} className="min-w-28 p-1">
                            <Controller
                              control={control}
                              name={`variants.${i}.${name}`}
                              render={({ field }) => (
                                <MoneyInput
                                  aria-label={
                                    { salePrice: "Sale price", mrp: "MRP", costPrice: "Cost" }[name]
                                  }
                                  aria-invalid={
                                    (name === "salePrice" && Boolean(priceError)) || undefined
                                  }
                                  value={field.value}
                                  onValueChange={field.onChange}
                                />
                              )}
                            />
                          </td>
                        ))}
                        <td className="min-w-24 p-1">
                          <Controller
                            control={control}
                            name={`variants.${i}.reorderLevel`}
                            render={({ field }) => (
                              <QtyInput
                                aria-label="Reorder level"
                                value={field.value}
                                onValueChange={field.onChange}
                              />
                            )}
                          />
                        </td>
                        <td className="min-w-28 p-1 text-right">
                          {existing ? (
                            <Link
                              to={`/inventory/stock-card/${existing.id}`}
                              className="inline-flex items-center gap-1 pt-2 tabular-nums underline-offset-2 hover:underline"
                              title={existing.stockByWarehouse
                                .map((w) => `${w.warehouseName}: ${showQty(w.qty)}`)
                                .join("\n")}
                            >
                              <History className="size-3.5" aria-hidden />
                              {showQty(existing.stock)} {baseUnit?.code}
                            </Link>
                          ) : (
                            <Controller
                              control={control}
                              name={`variants.${i}.openingStock`}
                              render={({ field }) => (
                                <QtyInput
                                  aria-label="Opening stock"
                                  value={field.value}
                                  onValueChange={field.onChange}
                                />
                              )}
                            />
                          )}
                        </td>
                        <td className="p-1">
                          {variants.fields.length > 1 && !existing && (
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon-sm"
                              aria-label="Remove variant"
                              onClick={() => variants.remove(i)}
                            >
                              <Trash2 />
                            </Button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              Prices are per {baseUnit?.name ?? "unit"}. Changing a sale price asks for a manager's
              approval if your role can't change prices.
            </p>
          </Section>

          <Section
            title="Pack sizes"
            actions={
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => packs.append({ fromUnitId: "", baseQty: null, barcode: "" })}
              >
                <Plus /> Add pack size
              </Button>
            }
          >
            {packs.fields.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                Sell or buy in boxes or cartons? Add a pack size, for example 1 box = 12{" "}
                {baseUnit?.code ?? "pcs"}.
              </p>
            ) : (
              <div className="flex flex-col gap-2">
                {packs.fields.map((f, i) => (
                  <div key={f.id} className="flex flex-wrap items-center gap-2 text-sm">
                    <span>1</span>
                    <Controller
                      control={control}
                      name={`packs.${i}.fromUnitId`}
                      render={({ field }) => (
                        <SelectField
                          aria-label="Pack unit"
                          className="w-40"
                          value={field.value}
                          onValueChange={field.onChange}
                          options={[
                            { value: "", label: "unit…" },
                            ...(units.data ?? [])
                              .filter((u) => u.id !== watch("baseUnitId"))
                              .map((u) => ({ value: u.id, label: u.name })),
                          ]}
                        />
                      )}
                    />
                    <span>=</span>
                    <Controller
                      control={control}
                      name={`packs.${i}.baseQty`}
                      render={({ field }) => (
                        <QtyInput
                          aria-label="Base units in the pack"
                          className="w-24"
                          value={field.value}
                          onValueChange={field.onChange}
                        />
                      )}
                    />
                    <span>{baseUnit?.code}</span>
                    <Input
                      aria-label="Pack barcode"
                      placeholder="Pack barcode"
                      className="w-44"
                      {...register(`packs.${i}.barcode`)}
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      aria-label="Remove pack size"
                      onClick={() => packs.remove(i)}
                    >
                      <Trash2 />
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </Section>
        </fieldset>
        {canManage && (
          <SaveBar
            dirty={formState.isDirty}
            saving={formState.isSubmitting}
            onDiscard={() => reset(defaults)}
            onSave={() => void save()}
          />
        )}
      </form>
    </Page>
  );
}

export default function ProductEditor() {
  const { id } = useParams();
  const isNew = !id || id === "new";
  const product = useCommand("products_get", { id: id ?? "" }, { enabled: !isNew });
  if (isNew) return <Editor product={null} />;
  if (product.error)
    return (
      <Page title="Product">
        <ErrorState error={product.error} onRetry={() => void product.refetch()} />
      </Page>
    );
  if (!product.data)
    return (
      <Page title="Product">
        <LoadingRows />
      </Page>
    );
  return <Editor product={product.data} />;
}
