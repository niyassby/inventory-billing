import { useState } from "react";
import type { Api } from "@inventory/shared";
import { FileUp } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
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
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { call } from "@/api/v2";
import { useMoney } from "@/lib/business";
import { showDate } from "@/lib/format";
import { errorText, useAction } from "@/lib/queries";

const NONE = "";

/** Column choices from the file's header (or "Column 1"…), plus "none" where it is optional. */
function columnOptions(columns: string[], optional: boolean) {
  return [
    ...(optional ? [{ value: NONE, label: "—" }] : []),
    ...columns.map((c, i) => ({ value: String(i), label: c })),
  ];
}

const pick = (v: string) => (v === NONE ? null : Number(v));
const shown = (n: number | null | undefined) => (n === null || n === undefined ? NONE : String(n));

/**
 * Bank statement import: choose the file, map its columns once (remembered per account), check
 * the preview, import. Lines already imported are skipped, so an overlapping file is safe.
 */
export function StatementImport({
  accountId,
  accountName,
  onClose,
}: {
  accountId: string;
  accountName: string;
  onClose: () => void;
}) {
  const money = useMoney();
  const [file, setFile] = useState<{ name: string; text: string } | null>(null);
  const [result, setResult] = useState<Api.StatementImportResult | null>(null);
  const [mapping, setMapping] = useState<Api.StatementMapping | null>(null);
  const [twoColumns, setTwoColumns] = useState(true);
  const [remember, setRemember] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const importNow = useAction("bank_import", {
    invalidates: ["bank_view", "bank_reconciliation"],
    success: (r) =>
      `${r.newRows} line(s) imported${r.duplicates ? `, ${r.duplicates} were already there` : ""}`,
    onSuccess: onClose,
  });
  const ask = async (text: string, name: string, m: Api.StatementMapping | null) => {
    try {
      setError(null);
      const r = await call("bank_import", {
        accountId,
        fileName: name,
        csv: text,
        mapping: m,
        saveMapping: false,
        apply: false,
      });
      setResult(r);
      if (!m && r.mapping) {
        setMapping(r.mapping);
        setTwoColumns(r.mapping.amountColumn === null || r.mapping.amountColumn === undefined);
      }
    } catch (e) {
      setError(errorText(e));
    }
  };
  const choose = async (picked: File | undefined) => {
    if (!picked) return;
    if (picked.size > 3 * 1024 * 1024) {
      setError("Statements can be at most 3 MB.");
      return;
    }
    const text = await picked.text();
    setFile({ name: picked.name, text });
    await ask(text, picked.name, null);
  };
  const update = (patch: Partial<Api.StatementMapping>) => {
    const next: Api.StatementMapping = {
      skipRows: 0,
      hasHeader: true,
      dateColumn: 0,
      dateFormat: "DD/MM/YYYY",
      descriptionColumn: 1,
      ...mapping,
      ...patch,
    };
    setMapping(next);
    if (file) void ask(file.text, file.name, next);
  };
  const columns = result?.columns ?? [];
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>Import a statement into {accountName}</DialogTitle>
          <DialogDescription>
            A CSV file from your bank's website. Match its columns once; the app remembers them for
            this account.
          </DialogDescription>
        </DialogHeader>
        <div className="flex max-h-[65vh] flex-col gap-4 overflow-y-auto pr-1">
          <label className="flex w-fit cursor-pointer items-center gap-2 rounded-md border border-border px-3 py-2 text-sm hover:bg-muted">
            <FileUp className="size-4" aria-hidden />
            {file ? file.name : "Choose the CSV file"}
            <input
              type="file"
              accept=".csv,text/csv,text/plain"
              className="sr-only"
              aria-label="Statement file"
              onChange={(e) => void choose(e.target.files?.[0])}
            />
          </label>
          {error && (
            <Alert variant="danger">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          {result && (
            <>
              <section
                aria-label="The file as read"
                className="overflow-auto rounded-md border border-border"
              >
                <table className="w-full text-xs">
                  <tbody>
                    {result.sample.map((row, i) => (
                      <tr key={i} className="border-b border-border/40">
                        <td className="px-2 py-1 text-muted-foreground">{i + 1}</td>
                        {row.map((c, j) => (
                          <td key={j} className="px-2 py-1 whitespace-nowrap">
                            {c}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>
              <div className="grid gap-3 sm:grid-cols-3">
                <Field>
                  <FieldLabel htmlFor="si-skip">Lines before the header</FieldLabel>
                  <Input
                    id="si-skip"
                    inputMode="numeric"
                    value={String(mapping?.skipRows ?? 0)}
                    onChange={(e) =>
                      update({ skipRows: Number(e.target.value.replace(/\D/g, "") || 0) })
                    }
                  />
                </Field>
                <label className="flex items-end gap-2 pb-2 text-sm">
                  <Checkbox
                    checked={mapping?.hasHeader ?? true}
                    onCheckedChange={(v) => update({ hasHeader: v })}
                  />
                  The first line read is a header
                </label>
                <Field>
                  <FieldLabel htmlFor="si-format">Dates look like</FieldLabel>
                  <SelectField
                    id="si-format"
                    value={mapping?.dateFormat ?? "DD/MM/YYYY"}
                    onValueChange={(dateFormat) => update({ dateFormat })}
                    options={result.dateFormats.map((f) => ({ value: f, label: f }))}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="si-date">Date</FieldLabel>
                  <SelectField
                    id="si-date"
                    value={shown(mapping?.dateColumn ?? null)}
                    onValueChange={(v) => update({ dateColumn: Number(v) })}
                    options={columnOptions(columns, false)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="si-desc">Description</FieldLabel>
                  <SelectField
                    id="si-desc"
                    value={shown(mapping?.descriptionColumn ?? null)}
                    onValueChange={(v) => update({ descriptionColumn: Number(v) })}
                    options={columnOptions(columns, false)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="si-ref">Reference (cheque no., UTR)</FieldLabel>
                  <SelectField
                    id="si-ref"
                    value={shown(mapping?.referenceColumn)}
                    onValueChange={(v) => update({ referenceColumn: pick(v) })}
                    options={columnOptions(columns, true)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="si-mode">Amounts</FieldLabel>
                  <SelectField
                    id="si-mode"
                    value={twoColumns ? "two" : "one"}
                    onValueChange={(v) => {
                      setTwoColumns(v === "two");
                      update(
                        v === "two"
                          ? { amountColumn: null }
                          : { withdrawalColumn: null, depositColumn: null },
                      );
                    }}
                    options={[
                      { value: "two", label: "Withdrawal and deposit columns" },
                      { value: "one", label: "One column (+ in, − out)" },
                    ]}
                  />
                </Field>
                {twoColumns ? (
                  <>
                    <Field>
                      <FieldLabel htmlFor="si-out">Withdrawal (debit)</FieldLabel>
                      <SelectField
                        id="si-out"
                        value={shown(mapping?.withdrawalColumn)}
                        onValueChange={(v) => update({ withdrawalColumn: pick(v) })}
                        options={columnOptions(columns, true)}
                      />
                    </Field>
                    <Field>
                      <FieldLabel htmlFor="si-in">Deposit (credit)</FieldLabel>
                      <SelectField
                        id="si-in"
                        value={shown(mapping?.depositColumn)}
                        onValueChange={(v) => update({ depositColumn: pick(v) })}
                        options={columnOptions(columns, true)}
                      />
                    </Field>
                  </>
                ) : (
                  <Field>
                    <FieldLabel htmlFor="si-amount">Amount</FieldLabel>
                    <SelectField
                      id="si-amount"
                      value={shown(mapping?.amountColumn)}
                      onValueChange={(v) => update({ amountColumn: pick(v) })}
                      options={columnOptions(columns, true)}
                    />
                  </Field>
                )}
                <Field>
                  <FieldLabel htmlFor="si-bal">Balance (optional)</FieldLabel>
                  <SelectField
                    id="si-bal"
                    value={shown(mapping?.balanceColumn)}
                    onValueChange={(v) => update({ balanceColumn: pick(v) })}
                    options={columnOptions(columns, true)}
                  />
                </Field>
              </div>
              {mapping && (
                <section aria-label="What will be imported">
                  <p className="mb-2 text-sm">
                    {result.totalRows} line(s)
                    {result.fromDate &&
                      ` from ${showDate(result.fromDate)} to ${showDate(result.toDate ?? result.fromDate)}`}
                    : <strong>{result.newRows} new</strong>
                    {result.duplicates > 0 && `, ${result.duplicates} already imported`}
                    {result.errors > 0 && (
                      <span className="text-danger">, {result.errors} can't be read</span>
                    )}
                  </p>
                  <div className="overflow-auto rounded-md border border-border">
                    <table className="w-full text-sm">
                      <thead className="text-xs text-muted-foreground">
                        <tr>
                          <th className="px-2 py-1 text-left font-medium">Line</th>
                          <th className="px-2 py-1 text-left font-medium">Date</th>
                          <th className="px-2 py-1 text-left font-medium">Description</th>
                          <th className="px-2 py-1 text-right font-medium">Amount</th>
                          <th className="px-2 py-1 text-right font-medium">Balance</th>
                        </tr>
                      </thead>
                      <tbody>
                        {result.rows.slice(0, 50).map((r) => (
                          <tr
                            key={r.rowNo}
                            className={`border-t border-border/40 ${r.duplicate ? "text-muted-foreground" : ""}`}
                          >
                            <td className="px-2 py-1">{r.rowNo}</td>
                            <td className="px-2 py-1">{r.txnDate ? showDate(r.txnDate) : ""}</td>
                            <td className="px-2 py-1">
                              {r.error ? (
                                <span className="text-danger">{r.error}</span>
                              ) : (
                                r.description
                              )}
                              {r.duplicate && " (already imported)"}
                            </td>
                            <td className="px-2 py-1 text-right tabular-nums">
                              {r.amount !== null && r.amount !== undefined ? money(r.amount) : ""}
                            </td>
                            <td className="px-2 py-1 text-right tabular-nums">
                              {r.balance !== null && r.balance !== undefined
                                ? money(r.balance)
                                : ""}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </section>
              )}
              <label className="flex items-center gap-2 text-sm">
                <Checkbox checked={remember} onCheckedChange={setRemember} />
                Remember these columns for {accountName}
              </label>
            </>
          )}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={
              !file ||
              !mapping ||
              !result ||
              result.errors > 0 ||
              result.newRows === 0 ||
              importNow.isPending
            }
            onClick={() =>
              file &&
              importNow.mutate({
                input: {
                  accountId,
                  fileName: file.name,
                  csv: file.text,
                  mapping,
                  saveMapping: remember,
                  apply: true,
                },
              })
            }
          >
            Import {result?.newRows ?? 0} line(s)
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
