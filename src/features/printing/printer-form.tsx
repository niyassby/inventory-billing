import { useState } from "react";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { Printer, Search } from "lucide-react";
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
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { ApiError, call } from "@/api/v2";
import { errorText } from "@/lib/queries";

export type ConnectionType = Api.PrinterConnection["type"];

/** A printer not saved yet. It has its id already, so documents can be routed to it at once. */
export function newPrinter(kind: Api.PrinterKind = "receipt"): Api.PrinterProfile {
  return {
    id: crypto.randomUUID(),
    name: kind === "label" ? "Label printer" : "Receipt printer",
    kind,
    connection: { type: "network", host: "", port: 9100 },
    paper: "mm80",
    codePage: "pc437",
    rupee: "symbol",
    pictureFallback: true,
    cut: "partial",
    feedLines: 4,
    copies: 1,
    hasDrawer: false,
    drawerPin: "pin2",
    labelLanguage: "zpl",
    labelDpi: 203,
    shared: false,
  };
}

export function describeConnection(c: Api.PrinterConnection): string {
  switch (c.type) {
    case "network":
      return `Network ${c.host}:${c.port}`;
    case "serial":
      return `Serial ${c.port}, ${c.baud} baud`;
    case "windows":
      return `Windows printer "${c.queue}"`;
  }
}

/** What went wrong with a test print, in the words of the backend (PRINTER_ERROR kinds). */
function testProblem(error: unknown): string {
  if (error instanceof ApiError && error.code === "PRINTER_ERROR") return error.message;
  return errorText(error);
}

function fieldOf(error: unknown): string | undefined {
  if (!(error instanceof ApiError) || error.code !== "VALIDATION") return undefined;
  const details = error.details as { field?: unknown } | undefined;
  return typeof details?.field === "string" ? details.field : undefined;
}

const BAUDS = ["9600", "19200", "38400", "57600", "115200"].map((b) => ({ value: b, label: b }));

/**
 * The fields of one printer. Used in Settings → Printers and in the first-start printer test,
 * with a test print that works before anything is saved.
 */
export function PrinterFields({
  value,
  onChange,
  showSharing,
}: {
  value: Api.PrinterProfile;
  onChange: (next: Api.PrinterProfile) => void;
  /** Offer "share with counters" (on the hub). */
  showSharing: boolean;
}) {
  const [found, setFound] = useState<Api.Discovered | null>(null);
  const [looking, setLooking] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const set = (patch: Partial<Api.PrinterProfile>) => onChange({ ...value, ...patch });
  const c = value.connection;
  const setType = (type: string) => {
    if (type === "network") set({ connection: { type: "network", host: "", port: 9100 } });
    if (type === "serial") set({ connection: { type: "serial", port: "COM1", baud: 9600 } });
    if (type === "windows") set({ connection: { type: "windows", queue: "" } });
  };
  const look = async () => {
    setLooking(true);
    try {
      setFound(await call("printers_discover", {}));
    } catch (e) {
      setError(e);
    } finally {
      setLooking(false);
    }
  };
  const errorFor = (field: string) =>
    fieldOf(error)?.endsWith(field) ? (error as ApiError).message : undefined;
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <Field>
        <FieldLabel htmlFor="printer-name">Name</FieldLabel>
        <Input
          id="printer-name"
          value={value.name}
          maxLength={60}
          onChange={(e) => set({ name: e.target.value })}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="printer-kind">Kind</FieldLabel>
        <SelectField
          id="printer-kind"
          value={value.kind}
          onValueChange={(v) => set({ kind: v as Api.PrinterKind })}
          options={[
            { value: "receipt", label: "Receipt printer (ESC/POS)" },
            { value: "label", label: "Label printer (ZPL or TSPL)" },
          ]}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="printer-connection">Connected by</FieldLabel>
        <SelectField
          id="printer-connection"
          value={c.type}
          onValueChange={setType}
          options={[
            { value: "network", label: "Network (IP address)" },
            { value: "windows", label: "USB or installed in Windows" },
            { value: "serial", label: "Serial port (COM)" },
          ]}
        />
      </Field>
      {c.type === "network" && (
        <div className="grid grid-cols-[1fr_6rem] gap-2">
          <Field>
            <FieldLabel htmlFor="printer-host">IP address</FieldLabel>
            <Input
              id="printer-host"
              value={c.host}
              placeholder="192.168.1.50"
              inputMode="decimal"
              aria-invalid={Boolean(errorFor("host"))}
              onChange={(e) => set({ connection: { ...c, host: e.target.value } })}
            />
            <FieldError>{errorFor("host")}</FieldError>
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-port">Port</FieldLabel>
            <Input
              id="printer-port"
              value={String(c.port)}
              inputMode="numeric"
              onChange={(e) =>
                set({ connection: { ...c, port: Number(e.target.value.replace(/\D/g, "")) || 0 } })
              }
            />
          </Field>
        </div>
      )}
      {c.type === "windows" && (
        <Field>
          <FieldLabel htmlFor="printer-queue">Windows printer</FieldLabel>
          {found && found.windowsQueues.length > 0 ? (
            <SelectField
              id="printer-queue"
              value={c.queue}
              placeholder="Choose a printer"
              onValueChange={(v) => set({ connection: { type: "windows", queue: v } })}
              options={found.windowsQueues.map((q) => ({ value: q, label: q }))}
            />
          ) : (
            <Input
              id="printer-queue"
              value={c.queue}
              placeholder="As shown in Windows Settings → Printers"
              onChange={(e) => set({ connection: { type: "windows", queue: e.target.value } })}
            />
          )}
          <FieldDescription>
            The printer's own Windows driver must be installed; the app sends it raw commands.
          </FieldDescription>
        </Field>
      )}
      {c.type === "serial" && (
        <div className="grid grid-cols-2 gap-2">
          <Field>
            <FieldLabel htmlFor="printer-serial">Port</FieldLabel>
            {found && found.serialPorts.length > 0 ? (
              <SelectField
                id="printer-serial"
                value={c.port}
                onValueChange={(v) => set({ connection: { ...c, port: v } })}
                options={found.serialPorts.map((p) => ({ value: p, label: p }))}
              />
            ) : (
              <Input
                id="printer-serial"
                value={c.port}
                onChange={(e) => set({ connection: { ...c, port: e.target.value } })}
              />
            )}
            <FieldError>{errorFor("port")}</FieldError>
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-baud">Speed</FieldLabel>
            <SelectField
              id="printer-baud"
              value={String(c.baud)}
              onValueChange={(v) => set({ connection: { ...c, baud: Number(v) } })}
              options={BAUDS}
            />
          </Field>
        </div>
      )}
      {c.type !== "network" && (
        <div className="flex items-end sm:col-span-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void look()}
            disabled={looking}
          >
            <Search /> {looking ? "Looking…" : "Find printers and ports on this PC"}
          </Button>
        </div>
      )}
      {value.kind === "receipt" ? (
        <>
          <Field>
            <FieldLabel htmlFor="printer-paper">Paper</FieldLabel>
            <SelectField
              id="printer-paper"
              value={value.paper ?? "mm80"}
              onValueChange={(v) => set({ paper: v as Api.PaperWidth })}
              options={[
                { value: "mm80", label: "80 mm (48 characters)" },
                { value: "mm58", label: "58 mm (32 characters)" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-codepage">Character set</FieldLabel>
            <SelectField
              id="printer-codepage"
              value={value.codePage ?? "pc437"}
              onValueChange={(v) => set({ codePage: v as Api.CodePage })}
              options={[
                { value: "pc437", label: "PC437 (most printers)" },
                { value: "pc858", label: "PC858 (Western, with €)" },
                { value: "wpc1252", label: "Windows-1252" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-rupee">Rupee sign</FieldLabel>
            <SelectField
              id="printer-rupee"
              value={value.rupee ?? "symbol"}
              onValueChange={(v) => set({ rupee: v as "symbol" | "text" })}
              options={[
                { value: "symbol", label: "₹ (printed as a picture)" },
                { value: "text", label: "Rs. (faster)" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-cut">Cut after printing</FieldLabel>
            <SelectField
              id="printer-cut"
              value={value.cut ?? "partial"}
              onValueChange={(v) => set({ cut: v as Api.CutMode })}
              options={[
                { value: "partial", label: "Partial cut" },
                { value: "full", label: "Full cut" },
                { value: "none", label: "No cutter" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-copies">Copies</FieldLabel>
            <SelectField
              id="printer-copies"
              value={String(value.copies ?? 1)}
              onValueChange={(v) => set({ copies: Number(v) })}
              options={["1", "2", "3"].map((n) => ({ value: n, label: n }))}
            />
          </Field>
          <div className="flex flex-col gap-2 text-sm sm:col-span-2">
            <label className="flex items-start gap-2">
              <Checkbox
                checked={value.pictureFallback ?? true}
                onCheckedChange={(v) => set({ pictureFallback: v })}
              />
              <span>
                Print Hindi, Malayalam and other text the printer lacks as a picture
                <span className="block text-xs text-muted-foreground">
                  Otherwise those letters print as "?".
                </span>
              </span>
            </label>
            <label className="flex items-start gap-2">
              <Checkbox
                checked={value.hasDrawer ?? false}
                onCheckedChange={(v) => set({ hasDrawer: v })}
              />
              <span>A cash drawer is plugged into this printer</span>
            </label>
            {value.hasDrawer && (
              <SelectField
                aria-label="Drawer connector"
                className="max-w-xs"
                value={value.drawerPin ?? "pin2"}
                onValueChange={(v) => set({ drawerPin: v as Api.DrawerPin })}
                options={[
                  { value: "pin2", label: "Drawer on pin 2 (most drawers)" },
                  { value: "pin5", label: "Drawer on pin 5" },
                ]}
              />
            )}
          </div>
        </>
      ) : (
        <>
          <Field>
            <FieldLabel htmlFor="printer-language">Printer language</FieldLabel>
            <SelectField
              id="printer-language"
              value={value.labelLanguage ?? "zpl"}
              onValueChange={(v) => set({ labelLanguage: v as "zpl" | "tspl" })}
              options={[
                { value: "zpl", label: "ZPL (Zebra and compatibles)" },
                { value: "tspl", label: "TSPL (TSC, Xprinter, most others)" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="printer-dpi">Resolution</FieldLabel>
            <SelectField
              id="printer-dpi"
              value={String(value.labelDpi ?? 203)}
              onValueChange={(v) => set({ labelDpi: Number(v) })}
              options={[
                { value: "203", label: "203 dpi (8 dots/mm)" },
                { value: "300", label: "300 dpi (12 dots/mm)" },
              ]}
            />
          </Field>
        </>
      )}
      {showSharing && (
        <label className="flex items-start gap-2 text-sm sm:col-span-2">
          <Checkbox checked={value.shared ?? false} onCheckedChange={(v) => set({ shared: v })} />
          <span>
            Share with counters
            <span className="block text-xs text-muted-foreground">
              Counter PCs can send receipts and labels to this printer through the hub.
            </span>
          </span>
        </label>
      )}
      {error !== null && !fieldOf(error) && (
        <p role="alert" className="text-sm text-destructive sm:col-span-2">
          {errorText(error)}
        </p>
      )}
    </div>
  );
}

/** Prints the test page on the printer as it is in the form. */
export function TestPrintButton({
  printer,
  variant = "outline",
}: {
  printer: Api.PrinterProfile;
  variant?: "outline" | "default";
}) {
  const [busy, setBusy] = useState(false);
  const test = async () => {
    setBusy(true);
    try {
      await call("printers_test", { printer });
      toast.success(`Test page sent to ${printer.name}`, {
        description: "Check the paper: the ruler line should fit across, and ₹ should show.",
      });
    } catch (e) {
      toast.error(testProblem(e), { duration: 12_000 });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Button type="button" variant={variant} onClick={() => void test()} disabled={busy}>
      <Printer /> {busy ? "Printing…" : "Print a test page"}
    </Button>
  );
}

export function PrinterDialog({
  printer,
  isNew,
  showSharing,
  onSave,
  onClose,
}: {
  printer: Api.PrinterProfile | null;
  isNew: boolean;
  showSharing: boolean;
  onSave: (printer: Api.PrinterProfile) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<Api.PrinterProfile | null>(printer);
  const [shown, setShown] = useState(printer);
  if (printer !== shown) {
    setShown(printer);
    // Keep the last contents while the dialog animates closed.
    if (printer) setDraft(printer);
  }
  return (
    <Dialog open={printer !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{isNew ? "Add a printer" : "Edit printer"}</DialogTitle>
          <DialogDescription>
            Network printers must be on the shop network. Test before saving.
          </DialogDescription>
        </DialogHeader>
        {draft && <PrinterFields value={draft} onChange={setDraft} showSharing={showSharing} />}
        <DialogFooter className="gap-2">
          {draft && <TestPrintButton printer={draft} />}
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="button"
            disabled={!draft || draft.name.trim() === ""}
            onClick={() => draft && onSave(draft)}
          >
            Keep
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
