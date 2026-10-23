import { useState } from "react";
import type { Api } from "@inventory/shared";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { QueryView, Section } from "@/components/app/page";
import { fieldError, useAction, useCommand } from "@/lib/queries";

/**
 * Conversions that hold for every product (1 dozen = 12 pieces). A product's own pack sizes are
 * on the product. Stock is always kept and shown in each product's base unit.
 */
export function UnitConversionsSection({
  units,
  canManage,
}: {
  units: Api.Unit[];
  canManage: boolean;
}) {
  const list = useCommand("unit_conversions_list", {});
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [factor, setFactor] = useState("");
  const save = useAction("unit_conversions_save", {
    invalidates: ["unit_conversions_list", "products_search"],
    success: "Conversion saved",
    onSuccess: () => {
      setFrom("");
      setTo("");
      setFactor("");
    },
  });
  const remove = useAction("unit_conversions_delete", {
    invalidates: ["unit_conversions_list", "products_search"],
    success: "Conversion removed",
  });
  // "12" or "2.5" → 12/1 or 5/2 (no floats).
  const parsed = (() => {
    const m = /^(\d+)(?:\.(\d{1,3}))?$/.exec(factor.trim());
    if (!m) return null;
    const frac = m[2] ?? "";
    const den = 10 ** frac.length;
    const num = Number(m[1]) * den + Number(frac || "0");
    return num > 0 ? { num, den } : null;
  })();
  const options = units.map((u) => ({ value: u.id, label: `${u.name} (${u.code})` }));
  return (
    <Section title="Conversions for every product">
      <QueryView query={list}>
        {(rows) => (
          <ul className="mb-4 flex flex-col divide-y divide-border text-sm">
            {rows.length === 0 && (
              <li className="py-2 text-muted-foreground">
                None yet. Products can still have their own pack sizes.
              </li>
            )}
            {rows.map((c) => (
              <li key={c.id} className="flex items-center justify-between py-2">
                <span className="tabular-nums">
                  1 {c.fromUnitCode} = {(c.factorNum / c.factorDen).toString()} {c.toUnitCode}
                </span>
                {canManage && (
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    aria-label={`Remove 1 ${c.fromUnitCode} = ${c.factorNum / c.factorDen} ${c.toUnitCode}`}
                    onClick={() => remove.mutate({ input: { id: c.id } })}
                  >
                    <Trash2 />
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </QueryView>
      {canManage && (
        <form
          className="grid items-end gap-2 sm:grid-cols-[1fr_7rem_1fr_auto]"
          onSubmit={(e) => {
            e.preventDefault();
            if (!parsed) return;
            save.mutate({
              input: {
                id: null,
                fromUnitId: from,
                toUnitId: to,
                factorNum: parsed.num,
                factorDen: parsed.den,
              },
            });
          }}
        >
          <Field>
            <FieldLabel htmlFor="conv-from">1</FieldLabel>
            <SelectField
              id="conv-from"
              value={from}
              placeholder="Unit"
              onValueChange={setFrom}
              options={options}
            />
            <FieldError>{fieldError(save.error, "fromUnitId")}</FieldError>
          </Field>
          <Field>
            <FieldLabel htmlFor="conv-factor">equals</FieldLabel>
            <Input
              id="conv-factor"
              inputMode="decimal"
              value={factor}
              placeholder="12"
              onChange={(e) => setFactor(e.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="conv-to">of</FieldLabel>
            <SelectField
              id="conv-to"
              value={to}
              placeholder="Base unit"
              onValueChange={setTo}
              options={options}
            />
            <FieldError>{fieldError(save.error, "toUnitId")}</FieldError>
          </Field>
          <Button type="submit" disabled={!from || !to || !parsed || save.isPending}>
            <Plus /> Add
          </Button>
        </form>
      )}
    </Section>
  );
}
