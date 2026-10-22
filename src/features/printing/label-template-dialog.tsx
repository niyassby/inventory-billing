import { useState } from "react";
import type { Api } from "@inventory/shared";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
import { fieldError, useAction } from "@/lib/queries";

export function newTemplate(): Api.LabelTemplate {
  return {
    id: null,
    name: "",
    widthMm: 50,
    heightMm: 25,
    gapMm: 2,
    columns: 1,
    symbology: "ean13",
    showBusiness: false,
    showName: true,
    showPrice: true,
    showMrp: true,
    showSku: false,
    showBarcode: true,
    showBatch: false,
    showExpiry: false,
  };
}

const FIELDS: [keyof Api.LabelTemplate, string][] = [
  ["showBusiness", "Shop name"],
  ["showName", "Item name"],
  ["showPrice", "Price"],
  ["showMrp", "MRP"],
  ["showSku", "SKU"],
  ["showBarcode", "Barcode"],
  ["showBatch", "Batch"],
  ["showExpiry", "Expiry"],
];

function MmInput({
  id,
  value,
  onChange,
}: {
  id: string;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <Input
      id={id}
      inputMode="numeric"
      value={String(value)}
      onChange={(e) => onChange(Number(e.target.value.replace(/\D/g, "")) || 0)}
    />
  );
}

/** Size, layout and fields of a label template. */
export function LabelTemplateDialog({
  template,
  onClose,
  onSaved,
}: {
  template: Api.LabelTemplate | null;
  onClose: () => void;
  onSaved: (saved: Api.LabelTemplate) => void;
}) {
  const [draft, setDraft] = useState(template);
  const [shown, setShown] = useState(template);
  if (template !== shown) {
    setShown(template);
    // Keep the last contents while the dialog animates closed.
    if (template) setDraft(template);
  }
  const save = useAction("labels_templates_save", {
    invalidates: ["labels_templates_list"],
    success: "Template saved",
    onSuccess: (saved) => onSaved(saved),
  });
  const set = (patch: Partial<Api.LabelTemplate>) => draft && setDraft({ ...draft, ...patch });
  const err = (f: string) => fieldError(save.error, f);
  return (
    <Dialog open={template !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{template?.id ? "Edit label template" : "New label template"}</DialogTitle>
          <DialogDescription>
            Sizes are in millimetres, as printed on the label roll.
          </DialogDescription>
        </DialogHeader>
        {draft && (
          <form
            id="label-template"
            className="grid gap-4 sm:grid-cols-2"
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate({ input: draft });
            }}
          >
            <Field className="sm:col-span-2">
              <FieldLabel htmlFor="tpl-name">Name</FieldLabel>
              <Input
                id="tpl-name"
                value={draft.name}
                maxLength={60}
                autoFocus
                onChange={(e) => set({ name: e.target.value })}
              />
              <FieldError>{err("name")}</FieldError>
            </Field>
            <div className="grid grid-cols-3 gap-2 sm:col-span-2">
              <Field>
                <FieldLabel htmlFor="tpl-w">Width</FieldLabel>
                <MmInput id="tpl-w" value={draft.widthMm} onChange={(v) => set({ widthMm: v })} />
                <FieldError>{err("widthMm")}</FieldError>
              </Field>
              <Field>
                <FieldLabel htmlFor="tpl-h">Height</FieldLabel>
                <MmInput id="tpl-h" value={draft.heightMm} onChange={(v) => set({ heightMm: v })} />
                <FieldError>{err("heightMm")}</FieldError>
              </Field>
              <Field>
                <FieldLabel htmlFor="tpl-gap">Gap</FieldLabel>
                <MmInput
                  id="tpl-gap"
                  value={draft.gapMm ?? 2}
                  onChange={(v) => set({ gapMm: v })}
                />
              </Field>
            </div>
            <Field>
              <FieldLabel htmlFor="tpl-cols">Labels across the roll</FieldLabel>
              <SelectField
                id="tpl-cols"
                value={String(draft.columns ?? 1)}
                onValueChange={(v) => set({ columns: Number(v) })}
                options={["1", "2", "3", "4"].map((n) => ({ value: n, label: n }))}
              />
              <FieldError>{err("columns")}</FieldError>
            </Field>
            <Field>
              <FieldLabel htmlFor="tpl-code">Barcode type</FieldLabel>
              <SelectField
                id="tpl-code"
                value={draft.symbology ?? "code128"}
                onValueChange={(v) => set({ symbology: v as Api.Symbology })}
                options={[
                  { value: "ean13", label: "EAN-13" },
                  { value: "ean8", label: "EAN-8" },
                  { value: "code128", label: "Code 128" },
                  { value: "qr", label: "QR code" },
                ]}
              />
            </Field>
            <fieldset className="sm:col-span-2">
              <legend className="mb-2 text-sm font-medium">On the label</legend>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                {FIELDS.map(([key, text]) => (
                  <label key={key} className="flex items-center gap-2 text-sm">
                    <Checkbox
                      checked={draft[key] === true}
                      onCheckedChange={(v) => set({ [key]: v })}
                    />
                    {text}
                  </label>
                ))}
              </div>
            </fieldset>
          </form>
        )}
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form="label-template" disabled={save.isPending}>
            {save.isPending ? "Saving…" : "Save template"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
