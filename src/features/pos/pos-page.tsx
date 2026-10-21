import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { ArrowLeft, Keyboard, Pause, Percent, ScanBarcode, Trash2, User, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Avatar } from "@/components/ui/avatar";
import { Kbd } from "@/components/ui/kbd";
import { Textarea } from "@/components/ui/textarea";
import { ConnectionBadge } from "@/components/connection-badge";
import { ProductImage } from "@/components/app/product-image";
import { describeConnection } from "@/api/connection";
import { useConnection } from "@/hooks/use-connection";
import { confirm } from "@/components/app/dialogs";
import type { PickerHandle } from "@/components/app/pickers";
import { PartyPicker, ProductPicker, useDebounced } from "@/components/app/pickers";
import { call } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showQty } from "@/lib/format";
import { errorText, useCommand } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useSession } from "@/stores/session";
import { useQueryClient } from "@tanstack/react-query";
import { fromHeld, itemCount, previewInput, saleLines, toHeld } from "./cart";
import { PaymentDialog } from "./payment-dialog";
import {
  BillDiscountDialog,
  HeldBillsDialog,
  LineDialog,
  ReceiptDialog,
  TillDialog,
} from "./pos-dialogs";
import { usePos } from "./pos-store";
import { useShortcuts } from "./shortcut-store";
import { ShortcutsDialog } from "./shortcuts-dialog";
import { SHORTCUT_ACTIONS, formatBinding, matchesEvent } from "./shortcuts";

/** The key on a narrow button: long bindings ("Ctrl + Delete") stay in the hint bar and the tooltip. */
function ButtonKey({ binding }: { binding: string }) {
  const text = formatBinding(binding);
  return text.length > 5 ? null : <Kbd className="ml-1">{text}</Kbd>;
}

function useBillPreview() {
  const cart = usePos((s) => s.cart);
  const input = useDebounced(previewInput(cart), 120);
  return useCommand("sales_preview", input, {
    enabled: input.lines.length > 0,
    staleTime: 0,
    retry: false,
  });
}

/** The till: scan or search, adjust, take payment, print. Built for the keyboard. */
export default function PosPage() {
  const user = useSession((s) => s.user);
  const settings = usePublicSettings();
  const money = useMoney();
  const queryClient = useQueryClient();
  const pos = usePos();
  const { cart } = pos;
  const preview = useBillPreview();
  const connection = useConnection().data;
  const connectionView = connection ? describeConnection(connection) : null;
  const notice =
    connectionView && (connectionView.tone === "warning" || connectionView.tone === "error")
      ? connectionView
      : null;
  const register = useCommand("register_current", {});
  const search = useRef<PickerHandle>(null);
  const customer = useRef<PickerHandle>(null);

  const [editing, setEditing] = useState<number | null>(null);
  const [billDiscountOpen, setBillDiscountOpen] = useState(false);
  const [heldOpen, setHeldOpen] = useState(false);
  const [tillOpen, setTillOpen] = useState(false);
  const [paying, setPaying] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const { bindings } = useShortcuts();
  const [saving, setSaving] = useState(false);
  const [done, setDone] = useState<Api.InvoiceDetail | null>(null);
  const anyDialog =
    editing !== null ||
    billDiscountOpen ||
    heldOpen ||
    tillOpen ||
    paying ||
    shortcutsOpen ||
    done !== null;

  const total = cart.lines.length > 0 ? (preview.data?.total ?? null) : 0;
  const previewMatches = preview.data?.lines.length === cart.lines.length;

  const pay = useCallback(() => {
    if (cart.lines.length === 0) return;
    if (preview.error) {
      toast.error(errorText(preview.error));
      return;
    }
    if (total === null || !previewMatches) return;
    setPaying(true);
  }, [cart.lines.length, preview.error, total, previewMatches]);

  const hold = useCallback(async () => {
    if (cart.lines.length === 0) return;
    const label = cart.party?.name ?? `Bill ${new Date().toLocaleTimeString()}`;
    try {
      await call("sales_hold", { label, cart: toHeld(cart), partyId: cart.party?.id ?? null });
      void queryClient.invalidateQueries({ queryKey: ["sales_held_list"] });
      pos.reset();
      toast.success(`Held: ${label}`);
      search.current?.focus();
    } catch (e) {
      toast.error(errorText(e));
    }
  }, [cart, pos, queryClient]);

  const clear = useCallback(async () => {
    if (cart.lines.length === 0) return;
    if (await confirm({ title: "Clear this bill?", confirmLabel: "Clear", destructive: true })) {
      pos.reset();
      search.current?.focus();
    }
  }, [cart.lines.length, pos]);

  // Function keys work everywhere on this screen; arrows, +/− and Delete when the search is empty.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (anyDialog) return;
      const inSearch =
        (e.target as HTMLElement | null)?.getAttribute("aria-label") === "Find an item";
      const searchEmpty = inSearch && (e.target as HTMLInputElement).value === "";
      const inField =
        (e.target as HTMLElement | null)?.closest("input, textarea, select") && !searchEmpty;
      const handled = () => e.preventDefault();
      const action = SHORTCUT_ACTIONS.find((def) => matchesEvent(bindings[def.id], e))?.id;
      switch (action) {
        case "search":
          handled();
          search.current?.focus();
          return;
        case "customer":
          handled();
          customer.current?.focus();
          return;
        case "editLine":
          handled();
          if (cart.lines.length > 0) setEditing(cart.selected);
          return;
        case "discount":
          handled();
          setBillDiscountOpen(true);
          return;
        case "hold":
          handled();
          void hold();
          return;
        case "recall":
          handled();
          setHeldOpen(true);
          return;
        case "pay":
          handled();
          pay();
          return;
        case "clear":
          handled();
          void clear();
          return;
      }
      // Ctrl+Enter always takes payment, whatever the bindings are.
      if (e.ctrlKey && e.key === "Enter") {
        handled();
        pay();
        return;
      }
      if (inField) return;
      if (e.key === "ArrowDown") {
        handled();
        pos.select(cart.selected + 1);
      } else if (e.key === "ArrowUp") {
        handled();
        pos.select(cart.selected - 1);
      } else if (e.key === "+" || e.key === "=") {
        handled();
        pos.step(cart.selected, 1);
      } else if (e.key === "-") {
        handled();
        pos.step(cart.selected, -1);
      } else if (e.key === "Delete") {
        handled();
        pos.remove(cart.selected);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [anyDialog, bindings, cart.lines.length, cart.selected, pos, pay, hold, clear]);

  const complete = async (payments: Api.PaymentInput[]) => {
    if (total === null) return;
    setSaving(true);
    try {
      const invoice = await call("sales_create_invoice", {
        idempotencyKey: pos.saleKey,
        lines: saleLines(cart),
        billDiscount: cart.billDiscount,
        partyId: cart.party?.id ?? null,
        payments,
        expectedTotal: total,
        notes: cart.notes.trim() || null,
        registerSessionId: register.data?.id ?? null,
      });
      setPaying(false);
      setDone(invoice);
      pos.reset();
      void queryClient.invalidateQueries({ queryKey: ["sales_list_invoices"] });
      void queryClient.invalidateQueries({ queryKey: ["products_search"] });
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  const nextCustomer = () => {
    setDone(null);
    setTimeout(() => search.current?.focus(), 30);
  };

  useEffect(() => {
    if (!done) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "p" && !e.ctrlKey) {
        document.querySelector<HTMLButtonElement>('[data-print-receipt="true"]')?.click();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [done]);

  const editingLine = editing !== null ? (cart.lines[editing] ?? null) : null;
  const p = preview.data;

  return (
    <div className="flex h-screen flex-col bg-background font-sans">
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-card px-4">
        <Button
          variant="ghost"
          size="icon"
          render={<Link to="/" aria-label="Back to the dashboard" />}
          nativeButton={false}
        >
          <ArrowLeft />
        </Button>
        <div className="min-w-0">
          <p className="truncate text-sm font-semibold">{settings.data?.business.name}</p>
          <p className="truncate text-xs text-muted-foreground">{user?.name}</p>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <ConnectionBadge />
          <Button variant="outline" size="sm" onClick={() => setTillOpen(true)}>
            {register.data ? `Till open · ${register.data.registerName}` : "Open till"}
          </Button>
          <Button variant="outline" size="sm" onClick={() => setHeldOpen(true)}>
            Held bills <Kbd>{formatBinding(bindings.recall)}</Kbd>
          </Button>
        </div>
      </header>
      {notice && (
        <div className="px-4 pt-3">
          <Alert variant="warning">
            <AlertTitle>{notice.label}</AlertTitle>
            <AlertDescription>{notice.detail}</AlertDescription>
          </Alert>
        </div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 p-4 lg:grid-cols-[1fr_380px]">
        <section className="flex min-h-0 flex-col gap-3" aria-label="Bill">
          <ProductPicker
            ref={search}
            autoFocus
            className="[&_input]:h-12 [&_input]:text-base"
            onSelect={(item) => {
              pos.add(item);
            }}
            onNotFound={(term) => toast.error(`No item matches "${term}"`)}
          />
          <div className="min-h-0 flex-1 overflow-auto rounded-xl border border-border bg-card shadow-xs">
            {cart.lines.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center text-muted-foreground">
                <span className="mb-1 flex size-14 items-center justify-center rounded-full bg-primary-soft text-primary">
                  <ScanBarcode className="size-7" aria-hidden />
                </span>
                <p className="text-section-title text-foreground">
                  Scan or search an item to start
                </p>
                <p className="text-sm">
                  ↑ ↓ choose a line · + − change quantity · Delete removes it
                </p>
              </div>
            ) : (
              <table className="w-full text-sm">
                <thead className="sticky top-0 bg-muted/80 text-xs text-muted-foreground backdrop-blur-sm">
                  <tr className="border-b border-border">
                    <th className="w-8 px-3 py-2 text-left font-medium">#</th>
                    <th className="px-3 py-2 text-left font-medium">Item</th>
                    <th className="px-3 py-2 text-right font-medium">Qty</th>
                    <th className="px-3 py-2 text-right font-medium">Price</th>
                    <th className="px-3 py-2 text-right font-medium">Discount</th>
                    <th className="px-3 py-2 text-right font-medium">Amount</th>
                    <th className="w-10" />
                  </tr>
                </thead>
                <tbody>
                  {cart.lines.map((line, i) => {
                    const pl = previewMatches ? p?.lines[i] : undefined;
                    return (
                      <tr
                        key={line.key}
                        aria-selected={i === cart.selected}
                        onClick={() => pos.select(i)}
                        onDoubleClick={() => setEditing(i)}
                        className={cn(
                          "cursor-pointer border-b border-border/60 transition-colors hover:bg-muted/40",
                          i === cart.selected &&
                            "bg-primary-soft shadow-[inset_3px_0_0_var(--primary)] hover:bg-primary-soft",
                        )}
                      >
                        <td className="px-3 py-2 text-muted-foreground">{i + 1}</td>
                        <td className="px-3 py-2">
                          <div className="flex items-center gap-3">
                            <ProductImage
                              productId={line.item.productId}
                              imageKey={line.item.imageKey}
                              name={line.item.name}
                              size="md"
                            />
                            <div className="min-w-0">
                              <p className="font-medium">{line.item.name}</p>
                              <p className="text-xs text-muted-foreground">
                                {line.item.sku} · {line.item.unitCode}
                                {!line.item.isService && line.qty > line.item.stock && (
                                  <span className="ml-1 text-warning">
                                    · only {showQty(line.item.stock)} in stock
                                  </span>
                                )}
                              </p>
                            </div>
                          </div>
                        </td>
                        <td className="px-3 py-2.5 text-right num text-base font-semibold">
                          {showQty(line.qty)}
                        </td>
                        <td className="px-3 py-2.5 text-right num">
                          {money(line.unitPrice ?? line.item.salePrice)}
                          {line.unitPrice !== null && (
                            <span className="block text-xs text-warning">changed</span>
                          )}
                        </td>
                        <td className="px-3 py-2.5 text-right num">
                          {pl && pl.lineDiscount > 0 ? `−${money(pl.lineDiscount)}` : ""}
                        </td>
                        <td className="px-3 py-2.5 text-right num text-base font-semibold">
                          {pl ? money(pl.total) : "…"}
                        </td>
                        <td className="px-1">
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`Remove ${line.item.name}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              pos.remove(i);
                            }}
                          >
                            <X />
                          </Button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-caption">
            {SHORTCUT_ACTIONS.map((def) => (
              <span key={def.id} className="flex items-center gap-1">
                <Kbd>{formatBinding(bindings[def.id])}</Kbd> {def.hint}
              </span>
            ))}
            <button
              type="button"
              className="flex items-center gap-1 rounded text-primary hover:underline"
              onClick={() => setShortcutsOpen(true)}
            >
              <Keyboard className="size-3.5" aria-hidden /> Change shortcuts
            </button>
          </div>
        </section>

        <aside className="flex min-h-0 flex-col gap-3" aria-label="Customer and totals">
          <div className="rounded-xl border border-border bg-card p-3 shadow-xs">
            <div className="mb-2 flex items-center justify-between">
              <p className="flex items-center gap-1 text-xs font-medium text-muted-foreground">
                <User className="size-3.5" aria-hidden /> Customer{" "}
                <Kbd>{formatBinding(bindings.customer)}</Kbd>
              </p>
              {cart.party && (
                <Button variant="ghost" size="xs" onClick={() => pos.setParty(null)}>
                  Walk-in
                </Button>
              )}
            </div>
            {cart.party ? (
              <div className="flex items-center gap-2.5 rounded-lg bg-muted/60 p-2">
                <Avatar name={cart.party.name} />
                <div className="min-w-0">
                  <p className="truncate font-medium">{cart.party.name}</p>
                  {cart.party.phone && <p className="text-caption">{cart.party.phone}</p>}
                </div>
              </div>
            ) : (
              <PartyPicker
                ref={customer}
                partyType="customer"
                placeholder="Walk-in customer (search to choose)"
                onSelect={(party) => {
                  pos.setParty({ id: party.id, name: party.name, phone: party.phone ?? null });
                  search.current?.focus();
                }}
              />
            )}
          </div>

          <div className="flex flex-1 flex-col justify-end gap-2 rounded-xl border border-border bg-card p-4 shadow-xs">
            <dl className="grid grid-cols-2 gap-y-1 text-sm">
              <dt className="text-muted-foreground">Items</dt>
              <dd className="text-right num">{showQty(itemCount(cart))}</dd>
              <dt className="text-muted-foreground">Subtotal</dt>
              <dd className="text-right num">{p ? money(p.gross) : "—"}</dd>
              {p && p.discount > 0 && (
                <>
                  <dt className="text-muted-foreground">Discount</dt>
                  <dd className="text-right num text-success">−{money(p.discount)}</dd>
                </>
              )}
              <dt className="text-muted-foreground">
                Tax{p?.isInterState ? " (IGST)" : ""}
                {settings.data?.tax.pricesIncludeTax ? " (included)" : ""}
              </dt>
              <dd className="text-right num">{p ? money(p.tax) : "—"}</dd>
              {p && p.roundOff !== 0 && (
                <>
                  <dt className="text-muted-foreground">Round off</dt>
                  <dd className="text-right num">{money(p.roundOff)}</dd>
                </>
              )}
            </dl>
            <div className="mt-2 flex items-end justify-between border-t border-border pt-3">
              <span className="text-sm font-medium text-muted-foreground">Total to pay</span>
              <span className="text-display" aria-live="polite">
                {total === null ? "…" : money(total)}
              </span>
            </div>
            {preview.error && cart.lines.length > 0 && (
              <p className="text-sm text-destructive" role="alert">
                {errorText(preview.error)}
              </p>
            )}
            <Textarea
              aria-label="Note on the bill"
              placeholder="Note (optional)"
              className="min-h-10"
              value={cart.notes}
              onChange={(e) => pos.setNotes(e.target.value)}
            />
            <div className="grid grid-cols-3 gap-2">
              <Button
                variant="outline"
                title={formatBinding(bindings.discount)}
                onClick={() => setBillDiscountOpen(true)}
              >
                <Percent /> Discount <ButtonKey binding={bindings.discount} />
              </Button>
              <Button
                variant="outline"
                title={formatBinding(bindings.hold)}
                disabled={cart.lines.length === 0}
                onClick={() => void hold()}
              >
                <Pause /> Hold <ButtonKey binding={bindings.hold} />
              </Button>
              <Button
                variant="outline"
                title={formatBinding(bindings.clear)}
                disabled={cart.lines.length === 0}
                onClick={() => void clear()}
              >
                <Trash2 /> Clear <ButtonKey binding={bindings.clear} />
              </Button>
            </div>
            <Button
              size="xl"
              className="h-14 text-lg"
              disabled={cart.lines.length === 0 || total === null || !previewMatches}
              onClick={pay}
            >
              Pay {total !== null && cart.lines.length > 0 ? money(total) : ""}{" "}
              <Kbd className="ml-2">{formatBinding(bindings.pay)}</Kbd>
            </Button>
          </div>
        </aside>
      </div>

      <LineDialog
        line={editingLine}
        onClose={() => setEditing(null)}
        onSave={({ qty, unitPrice, discount }) => {
          if (editing === null) return;
          pos.setQty(editing, qty);
          pos.setLinePrice(editing, unitPrice);
          pos.setLineDiscount(editing, discount);
          setEditing(null);
        }}
      />
      <BillDiscountDialog
        open={billDiscountOpen}
        value={cart.billDiscount}
        onClose={() => setBillDiscountOpen(false)}
        onSave={(d) => {
          pos.setBillDiscount(d);
          setBillDiscountOpen(false);
        }}
      />
      <HeldBillsDialog
        open={heldOpen}
        onClose={() => setHeldOpen(false)}
        onRecall={(bill) => {
          pos.load(fromHeld(bill.cart));
          setHeldOpen(false);
        }}
      />
      <TillDialog open={tillOpen} session={register.data} onClose={() => setTillOpen(false)} />
      <PaymentDialog
        open={paying}
        total={total ?? 0}
        creditAllowed={cart.party !== null}
        busy={saving}
        onClose={() => setPaying(false)}
        onPay={(payments) => void complete(payments)}
      />
      <ReceiptDialog invoice={done} onClose={nextCustomer} />
      <ShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
    </div>
  );
}
