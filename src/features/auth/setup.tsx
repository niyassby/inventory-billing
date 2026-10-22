import { SelectField } from "@/components/ui/select-field";
import { useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import type { Api } from "@inventory/shared";
import { isValidGstin } from "@inventory/shared";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { call } from "@/api/v2";
import { errorText } from "@/lib/queries";
import { showMoney } from "@/lib/format";
import { useSession } from "@/stores/session";
import { PrinterSetupStep } from "@/features/printing/printer-setup-step";
import { AuthFrame } from "./sign-in";

const setupSchema = z
  .object({
    businessName: z.string().trim().min(1, "Enter the business name").max(120),
    address: z.string().trim().max(300),
    phone: z.string().trim().max(20),
    gstin: z
      .string()
      .trim()
      .toUpperCase()
      .refine((v) => v === "" || isValidGstin(v), "This GSTIN is not valid"),
    taxRegime: z.enum(["gst", "vat", "none"]),
    pricesIncludeTax: z.boolean(),
    ownerName: z.string().trim().min(1, "Enter your name").max(80),
    ownerEmail: z.email("Enter a valid e-mail").trim(),
    password: z.string().min(10, "Use at least 10 characters"),
    passwordAgain: z.string(),
  })
  .refine((v) => v.password === v.passwordAgain, {
    path: ["passwordAgain"],
    message: "The passwords don't match",
  });

type SetupForm = z.infer<typeof setupSchema>;

function NewBusiness({ onDone }: { onDone: () => void }) {
  const signIn = useSession((s) => s.signIn);
  const [error, setError] = useState<string | null>(null);
  const form = useForm<SetupForm>({
    resolver: zodResolver(setupSchema),
    defaultValues: {
      businessName: "",
      address: "",
      phone: "",
      gstin: "",
      taxRegime: "gst",
      pricesIncludeTax: true,
      ownerName: "",
      ownerEmail: "",
      password: "",
      passwordAgain: "",
    },
  });
  const { register, formState } = form;
  const err = (name: keyof SetupForm) => formState.errors[name]?.message;

  const submit = form.handleSubmit(async (v) => {
    setError(null);
    try {
      const session = await call("setup_create_business", {
        businessName: v.businessName,
        address: v.address || null,
        phone: v.phone || null,
        gstin: v.gstin || null,
        taxRegime: v.taxRegime,
        pricesIncludeTax: v.pricesIncludeTax,
        owner: { name: v.ownerName, email: v.ownerEmail, password: v.password },
      });
      signIn(session);
      onDone();
    } catch (e) {
      setError(errorText(e));
    }
  });

  return (
    <form className="flex flex-col gap-4" onSubmit={(e) => void submit(e)}>
      <p className="text-sm font-medium">Your business</p>
      <Field data-invalid={Boolean(err("businessName")) || undefined}>
        <FieldLabel htmlFor="businessName">Business name</FieldLabel>
        <Input id="businessName" autoFocus {...register("businessName")} />
        <FieldError>{err("businessName")}</FieldError>
      </Field>
      <Field>
        <FieldLabel htmlFor="address">Address (printed on bills)</FieldLabel>
        <Input id="address" {...register("address")} />
      </Field>
      <div className="grid grid-cols-2 gap-3">
        <Field>
          <FieldLabel htmlFor="phone">Phone</FieldLabel>
          <Input id="phone" inputMode="tel" {...register("phone")} />
        </Field>
        <Field data-invalid={Boolean(err("gstin")) || undefined}>
          <FieldLabel htmlFor="gstin">GSTIN (optional)</FieldLabel>
          <Input id="gstin" className="uppercase" {...register("gstin")} />
          <FieldError>{err("gstin")}</FieldError>
        </Field>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field>
          <FieldLabel htmlFor="taxRegime">Tax</FieldLabel>
          <Controller
            control={form.control}
            name="taxRegime"
            render={({ field }) => (
              <SelectField
                id="taxRegime"
                value={field.value}
                onValueChange={field.onChange}
                options={[
                  { value: "gst", label: "GST (India)" },
                  { value: "vat", label: "VAT" },
                  { value: "none", label: "No tax" },
                ]}
              />
            )}
          />
        </Field>
        <Field orientation="horizontal" className="items-center self-end pb-2">
          <Checkbox
            id="pricesIncludeTax"
            checked={form.watch("pricesIncludeTax")}
            onCheckedChange={(checked) => form.setValue("pricesIncludeTax", checked)}
          />
          <FieldLabel htmlFor="pricesIncludeTax">Prices include tax</FieldLabel>
        </Field>
      </div>
      <p className="mt-2 text-sm font-medium">Owner account</p>
      <Field data-invalid={Boolean(err("ownerName")) || undefined}>
        <FieldLabel htmlFor="ownerName">Your name</FieldLabel>
        <Input id="ownerName" {...register("ownerName")} />
        <FieldError>{err("ownerName")}</FieldError>
      </Field>
      <Field data-invalid={Boolean(err("ownerEmail")) || undefined}>
        <FieldLabel htmlFor="ownerEmail">E-mail (you sign in with it)</FieldLabel>
        <Input id="ownerEmail" type="email" autoComplete="username" {...register("ownerEmail")} />
        <FieldError>{err("ownerEmail")}</FieldError>
      </Field>
      <div className="grid grid-cols-2 gap-3">
        <Field data-invalid={Boolean(err("password")) || undefined}>
          <FieldLabel htmlFor="password">Password</FieldLabel>
          <Input
            id="password"
            type="password"
            autoComplete="new-password"
            {...register("password")}
          />
          <FieldError>{err("password")}</FieldError>
        </Field>
        <Field data-invalid={Boolean(err("passwordAgain")) || undefined}>
          <FieldLabel htmlFor="passwordAgain">Again</FieldLabel>
          <Input
            id="passwordAgain"
            type="password"
            autoComplete="new-password"
            {...register("passwordAgain")}
          />
          <FieldError>{err("passwordAgain")}</FieldError>
        </Field>
      </div>
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      <Button type="submit" size="lg" disabled={formState.isSubmitting}>
        {formState.isSubmitting ? "Setting up…" : "Create business"}
      </Button>
    </form>
  );
}

function ReportSummary({ report }: { report: Api.MigrationReport }) {
  const t = report.totals;
  const rows: [string, string, string][] = [
    ["Sales", t.legacySalesTotal.toFixed(2), showMoney(t.salesTotalMinor)],
    ["Purchases", t.legacyPurchaseTotal.toFixed(2), showMoney(t.purchaseTotalMinor)],
    ["Stock value", t.legacyStockValue.toFixed(2), showMoney(t.stockValueMinor)],
    ["Expenses", t.legacyExpenseTotal.toFixed(2), showMoney(t.expenseTotalMinor)],
  ];
  const tables = Object.entries(report.tables);
  return (
    <div className="flex flex-col gap-3 text-sm">
      <table className="w-full">
        <thead className="text-xs text-muted-foreground">
          <tr>
            <th className="text-left font-medium" />
            <th className="text-right font-medium">Old version</th>
            <th className="text-right font-medium">After import</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([name, before, after]) => (
            <tr key={name}>
              <td className="py-0.5">{name}</td>
              <td className="text-right tabular-nums">{before}</td>
              <td className="text-right tabular-nums">{after}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="text-muted-foreground">
        {tables.reduce((n, [, c]) => n + c.migratedRows, 0)} records in {tables.length} tables.
        {report.balanceAdjustments.length > 0 &&
          ` ${report.balanceAdjustments.length} balance difference(s) will be listed for review.`}
        {report.usersRequiringPasswordChange.length > 0 &&
          ` ${report.usersRequiringPasswordChange.length} user(s) must choose a new password.`}
      </p>
      {report.warnings.length > 0 && (
        <details className="rounded-xl border border-border p-3">
          <summary className="cursor-pointer font-medium">{report.warnings.length} note(s)</summary>
          <ul className="mt-2 list-disc pl-5 text-muted-foreground">
            {report.warnings.slice(0, 50).map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

function ImportOldData({ onDone, onFresh }: { onDone: () => void; onFresh: () => void }) {
  const [preview, setPreview] = useState<Api.ImportLegacyResult | null>(null);
  const [done, setDone] = useState<Api.ImportLegacyResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (dryRun: boolean) => {
    setBusy(true);
    setError(null);
    try {
      const result = await call("setup_import_legacy", { dryRun });
      if (dryRun) setPreview(result);
      else setDone(result);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  if (done) {
    return (
      <div className="flex flex-col gap-4">
        <p className="text-sm">
          Your data has been moved to the new version. The old file was not changed, and a copy was
          kept at <code className="text-xs break-all">{done.report.backupPath}</code>.
        </p>
        <ReportSummary report={done.report} />
        <Button size="lg" onClick={onDone}>
          Sign in with your existing account
        </Button>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm">
        Data from the previous version was found on this PC. Check it first: nothing changes until
        you confirm.
      </p>
      {preview && <ReportSummary report={preview.report} />}
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      {preview ? (
        <Button size="lg" disabled={busy} onClick={() => void run(false)}>
          {busy ? "Importing…" : "Import my data"}
        </Button>
      ) : (
        <Button size="lg" disabled={busy} onClick={() => void run(true)}>
          {busy ? "Checking…" : "Check my data"}
        </Button>
      )}
      <Button variant="ghost" onClick={onFresh}>
        Start with an empty business instead
      </Button>
    </div>
  );
}

/** First start: import the old data, or set up a new business. */
export function Setup({ status }: { status: Api.SetupStatus }) {
  const queryClient = useQueryClient();
  const [fresh, setFresh] = useState(!status.legacyDatabaseFound);
  const [printerStep, setPrinterStep] = useState(false);
  const done = () => void queryClient.invalidateQueries({ queryKey: ["setup_status"] });
  if (printerStep) {
    return (
      <AuthFrame title="Receipt printer" subtitle="Optional. You can change it later in Settings.">
        <PrinterSetupStep onDone={done} />
      </AuthFrame>
    );
  }
  return (
    <AuthFrame
      title={fresh ? "Set up your business" : "Welcome to the new version"}
      subtitle={fresh ? "Takes a minute. You can change everything later in Settings." : undefined}
    >
      {fresh ? (
        <>
          <NewBusiness onDone={() => setPrinterStep(true)} />
          {status.legacyDatabaseFound && (
            <FieldDescription className="mt-3 text-center">
              <button type="button" className="underline" onClick={() => setFresh(false)}>
                Import data from the previous version instead
              </button>
            </FieldDescription>
          )}
        </>
      ) : (
        <ImportOldData onDone={done} onFresh={() => setFresh(true)} />
      )}
    </AuthFrame>
  );
}
