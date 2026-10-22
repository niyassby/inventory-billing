import { useState } from "react";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { Inbox, Pencil, Plus, RotateCcw, Trash2, Vault, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { EmptyState, QueryView, Section } from "@/components/app/page";
import { SaveBar } from "@/components/app/save-bar";
import { call } from "@/api/v2";
import { useConnection } from "@/hooks/use-connection";
import { label, showDateTime } from "@/lib/format";
import { errorText, useAction, useCommand, useList } from "@/lib/queries";
import { useCan } from "@/stores/session";
import { describeConnection, newPrinter, PrinterDialog, TestPrintButton } from "./printer-form";

const DOCS: { key: keyof Api.Routes; title: string; kind: Api.PrinterKind; help: string }[] = [
  {
    key: "receipt",
    title: "Till receipts",
    kind: "receipt",
    help: "After each sale and on reprint.",
  },
  { key: "returnSlip", title: "Return slips", kind: "receipt", help: "Credit notes for returns." },
  {
    key: "tillClose",
    title: "Till close summary",
    kind: "receipt",
    help: "When a till is closed.",
  },
  { key: "heldSlip", title: "Held-bill slips", kind: "receipt", help: "When a bill is parked." },
  { key: "labels", title: "Barcode labels", kind: "label", help: "From the Labels screen." },
];

function routeValue(route: Api.PrintRoute | null | undefined): string {
  if (!route) return "";
  return `${route.onHub ? "hub" : "local"}:${route.printerId}`;
}

function parseRoute(value: string): Api.PrintRoute | null {
  if (!value) return null;
  const [where, ...rest] = value.split(":");
  return { printerId: rest.join(":"), onHub: where === "hub" };
}

function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Settings → Printers: this PC's printers, what prints where, the drawer and the hub's queue. */
export function PrintersSection() {
  const canManage = useCan("printing.manage");
  const setup = useCommand("printers_config_get", {});
  const connection = useConnection();
  const isHub = connection.data?.runningMode === "hub";
  const [draft, setDraft] = useState<Api.PrinterConfig | null>(null);
  const [editing, setEditing] = useState<{ index: number; printer: Api.PrinterProfile } | null>(
    null,
  );
  const save = useAction("printers_config_save", {
    invalidates: ["printers_config_get"],
    success: "Printer settings saved",
    onSuccess: () => setDraft(null),
  });
  const saved = setup.data?.config;
  const config = draft ?? saved;
  const dirty = draft !== null && saved !== undefined && !same(draft, saved);
  const change = (patch: Partial<Api.PrinterConfig>) => {
    if (config) setDraft({ ...config, ...patch });
  };
  const printers = config?.printers ?? [];

  const openDrawer = async () => {
    try {
      await call("drawer_open", { reason: "No sale (from Settings)" });
      toast.success("Drawer opened");
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <>
      <Section
        title="Printers on this PC"
        actions={
          canManage && (
            <Button
              size="sm"
              onClick={() => setEditing({ index: -1, printer: newPrinter() })}
              disabled={!config}
            >
              <Plus /> Add printer
            </Button>
          )
        }
      >
        <QueryView query={setup}>
          {() =>
            printers.length === 0 ? (
              <EmptyState
                title="No printers yet"
                description="Receipts and labels print through the system print dialog until you add a printer here."
              />
            ) : (
              <ul className="flex flex-col divide-y divide-border">
                {printers.map((p, index) => (
                  <li
                    key={p.id || `new-${index}`}
                    className="flex flex-wrap items-center gap-3 py-2.5"
                  >
                    <div className="flex min-w-0 flex-1 flex-col">
                      <span className="font-medium">{p.name}</span>
                      <span className="text-xs text-muted-foreground">
                        {describeConnection(p.connection)}
                        {p.kind === "receipt"
                          ? ` · ${p.paper === "mm58" ? "58" : "80"} mm`
                          : ` · ${(p.labelLanguage ?? "zpl").toUpperCase()} ${p.labelDpi ?? 203} dpi`}
                      </span>
                    </div>
                    <span className="flex flex-wrap gap-1">
                      <Badge variant="secondary">
                        {p.kind === "label" ? "Labels" : "Receipts"}
                      </Badge>
                      {p.hasDrawer && <Badge variant="outline">Cash drawer</Badge>}
                      {p.shared && <Badge variant="outline">Shared with counters</Badge>}
                    </span>
                    {canManage && (
                      <span className="flex gap-1">
                        <TestPrintButton printer={p} />
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          aria-label={`Edit ${p.name}`}
                          onClick={() => setEditing({ index, printer: p })}
                        >
                          <Pencil />
                        </Button>
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          aria-label={`Remove ${p.name}`}
                          onClick={() =>
                            void confirm({
                              title: `Remove ${p.name}?`,
                              description: "Documents routed to it go back to the print dialog.",
                              confirmLabel: "Remove",
                              destructive: true,
                            }).then((ok) => {
                              if (ok === false || !config) return;
                              const routes = { ...config.routes };
                              for (const d of DOCS) {
                                const r = routes[d.key];
                                if (r && !r.onHub && r.printerId === p.id) routes[d.key] = null;
                              }
                              change({
                                printers: printers.filter((_, i) => i !== index),
                                routes,
                                ...(config.drawerPrinterId === p.id
                                  ? { drawerPrinterId: null, drawerOnCash: false }
                                  : {}),
                              });
                            })
                          }
                        >
                          <Trash2 />
                        </Button>
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            )
          }
        </QueryView>
      </Section>

      {config && (
        <Section title="What prints where">
          <div className="grid gap-4 sm:grid-cols-2">
            {DOCS.map((d) => {
              const local = printers
                .filter((p) => p.kind === d.kind && p.id)
                .map((p) => ({ value: `local:${p.id}`, label: p.name }));
              const hub = (setup.data?.hubPrinters ?? [])
                .filter((p) => p.kind === d.kind)
                .map((p) => ({ value: `hub:${p.id}`, label: `${p.name} (on the hub)` }));
              return (
                <Field key={d.key}>
                  <FieldLabel htmlFor={`route-${d.key}`}>{d.title}</FieldLabel>
                  <SelectField
                    id={`route-${d.key}`}
                    value={routeValue(config.routes?.[d.key])}
                    disabled={!canManage}
                    onValueChange={(v) =>
                      change({ routes: { ...config.routes, [d.key]: parseRoute(v) } })
                    }
                    options={[{ value: "", label: "System print dialog" }, ...local, ...hub]}
                  />
                  <FieldDescription>{d.help}</FieldDescription>
                </Field>
              );
            })}
          </div>
          <div className="mt-4 flex flex-col gap-3 border-t border-border pt-4 text-sm">
            <label className="flex items-start gap-2">
              <Checkbox
                checked={config.autoPrintReceipt ?? false}
                disabled={!canManage || !config.routes?.receipt}
                onCheckedChange={(v) => change({ autoPrintReceipt: v })}
              />
              <span>
                Print the receipt as soon as a sale is saved
                <span className="block text-xs text-muted-foreground">
                  Needs a receipt printer above. If the printer fails the sale is still saved and
                  you can reprint.
                </span>
              </span>
            </label>
            <div className="grid gap-3 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor="drawer-printer">Cash drawer</FieldLabel>
                <SelectField
                  id="drawer-printer"
                  value={config.drawerPrinterId ?? ""}
                  disabled={!canManage}
                  onValueChange={(v) =>
                    change({
                      drawerPrinterId: v || null,
                      ...(v ? {} : { drawerOnCash: false }),
                    })
                  }
                  options={[
                    { value: "", label: "No cash drawer" },
                    ...printers
                      .filter((p) => p.kind === "receipt" && p.hasDrawer && p.id)
                      .map((p) => ({ value: p.id ?? "", label: `On ${p.name}` })),
                  ]}
                />
                <FieldDescription>
                  Only printers marked "cash drawer attached" are listed.
                </FieldDescription>
              </Field>
              <div className="flex flex-col justify-end gap-2">
                <label className="flex items-start gap-2">
                  <Checkbox
                    checked={config.drawerOnCash ?? false}
                    disabled={!canManage || !config.drawerPrinterId}
                    onCheckedChange={(v) => change({ drawerOnCash: v })}
                  />
                  <span>Open the drawer for cash sales</span>
                </label>
                {saved?.drawerPrinterId && (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="self-start"
                    onClick={() => void openDrawer()}
                  >
                    <Vault /> Open drawer (no sale)
                  </Button>
                )}
              </div>
            </div>
            <Field>
              <FieldLabel htmlFor="font-path">Font for ₹ and Indian scripts</FieldLabel>
              <Input
                id="font-path"
                value={config.fontPath ?? ""}
                disabled={!canManage}
                placeholder={
                  setup.data?.pictureFont
                    ? "Using the system font (Nirmala UI)"
                    : "C:\\Fonts\\NotoSans.ttf"
                }
                onChange={(e) => change({ fontPath: e.target.value || null })}
              />
              <FieldDescription>
                {setup.data?.pictureFont
                  ? "Found on this PC: the rupee sign, Hindi and Malayalam print as pictures."
                  : "No suitable font was found: give a font file, or set printers to print Rs."}
              </FieldDescription>
            </Field>
          </div>
        </Section>
      )}

      {draft && (
        <SaveBar
          dirty={dirty}
          saving={save.isPending}
          onDiscard={() => setDraft(null)}
          onSave={() => save.mutate({ input: draft })}
        />
      )}

      <PrinterDialog
        printer={editing?.printer ?? null}
        isNew={(editing?.index ?? -1) < 0}
        showSharing={isHub}
        onClose={() => setEditing(null)}
        onSave={(p) => {
          if (!editing || !config) return;
          const next =
            editing.index < 0
              ? [...printers, p]
              : printers.map((old, i) => (i === editing.index ? p : old));
          change({ printers: next });
          setEditing(null);
        }}
      />

      <PrintQueueSection />
    </>
  );
}

const STATUS_TONE: Record<string, "secondary" | "outline" | "destructive"> = {
  printed: "secondary",
  queued: "outline",
  failed: "destructive",
  cancelled: "outline",
};

/** The hub's print queue: jobs counters sent to shared printers (own jobs for cashiers). */
export function PrintQueueSection() {
  const connection = useConnection();
  const mode = connection.data?.runningMode;
  const [status, setStatus] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const show = mode === "hub" || mode === "client";
  const jobs = useList(
    "print_jobs_list",
    { page, pageSize, status: status || null },
    { enabled: show, refetchInterval: 10_000 },
  );
  const retry = useAction("print_jobs_retry", { invalidates: ["print_jobs_list"] });
  const cancel = useAction("print_jobs_cancel", { invalidates: ["print_jobs_list"] });
  if (!show) return null;
  const columns: ColumnDef<Api.PrintJob>[] = [
    {
      id: "created",
      header: "Sent",
      cell: ({ row }) => showDateTime(row.original.createdAt),
    },
    { id: "title", header: "Document", cell: ({ row }) => row.original.title },
    {
      id: "printer",
      header: "Printer",
      cell: ({ row }) => (
        <span className="flex flex-col">
          {row.original.printerName}
          <span className="text-xs text-muted-foreground">
            {row.original.deviceName ?? "Hub"} · {row.original.createdByName ?? ""}
          </span>
        </span>
      ),
    },
    {
      id: "status",
      header: "Status",
      cell: ({ row }) => (
        <span className="flex flex-col gap-0.5">
          <Badge variant={STATUS_TONE[row.original.status] ?? "outline"}>
            {label(row.original.status)}
          </Badge>
          {row.original.lastError && row.original.status === "failed" && (
            <span className="max-w-xs text-xs text-destructive">{row.original.lastError}</span>
          )}
        </span>
      ),
    },
    {
      id: "actions",
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) =>
        ["queued", "failed"].includes(row.original.status) ? (
          <span className="flex justify-end gap-1">
            <Button
              size="sm"
              variant="outline"
              onClick={() => retry.mutate({ input: { id: row.original.id } })}
            >
              <RotateCcw /> Retry
            </Button>
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label="Cancel this job"
              onClick={() => cancel.mutate({ input: { id: row.original.id } })}
            >
              <X />
            </Button>
          </span>
        ) : null,
    },
  ];
  return (
    <Section
      title="Print queue on the hub"
      actions={
        <SelectField
          size="sm"
          aria-label="Show"
          className="w-40"
          value={status}
          onValueChange={(v) => {
            setStatus(v);
            setPage(1);
          }}
          options={[
            { value: "", label: "All jobs" },
            { value: "failed", label: "Failed" },
            { value: "queued", label: "Waiting" },
            { value: "printed", label: "Printed" },
            { value: "cancelled", label: "Cancelled" },
          ]}
        />
      }
    >
      <QueryView query={jobs}>
        {(data) =>
          data.total === 0 ? (
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <Inbox className="size-4" aria-hidden /> Nothing in the queue.
            </p>
          ) : (
            <DataTable
              columns={columns}
              data={data.items}
              rowKey={(j) => j.id}
              paging={{
                page: data.page,
                pageSize: data.pageSize,
                total: data.total,
                onPage: setPage,
                onPageSize: (n: number) => {
                  setPageSize(n);
                  setPage(1);
                },
              }}
            />
          )
        }
      </QueryView>
    </Section>
  );
}
