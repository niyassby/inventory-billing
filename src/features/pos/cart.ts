import type { Api } from "@inventory/shared";

/**
 * The POS bill being built. Pure data and functions (no React): the screen keeps it in a store,
 * the tests drive it directly. Prices, tax and totals are never computed here: the backend
 * prices the bill (`sales_preview`) and checks the total again when it is saved.
 */
export interface CartLine {
  /** Stable key for React and for merging repeated scans. */
  key: string;
  item: Api.SearchItem;
  /** Thousandths of the sold unit. */
  qty: number;
  discount: Api.Discount | null;
  /** A changed price (needs `sales.price_override` or a manager); null = catalogue price. */
  unitPrice: number | null;
}

export interface CartParty {
  id: string;
  name: string;
  phone?: string | null;
}

export interface Cart {
  lines: CartLine[];
  party: CartParty | null;
  billDiscount: Api.Discount | null;
  notes: string;
  /** Highlighted row (keyboard). */
  selected: number;
}

export const EMPTY_CART: Cart = {
  lines: [],
  party: null,
  billDiscount: null,
  notes: "",
  selected: 0,
};

const ONE = 1000;

function lineKey(item: Api.SearchItem): string {
  return `${item.variantId}:${item.unitId}`;
}

/** Adds an item; scanning the same item again adds to its line (unless that line was changed). */
export function addItem(cart: Cart, item: Api.SearchItem, qty = ONE): Cart {
  const key = lineKey(item);
  const index = cart.lines.findIndex(
    (l) => l.key === key && l.discount === null && l.unitPrice === null,
  );
  if (index >= 0) {
    const lines = cart.lines.map((l, i) => (i === index ? { ...l, qty: l.qty + qty } : l));
    return { ...cart, lines, selected: index };
  }
  const unique = cart.lines.some((l) => l.key === key) ? `${key}:${cart.lines.length}` : key;
  const lines = [...cart.lines, { key: unique, item, qty, discount: null, unitPrice: null }];
  return { ...cart, lines, selected: lines.length - 1 };
}

function update(cart: Cart, index: number, change: (l: CartLine) => CartLine): Cart {
  if (index < 0 || index >= cart.lines.length) return cart;
  return { ...cart, lines: cart.lines.map((l, i) => (i === index ? change(l) : l)) };
}

/** Sets a quantity; zero or less removes the line. */
export function setQty(cart: Cart, index: number, qty: number): Cart {
  if (qty <= 0) return removeLine(cart, index);
  return update(cart, index, (l) => ({ ...l, qty }));
}

/** +1 / −1 unit on the selected line (keyboard + and −). */
export function stepQty(cart: Cart, index: number, units: number): Cart {
  const line = cart.lines[index];
  if (!line) return cart;
  return setQty(cart, index, line.qty + units * ONE);
}

export function removeLine(cart: Cart, index: number): Cart {
  if (index < 0 || index >= cart.lines.length) return cart;
  const lines = cart.lines.filter((_, i) => i !== index);
  return { ...cart, lines, selected: Math.max(0, Math.min(cart.selected, lines.length - 1)) };
}

export function setLineDiscount(cart: Cart, index: number, discount: Api.Discount | null): Cart {
  return update(cart, index, (l) => ({ ...l, discount: normaliseDiscount(discount) }));
}

export function setLinePrice(cart: Cart, index: number, unitPrice: number | null): Cart {
  return update(cart, index, (l) => ({
    ...l,
    unitPrice: unitPrice === null || unitPrice === l.item.salePrice ? null : unitPrice,
  }));
}

export function setBillDiscount(cart: Cart, discount: Api.Discount | null): Cart {
  return { ...cart, billDiscount: normaliseDiscount(discount) };
}

export function setParty(cart: Cart, party: CartParty | null): Cart {
  return { ...cart, party };
}

export function select(cart: Cart, index: number): Cart {
  if (cart.lines.length === 0) return { ...cart, selected: 0 };
  return { ...cart, selected: Math.max(0, Math.min(index, cart.lines.length - 1)) };
}

function normaliseDiscount(d: Api.Discount | null): Api.Discount | null {
  if (!d) return null;
  if (d.kind === "amount") return d.amount > 0 ? d : null;
  return d.rate > 0 ? d : null;
}

export function itemCount(cart: Cart): number {
  return cart.lines.reduce((n, l) => n + l.qty, 0);
}

/** The bill's lines as the backend takes them. */
export function saleLines(cart: Cart): Api.SaleLineInput[] {
  return cart.lines.map((l) => ({
    variantId: l.item.variantId,
    unitId: l.item.unitId,
    qty: l.qty,
    unitPrice: l.unitPrice,
    discount: l.discount,
    batchId: null,
  }));
}

export function previewInput(cart: Cart): Api.PreviewInput {
  return {
    lines: saleLines(cart),
    billDiscount: cart.billDiscount,
    partyId: cart.party?.id ?? null,
  };
}

// ---------------------------------------------------------------------------------------------
// Payments

export interface Tender {
  mode: Api.PaymentInput["mode"];
  /** What the customer hands over (cash) or pays (card, UPI). */
  amount: number;
}

export interface Settlement {
  /** Applied to the bill. */
  payments: Api.PaymentInput[];
  paid: number;
  /** Cash given back. */
  change: number;
  /** Left on credit. */
  due: number;
}

/**
 * Splits what the customer handed over into payments applied to the bill. Non-cash payments are
 * applied first; cash covers the rest and any extra cash is change (never "paid"). Mirrors
 * `settleTender` in packages/shared (the backend checks the same rules).
 */
export function settle(total: number, tenders: Tender[]): Settlement {
  let remaining = total;
  const payments: Api.PaymentInput[] = [];
  const order = [
    ...tenders.filter((t) => t.mode !== "cash"),
    ...tenders.filter((t) => t.mode === "cash"),
  ];
  let change = 0;
  for (const t of order) {
    if (t.amount <= 0) continue;
    if (t.mode === "cash") {
      const applied = Math.min(t.amount, remaining);
      change += t.amount - applied;
      if (applied > 0) payments.push({ mode: "cash", amount: applied, tendered: t.amount });
      remaining -= applied;
    } else {
      const applied = Math.min(t.amount, remaining);
      if (applied > 0) payments.push({ mode: t.mode, amount: applied, tendered: null });
      remaining -= applied;
    }
  }
  const paid = total - remaining;
  return { payments, paid, change, due: remaining };
}

/** Note sizes (in major units) a customer is likely to hand over. */
const NOTE_SIZES = [50, 100, 200, 500, 2000];

/**
 * One-tap cash amounts for the payment dialog: the exact amount first, then the next few round
 * amounts above it ("₹118 due" → 118, 150, 200, 500). `scale` is the minor units per major unit.
 */
export function quickTenders(due: number, scale: number): number[] {
  if (due <= 0) return [];
  const out = [due];
  for (const size of NOTE_SIZES) {
    const step = size * scale;
    const up = Math.ceil(due / step) * step;
    if (up > due && !out.includes(up)) out.push(up);
  }
  return out.slice(0, 4);
}

// ---------------------------------------------------------------------------------------------
// Held bills (saved on the hub, recalled on any counter)

export const HELD_FORMAT = "pos-cart-v1";

export function toHeld(cart: Cart): Record<string, unknown> {
  return {
    format: HELD_FORMAT,
    lines: cart.lines.map((l) => ({
      item: l.item,
      qty: l.qty,
      discount: l.discount,
      unitPrice: l.unitPrice,
    })),
    party: cart.party,
    billDiscount: cart.billDiscount,
    notes: cart.notes,
  };
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** Restores a held bill; anything unreadable gives an empty cart rather than a broken one. */
export function fromHeld(payload: unknown): Cart {
  if (!isRecord(payload) || payload.format !== HELD_FORMAT || !Array.isArray(payload.lines)) {
    return EMPTY_CART;
  }
  let cart: Cart = { ...EMPTY_CART };
  for (const raw of payload.lines as unknown[]) {
    if (!isRecord(raw) || !isRecord(raw.item) || typeof raw.qty !== "number") continue;
    const item = raw.item as unknown as Api.SearchItem;
    cart = addItem(cart, item, raw.qty);
    const index = cart.lines.length - 1;
    cart = setLineDiscount(cart, index, (raw.discount as Api.Discount | null) ?? null);
    cart = setLinePrice(cart, index, typeof raw.unitPrice === "number" ? raw.unitPrice : null);
  }
  return {
    ...cart,
    party: isRecord(payload.party) ? (payload.party as unknown as CartParty) : null,
    billDiscount: (payload.billDiscount as Api.Discount | null) ?? null,
    notes: typeof payload.notes === "string" ? payload.notes : "",
    selected: 0,
  };
}
