import { useState } from "react";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { Ban, DatabaseBackup, Wifi } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { confirm } from "@/components/app/dialogs";
import { QueryView, Section } from "@/components/app/page";
import { call } from "@/api/v2";
import { describeConnection } from "@/api/connection";
import { useConnection } from "@/hooks/use-connection";
import { useMoney } from "@/lib/business";
import { showDateTime, label } from "@/lib/format";
import { errorText, useAction, useCommand } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";
import { useQueryClient } from "@tanstack/react-query";

// ------------------------------------------------------------------------------------- backups

function bytes(n: number | null | undefined): string {
  if (!n) return "";
  return n > 1_048_576
    ? `${(n / 1_048_576).toFixed(1)} MB`
    : `${Math.max(1, Math.round(n / 1024))} KB`;
}

export function BackupSection() {
  const canManage = useCan("backup.manage");
  const status = useCommand("backup_status", {}, { enabled: canManage });
  const backupNow = useAction("backup_now", {
    invalidates: ["backup_status"],
    success: "Backup made",
  });
  const [passphrase, setPassphrase] = useState("");
  const setKey = useAction("backup_set_passphrase", {
    invalidates: ["backup_status"],
    success:
      "Backup passphrase set. Write it down: without it an encrypted backup can't be restored on another PC.",
    onSuccess: () => setPassphrase(""),
  });
  const [path, setPath] = useState("");
  const [restorePass, setRestorePass] = useState("");
  const [checked, setChecked] = useState<Api.VerifiedBackup | null>(null);
  const [restoring, setRestoring] = useState(false);
  const queryClient = useQueryClient();
  if (!canManage) return null;
  const input = { path: path.trim(), passphrase: restorePass || null };
  const verify = async () => {
    try {
      setChecked(await call("backup_verify", input));
    } catch (e) {
      setChecked(null);
      toast.error(errorText(e));
    }
  };
  const restore = async () => {
    const ok = await confirm({
      title: "Restore this backup?",
      description:
        "Everything made after the backup is lost. A safety copy of the current data is made first, and everyone is signed out.",
      confirmLabel: "Restore",
      destructive: true,
    });
    if (ok === false) return;
    setRestoring(true);
    try {
      await call("backup_restore", input);
      toast.success("Restored. Please sign in again.");
      queryClient.clear();
      window.location.assign("/");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setRestoring(false);
    }
  };
  return (
    <>
      <Section
        title="Backups"
        actions={
          <Button
            size="sm"
            onClick={() => backupNow.mutate({ input: {} })}
            disabled={backupNow.isPending}
          >
            <DatabaseBackup /> {backupNow.isPending ? "Backing up…" : "Back up now"}
          </Button>
        }
      >
        <QueryView query={status}>
          {(s) => (
            <div className="flex flex-col gap-3 text-sm">
              <p>
                Folder: <code className="text-xs">{s.directory}</code>
              </p>
              <p>
                {s.encrypted ? (
                  <Badge variant="secondary">Encrypted</Badge>
                ) : (
                  <Badge variant="destructive">Not encrypted: set a passphrase below</Badge>
                )}{" "}
                {s.lastSuccess
                  ? `Last good backup ${showDateTime(s.lastSuccess.finishedAt)}`
                  : "No backup has been made yet."}
              </p>
              <table className="w-full">
                <tbody>
                  {s.runs.slice(0, 8).map((r) => (
                    <tr key={r.id} className="border-t border-border/40">
                      <td className="py-1.5">{showDateTime(r.startedAt)}</td>
                      <td>{label(r.kind)}</td>
                      <td>
                        {r.status === "ok" ? (
                          <span>
                            {r.fileName} · {bytes(r.sizeBytes)}
                          </span>
                        ) : (
                          <span className="text-destructive">{r.error}</span>
                        )}
                      </td>
                      <td className="text-right">
                        {r.status === "ok" ? (
                          r.fileExists ? (
                            ""
                          ) : (
                            <span className="text-warning">file removed</span>
                          )
                        ) : (
                          <Badge variant="destructive">Failed</Badge>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </QueryView>
      </Section>
      <Section title="Backup passphrase">
        <form
          className="flex max-w-lg flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            setKey.mutate({ input: { passphrase } });
          }}
        >
          <Field className="min-w-64 flex-1">
            <FieldLabel htmlFor="bk-pass">
              {status.data?.encrypted ? "Change the passphrase" : "Set a passphrase"}
            </FieldLabel>
            <Input
              id="bk-pass"
              type="password"
              autoComplete="new-password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
            />
            <FieldDescription>
              At least 10 characters. Older backups keep the passphrase they were made with.
            </FieldDescription>
          </Field>
          <Button type="submit" disabled={passphrase.length < 10 || setKey.isPending}>
            Save
          </Button>
        </form>
      </Section>
      <Section title="Restore a backup">
        <div className="flex max-w-2xl flex-col gap-3">
          <Field>
            <FieldLabel htmlFor="rs-path">Backup file (full path to a .invbak file)</FieldLabel>
            <Input
              id="rs-path"
              placeholder="D:\Backups\inventory-20261019-210000-scheduled.invbak"
              value={path}
              onChange={(e) => {
                setPath(e.target.value);
                setChecked(null);
              }}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="rs-pass">Passphrase (if it was made with another one)</FieldLabel>
            <Input
              id="rs-pass"
              type="password"
              autoComplete="off"
              value={restorePass}
              onChange={(e) => setRestorePass(e.target.value)}
            />
          </Field>
          <div className="flex gap-2">
            <Button variant="outline" disabled={!path.trim()} onClick={run(verify)}>
              Check the file
            </Button>
            <Button variant="destructive" disabled={!checked || restoring} onClick={run(restore)}>
              {restoring ? "Restoring…" : "Restore"}
            </Button>
          </div>
          {checked && (
            <p className="rounded-xl bg-muted/50 p-3 text-sm">
              Made {showDateTime(checked.header.createdAt)}: {checked.products} products,{" "}
              {checked.invoices} invoices
              {checked.lastInvoiceDate ? `, last bill ${checked.lastInvoiceDate}` : ""}. The file is
              intact.
            </p>
          )}
        </div>
      </Section>
    </>
  );
}

// ------------------------------------------------------------------------------------- devices

function PairCounterDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [url, setUrl] = useState("");
  const [probe, setProbe] = useState<Api.ProbeResult | null>(null);
  const [code, setCode] = useState("");
  const [name, setName] = useState("Counter");
  const [busy, setBusy] = useState(false);
  const found = useCommand("device_discover_hubs", {}, { enabled: open, retry: false });
  const queryClient = useQueryClient();
  const check = async (address: string) => {
    setBusy(true);
    try {
      setProbe(await call("device_probe_hub", { url: address }));
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const pair = async () => {
    if (!probe) return;
    setBusy(true);
    try {
      await call("device_pair", {
        url: probe.url,
        code,
        fingerprint: probe.fingerprint,
        deviceName: name,
      });
      toast.success("Paired. Restart the app to work as a counter.");
      void queryClient.invalidateQueries({ queryKey: ["device-status"] });
      onClose();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Connect to a hub</DialogTitle>
          <DialogDescription>
            Make this PC a counter that works through the shop's main PC.
          </DialogDescription>
        </DialogHeader>
        {!probe ? (
          <div className="flex flex-col gap-3">
            {found.data && found.data.length > 0 && (
              <ul className="text-sm">
                {found.data.map((h) => (
                  <li key={h.name}>
                    <button
                      type="button"
                      className="w-full rounded-lg border border-border/60 px-3 py-2 text-left hover:bg-muted"
                      onClick={run(() => check(h.urls[0] ?? ""))}
                    >
                      <Wifi className="mr-2 inline size-4" aria-hidden />
                      {h.name} <span className="text-xs text-muted-foreground">{h.urls[0]}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <Field>
              <FieldLabel htmlFor="hub-url">Hub address</FieldLabel>
              <Input
                id="hub-url"
                placeholder="192.168.1.20"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
              />
            </Field>
            <Button disabled={!url.trim() || busy} onClick={run(() => check(url))}>
              {busy ? "Looking…" : "Find the hub"}
            </Button>
          </div>
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              void pair();
            }}
          >
            <p className="rounded-xl bg-muted/50 p-3 text-sm">
              Found <strong>{probe.businessName}</strong>. Its code is{" "}
              <strong className="tracking-wide">{probe.fingerprintShort}</strong>. It must match the
              code shown on the hub's Network settings: if it doesn't, stop.
            </p>
            <Field>
              <FieldLabel htmlFor="pair-code">6-digit pairing code from the hub</FieldLabel>
              <Input
                id="pair-code"
                inputMode="numeric"
                maxLength={6}
                autoFocus
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="pair-name">Name of this counter</FieldLabel>
              <Input id="pair-name" value={name} onChange={(e) => setName(e.target.value)} />
            </Field>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setProbe(null)}>
                Back
              </Button>
              <Button type="submit" disabled={code.length !== 6 || !name.trim() || busy}>
                Pair
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

function PairingCodeButton() {
  const [code, setCode] = useState<Api.PairingCode | null>(null);
  const make = useAction("devices_pairing_code", { onSuccess: (c) => setCode(c) });
  return (
    <>
      <Button size="sm" onClick={() => make.mutate({ input: {} })} disabled={make.isPending}>
        Pair a counter
      </Button>
      <Dialog open={code !== null} onOpenChange={(o) => !o && setCode(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Pairing code</DialogTitle>
            <DialogDescription>
              Type this on the new counter within 5 minutes. It works once.
            </DialogDescription>
          </DialogHeader>
          <p className="text-center text-5xl font-semibold tracking-[0.3em] tabular-nums">
            {code?.code}
          </p>
          <DialogFooter>
            <Button onClick={() => setCode(null)}>Done</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

export function DevicesSection() {
  const connection = useConnection();
  const canManage = useCan("devices.manage");
  const canReview = useCan("sync.review");
  const [pairing, setPairing] = useState(false);
  const queryClient = useQueryClient();
  const status = connection.data;
  const isHub = status?.runningMode === "hub";
  const hubInfo = useCommand("hub_status", {}, { enabled: isHub && canManage });
  const devices = useCommand(
    "devices_list",
    {},
    { enabled: canManage && status?.runningMode !== "client" },
  );
  const exceptions = useCommand(
    "sync_exceptions",
    {},
    { enabled: canReview && status?.runningMode !== "client" },
  );
  const revoke = useAction("devices_revoke", {
    invalidates: ["devices_list"],
    success: "Counter removed",
  });
  const resolve = useAction("sync_resolve_exception", {
    invalidates: ["sync_exceptions", "devices_list"],
    success: "Marked as handled",
  });
  const setMode = useAction("device_set_mode", {
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["device-status"] });
      toast.info("Restart the app to apply the change.");
    },
  });
  if (!status) return null;
  const view = describeConnection(status);
  return (
    <>
      <Section title="This PC">
        <div className="flex flex-col gap-3 text-sm">
          <p>
            <strong>{view.label}</strong>: {view.detail}
          </p>
          {status.mode === "client" ? (
            <p className="text-muted-foreground">
              This is a counter. To change it, send or review its offline sales first, then switch
              it back here.
            </p>
          ) : null}
          <div className="flex flex-wrap gap-2">
            {status.mode !== "hub" && status.mode !== "client" && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setMode.mutate({ input: { mode: "hub" } })}
              >
                Make this PC the hub
              </Button>
            )}
            {status.mode !== "standalone" && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setMode.mutate({ input: { mode: "standalone" } })}
              >
                Work on this PC only
              </Button>
            )}
            {status.mode === "standalone" && (
              <Button variant="outline" size="sm" onClick={() => setPairing(true)}>
                Connect to a hub…
              </Button>
            )}
            {status.restartRequired && (
              <Button size="sm" onClick={() => void call("device_restart", {})}>
                Restart now
              </Button>
            )}
          </div>
        </div>
      </Section>
      {isHub && hubInfo.data && (
        <Section title="Hub" actions={canManage && <PairingCodeButton />}>
          <dl className="grid gap-1 text-sm sm:grid-cols-[10rem_1fr]">
            <dt className="text-muted-foreground">Addresses for counters</dt>
            <dd>{hubInfo.data.urls.join(", ") || "—"}</dd>
            <dt className="text-muted-foreground">Code to compare when pairing</dt>
            <dd className="font-semibold tracking-wide">{hubInfo.data.fingerprintShort}</dd>
            {hubInfo.data.error && (
              <>
                <dt className="text-muted-foreground">Problem</dt>
                <dd className="text-destructive">{hubInfo.data.error}</dd>
              </>
            )}
          </dl>
        </Section>
      )}
      {canManage && status.runningMode !== "client" && (
        <Section title="Paired counters">
          <QueryView query={devices}>
            {(list) =>
              list.length === 0 ? (
                <p className="text-sm text-muted-foreground">No counters are paired.</p>
              ) : (
                <table className="w-full text-sm">
                  <tbody>
                    {list.map((d) => (
                      <tr key={d.id} className="border-t border-border/40 first:border-0">
                        <td className="py-1.5 font-medium">
                          {d.name}{" "}
                          <span className="text-xs text-muted-foreground">{d.shortCode}</span>
                        </td>
                        <td className="text-muted-foreground">
                          {d.revokedAt
                            ? "removed"
                            : d.lastSeenAt
                              ? `seen ${showDateTime(d.lastSeenAt)}`
                              : "never seen"}
                        </td>
                        <td>
                          {d.openExceptions > 0 && (
                            <Badge variant="outline">{d.openExceptions} to review</Badge>
                          )}
                        </td>
                        <td className="text-right">
                          {!d.revokedAt && (
                            <Button
                              variant="ghost"
                              size="icon-sm"
                              aria-label={`Remove ${d.name}`}
                              onClick={run(async () => {
                                if (
                                  (await confirm({
                                    title: `Remove ${d.name}?`,
                                    description:
                                      "It stops working at once. Pair it again to use it.",
                                    confirmLabel: "Remove",
                                    destructive: true,
                                  })) !== false
                                )
                                  revoke.mutate({ input: { id: d.id } });
                              })}
                            >
                              <Ban />
                            </Button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )
            }
          </QueryView>
        </Section>
      )}
      {canReview && status.runningMode !== "client" && (
        <Section title="Offline sales to review">
          <QueryView query={exceptions}>
            {(page) => (
              <ExceptionList
                items={page.items}
                onResolve={(id, note) => resolve.mutate({ input: { id, note } })}
              />
            )}
          </QueryView>
        </Section>
      )}
      <PairCounterDialog open={pairing} onClose={() => setPairing(false)} />
    </>
  );
}

const KIND_TEXT: Record<string, string> = {
  offline_oversell: "Sold more than the hub had in stock",
  offline_price_changed: "Sold at a price that changed meanwhile",
  clock_skew: "The counter's clock was wrong",
};

function ExceptionList({
  items,
  onResolve,
}: {
  items: Api.SyncException[];
  onResolve: (id: string, note: string) => void;
}) {
  const money = useMoney();
  if (items.length === 0)
    return <p className="text-sm text-muted-foreground">Nothing to review.</p>;
  return (
    <ul className="text-sm">
      {items.map((x) => (
        <li
          key={x.id}
          className="flex items-center gap-3 border-t border-border/40 py-2 first:border-0"
        >
          <div className="flex-1">
            <p className="font-medium">
              {KIND_TEXT[x.kind] ?? x.kind}{" "}
              {x.docNo && <span className="text-muted-foreground">· {x.docNo}</span>}
            </p>
            <p className="text-xs text-muted-foreground">
              {x.deviceName} · {x.itemName ?? ""}{" "}
              {x.kind === "offline_price_changed"
                ? money(x.amount)
                : x.kind === "offline_oversell"
                  ? `${x.amount / 1000} short`
                  : `${Math.round(x.amount / 3600)} h`}{" "}
              · {showDateTime(x.createdAt)}
            </p>
          </div>
          <Button
            variant="outline"
            size="xs"
            onClick={run(async () => {
              const note = await confirm({
                title: "Mark as handled",
                description: "Say what was done about it.",
                confirmLabel: "Handled",
                reason: { label: "What was done", required: true },
              });
              if (note !== false) onResolve(x.id, note);
            })}
          >
            Handled
          </Button>
        </li>
      ))}
    </ul>
  );
}
