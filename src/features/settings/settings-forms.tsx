import { SelectField } from "@/components/ui/select-field";
import { useEffect, useState } from "react";
import type { Api } from "@inventory/shared";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { QtyInput, RateInput } from "@/components/app/inputs";
import { QueryView, Section } from "@/components/app/page";
import { SaveBar } from "@/components/app/save-bar";
import { useAction, useCommand } from "@/lib/queries";
import { useCan } from "@/stores/session";

type Draft = Record<string, unknown>;

export type FieldSpec =
  | {
      kind: "text" | "textarea" | "time";
      key: string;
      label: string;
      help?: string;
      placeholder?: string;
    }
  | { kind: "number"; key: string; label: string; help?: string; min?: number; max?: number }
  | { kind: "qty" | "rate"; key: string; label: string; help?: string }
  | { kind: "bool"; key: string; label: string; help?: string }
  | { kind: "select"; key: string; label: string; help?: string; options: [string, string][] }
  /** A list of strings, one per line. */
  | { kind: "lines"; key: string; label: string; help?: string };

const text = (v: unknown): string => (typeof v === "string" ? v : "");

function FieldInput({
  spec,
  value,
  onChange,
  disabled,
}: {
  spec: FieldSpec;
  value: unknown;
  onChange: (v: unknown) => void;
  disabled: boolean;
}) {
  const id = `s-${spec.key}`;
  switch (spec.kind) {
    case "bool":
      return (
        <label className="flex items-start gap-2 text-sm">
          <Checkbox checked={value === true} disabled={disabled} onCheckedChange={onChange} />
          <span>
            {spec.label}
            {spec.help && <span className="block text-xs text-muted-foreground">{spec.help}</span>}
          </span>
        </label>
      );
    case "select":
      return (
        <Field>
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <SelectField
            id={id}
            disabled={disabled}
            value={text(value)}
            onValueChange={(v) => onChange(v)}
            options={spec.options.map(([v, t]) => ({ value: v, label: t }))}
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
    case "textarea":
      return (
        <Field className="md:col-span-2">
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <Textarea
            id={id}
            disabled={disabled}
            value={text(value)}
            onChange={(e) => onChange(e.target.value || null)}
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
    case "lines":
      return (
        <Field className="md:col-span-2">
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <Textarea
            id={id}
            disabled={disabled}
            value={Array.isArray(value) ? (value as string[]).join("\n") : ""}
            onChange={(e) =>
              onChange(
                e.target.value
                  .split("\n")
                  .map((l) => l.trim())
                  .filter(Boolean),
              )
            }
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
    case "qty":
      return (
        <Field>
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <QtyInput
            id={id}
            disabled={disabled}
            value={typeof value === "number" ? value : null}
            onValueChange={(v) => onChange(v ?? 0)}
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
    case "rate":
      return (
        <Field>
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <RateInput
            id={id}
            disabled={disabled}
            value={typeof value === "number" ? value : null}
            onValueChange={(v) => onChange(v ?? 0)}
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
    case "number":
      return (
        <Field>
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <Input
            id={id}
            inputMode="numeric"
            disabled={disabled}
            value={typeof value === "number" ? String(value) : ""}
            onChange={(e) => {
              const n = e.target.value.replace(/\D/g, "");
              onChange(n === "" ? 0 : Number(n));
            }}
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
    default:
      return (
        <Field>
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          <Input
            id={id}
            disabled={disabled}
            placeholder={spec.placeholder}
            value={text(value)}
            onChange={(e) => onChange(e.target.value || null)}
          />
          {spec.help && <FieldDescription>{spec.help}</FieldDescription>}
        </Field>
      );
  }
}

/** One settings section as a form: edited as a whole, saved with one click, audited by the backend. */
export function SectionForm({
  section,
  title,
  description,
  fields,
  settings,
  permission = "settings.manage",
}: {
  section: keyof Api.Settings;
  title: string;
  description?: string;
  fields: FieldSpec[];
  settings: Api.Settings;
  permission?: string;
}) {
  const canEdit = useCan(permission);
  const original = settings[section] as unknown as Draft;
  const [draft, setDraft] = useState<Draft>(original);
  useEffect(() => setDraft(original), [original]);
  const save = useAction("settings_update", {
    invalidates: ["settings_get", "settings_public"],
    success: `${title} saved`,
  });
  const dirty = JSON.stringify(draft) !== JSON.stringify(original);
  return (
    <Section title={title}>
      {description && <p className="mb-4 text-sm text-muted-foreground">{description}</p>}
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate({ input: { section, value: draft } });
        }}
      >
        <div className="grid gap-4 md:grid-cols-2">
          {fields.map((f) => (
            <FieldInput
              key={f.key}
              spec={f}
              value={draft[f.key]}
              disabled={!canEdit}
              onChange={(v) => setDraft((d) => ({ ...d, [f.key]: v }))}
            />
          ))}
        </div>
        {canEdit && (
          <SaveBar dirty={dirty} saving={save.isPending} onDiscard={() => setDraft(original)} />
        )}
      </form>
    </Section>
  );
}

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
].map((m, i): [string, string] => [String(i + 1), m]);

export const SECTIONS: Record<
  string,
  { title: string; description?: string; fields: FieldSpec[]; permission?: string }
> = {
  business: {
    title: "Business profile",
    description:
      "Printed on every bill and used for tax: GSTIN and state decide CGST+SGST versus IGST.",
    fields: [
      { kind: "text", key: "name", label: "Business name" },
      { kind: "text", key: "gstin", label: "GSTIN" },
      { kind: "textarea", key: "address", label: "Address" },
      { kind: "text", key: "phone", label: "Phone" },
      { kind: "text", key: "email", label: "E-mail" },
      {
        kind: "text",
        key: "stateCode",
        label: "GST state code",
        help: "Two digits, for example 29 for Karnataka. Taken from the GSTIN when empty.",
      },
      {
        kind: "text",
        key: "currency",
        label: "Currency code",
        help: "INR, USD, EUR … Changing it later doesn't convert old amounts.",
      },
      {
        kind: "number",
        key: "currencyMinorDigits",
        label: "Decimals in the currency",
        min: 0,
        max: 3,
      },
      {
        kind: "text",
        key: "timezone",
        label: "Time zone",
        help: 'For example Asia/Kolkata. Decides what "today" is.',
      },
      { kind: "select", key: "fyStartMonth", label: "Financial year starts in", options: MONTHS },
      {
        kind: "text",
        key: "locale",
        label: "Number format",
        help: "en-IN groups 1,00,000; en-US groups 100,000.",
      },
    ],
  },
  tax: {
    title: "Tax and pricing",
    fields: [
      {
        kind: "select",
        key: "regime",
        label: "Tax",
        options: [
          ["gst", "GST (India)"],
          ["vat", "VAT"],
          ["none", "No tax"],
        ],
      },
      {
        kind: "bool",
        key: "pricesIncludeTax",
        label: "Prices include tax",
        help: "Shelf prices already contain the tax (retail style). Each bill can override this.",
      },
      {
        kind: "number",
        key: "roundTotalTo",
        label: "Round bill totals to",
        help: "In the smallest unit: 100 = nearest ₹1, 50 = nearest 50 paise, 0 = don't round.",
      },
      {
        kind: "select",
        key: "rounding",
        label: "Rounding",
        options: [
          ["half_up", "Half up (0.5 rounds up)"],
          ["half_even", "Half even (banker's)"],
        ],
      },
    ],
  },
  invoice: {
    title: "Invoices and printing",
    fields: [
      {
        kind: "select",
        key: "paper",
        label: "Default paper",
        options: [
          ["thermal_80", "Thermal 80 mm"],
          ["thermal_58", "Thermal 58 mm"],
          ["a4", "A4"],
        ],
      },
      {
        kind: "bool",
        key: "showQr",
        label: "Show a QR code (when supported by the printer layout)",
      },
      {
        kind: "textarea",
        key: "footerText",
        label: "Footer text",
        help: 'For example "Thank you, visit again".',
      },
      {
        kind: "textarea",
        key: "terms",
        label: "Terms and conditions",
        help: "Printed on A4 invoices and quotations.",
      },
    ],
  },
  inventory: {
    title: "Inventory",
    fields: [
      {
        kind: "select",
        key: "negativeStockPolicy",
        label: "When stock would go below zero",
        options: [
          ["block", "Block the sale (safest)"],
          ["warn", "Allow, with a warning"],
          ["allow", "Allow silently"],
        ],
      },
      {
        kind: "select",
        key: "expiredStockPolicy",
        label: "Selling an expired batch",
        help: "Batch products are sold first-to-expire first; this decides what happens to expired batches.",
        options: [
          ["block", "Never sell it (safest)"],
          ["warn", "Only when nothing else is left, with a warning"],
          ["allow", "Sell it like any other batch"],
        ],
      },
      {
        kind: "number",
        key: "expiryAlertDays",
        label: "Near-expiry warning (days before expiry)",
        min: 1,
        max: 365,
      },
      {
        kind: "bool",
        key: "expiryAlerts",
        label: "Expiry notices on the dashboard",
        help: "Batches that have expired or expire within the warning window.",
      },
      {
        kind: "bool",
        key: "lowStockAlerts",
        label: "Low-stock notices on the dashboard",
        help: "Items at or below their reorder level.",
      },
    ],
  },
  security: {
    title: "Security",
    description:
      "Applies to everyone who signs in. A locked account unlocks itself after the lockout time.",
    fields: [
      { kind: "number", key: "sessionHours", label: "Sign-in lasts at most (hours)" },
      { kind: "number", key: "idleTimeoutMinutes", label: "Sign out after idle (minutes)" },
      { kind: "number", key: "maxFailedLogins", label: "Lock after wrong passwords" },
      { kind: "number", key: "lockoutMinutes", label: "Locked for (minutes)" },
      { kind: "number", key: "minPasswordLength", label: "Shortest password" },
      {
        kind: "bool",
        key: "pinLoginEnabled",
        label: "Quick sign-in with a PIN",
        help: "For cashiers switching at a counter.",
      },
    ],
  },
  network: {
    title: "Network and offline sales",
    description: "How counters behave when the hub PC can't be reached. See the Admin Guide.",
    fields: [
      {
        kind: "select",
        key: "offlineSalesPolicy",
        label: "Sales while the hub is offline",
        options: [
          ["allow_reserved_numbers", "Allowed (numbers reserved in advance)"],
          ["allow_restricted", "Allowed, except low-stock and batch items"],
          ["block", "Not allowed"],
        ],
      },
      { kind: "number", key: "leaseSize", label: "Invoice numbers reserved per counter" },
      { kind: "qty", key: "restrictedMinStock", label: 'Low-stock limit for "restricted" (units)' },
      {
        kind: "number",
        key: "maxClockSkewHours",
        label: "Flag counters with a clock off by (hours)",
      },
      {
        kind: "number",
        key: "port",
        label: "Hub port",
        help: "Restart the app after changing it, then pair counters again.",
      },
      {
        kind: "number",
        key: "catalogRefreshMinutes",
        label: "Counter catalogue refresh (minutes)",
      },
      {
        kind: "lines",
        key: "allowedSubnets",
        label: "Extra networks allowed to connect",
        help: "One per line, like 192.168.5.0/24. Home and office networks are always allowed.",
      },
    ],
  },
  backup: {
    title: "Backup schedule",
    permission: "backup.manage",
    fields: [
      { kind: "bool", key: "enabled", label: "Back up automatically" },
      { kind: "time", key: "dailyAt", label: "Daily backup at", help: "24-hour time, like 21:00." },
      {
        kind: "number",
        key: "intervalHours",
        label: "Also every (hours)",
        help: "0 = once a day only.",
      },
      { kind: "number", key: "keepDaily", label: "Daily backups to keep" },
      { kind: "number", key: "keepWeekly", label: "Weekly backups to keep" },
      { kind: "number", key: "keepMonthly", label: "Monthly backups to keep" },
      {
        kind: "text",
        key: "directory",
        label: "Backup folder",
        help: 'A full path. Empty = a "backups" folder next to the data.',
      },
      {
        kind: "text",
        key: "mirrorDirectory",
        label: "Second copy (USB drive or network share)",
        help: "A full path. Empty = no second copy.",
      },
    ],
  },
};

export function SettingsSection({ name }: { name: string }) {
  const spec = SECTIONS[name];
  const settings = useCommand("settings_get", {});
  if (!spec) return null;
  return (
    <QueryView query={settings}>
      {(s) => (
        <SectionForm
          section={name as keyof Api.Settings}
          title={spec.title}
          fields={spec.fields}
          settings={s}
          {...(spec.description ? { description: spec.description } : {})}
          {...(spec.permission ? { permission: spec.permission } : {})}
        />
      )}
    </QueryView>
  );
}
