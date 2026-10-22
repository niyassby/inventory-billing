import { useState } from "react";
import type { Api } from "@inventory/shared";
import { Button } from "@/components/ui/button";
import { call } from "@/api/v2";
import { errorText } from "@/lib/queries";
import { newPrinter, PrinterFields, TestPrintButton } from "./printer-form";

/**
 * First start: set up and test the receipt printer, or skip (receipts then print through the
 * system print dialog, as before). Everything can be changed later in Settings → Printers.
 */
export function PrinterSetupStep({ onDone }: { onDone: () => void }) {
  const [printer, setPrinter] = useState<Api.PrinterProfile>(() => newPrinter("receipt"));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const id = printer.id ?? "";
      await call("printers_config_save", {
        printers: [printer],
        routes: { receipt: { printerId: id, onHub: false } },
        autoPrintReceipt: true,
        drawerOnCash: Boolean(printer.hasDrawer),
        drawerPrinterId: printer.hasDrawer ? id : null,
      });
      onDone();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        Connect the receipt printer and print a test page. Skip this if you print bills on an office
        printer: they then go through the normal print dialog.
      </p>
      <PrinterFields value={printer} onChange={setPrinter} showSharing={false} />
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <TestPrintButton printer={printer} />
        <Button onClick={() => void save()} disabled={saving || printer.name.trim() === ""}>
          {saving ? "Saving…" : "Use this printer"}
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Skip for now
        </Button>
      </div>
    </div>
  );
}
