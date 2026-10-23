/**
 * Which price a price list gives for a line. Mirrored by Rust `domain::price_lists`; both pass
 * `test-vectors/price-lists.json`, so the till, the hub and an offline counter agree.
 *
 * Rules:
 * 1. Only prices valid on the bill's date count (`validFrom` / `validTo`, inclusive, either open).
 * 2. A price for the sold unit (a pack) wins over a base-unit price; a base-unit price is scaled to
 *    the pack (1 pack = factorNum / factorDen base units, rounded half up).
 * 3. Quantity slabs: the price with the highest `minQty` the line reaches (in the price's unit).
 * 4. Ties: the most recent `validFrom`, then the first entry.
 */
import { divRound } from "./rounding";

export interface ListPriceEntry {
  /** The unit the price is for; null = the product's base unit. */
  unitId: string | null;
  /** Smallest quantity (thousandths of the price's unit) the price applies from. */
  minQty: number;
  /** Price per unit (minor units). */
  price: number;
  validFrom: string | null;
  validTo: string | null;
}

export interface ListPriceQuery {
  entries: ListPriceEntry[];
  soldUnitId: string;
  baseUnitId: string;
  /** 1 sold unit = factorNum / factorDen base units. */
  factorNum: number;
  factorDen: number;
  /** Thousandths of the sold unit. */
  qty: number;
  /** YYYY-MM-DD */
  date: string;
}

export interface ListPriceResult {
  /** Price per sold unit; null when the list has none for this line (use the product's price). */
  price: number | null;
  /** Index of the entry used. */
  entryIndex: number | null;
}

function scale(value: number, num: number, den: number): number {
  return Number(divRound(BigInt(value) * BigInt(num), BigInt(den), "half_up"));
}

export function resolveListPrice(q: ListPriceQuery): ListPriceResult {
  const valid = q.entries
    .map((entry, index) => ({ entry, index }))
    .filter(
      ({ entry }) =>
        (entry.validFrom === null || entry.validFrom <= q.date) &&
        (entry.validTo === null || entry.validTo >= q.date),
    );
  const soldIsBase = q.soldUnitId === q.baseUnitId;
  const isBase = (e: ListPriceEntry) => e.unitId === null || e.unitId === q.baseUnitId;
  const exact = valid.filter(({ entry }) =>
    soldIsBase ? isBase(entry) : entry.unitId === q.soldUnitId,
  );
  const useBase = !soldIsBase && exact.length === 0;
  const candidates = useBase ? valid.filter(({ entry }) => isBase(entry)) : exact;
  const slabQty = useBase ? scale(q.qty, q.factorNum, q.factorDen) : q.qty;
  let best: { entry: ListPriceEntry; index: number } | null = null;
  for (const c of candidates) {
    if (c.entry.minQty > slabQty) continue;
    if (
      best === null ||
      c.entry.minQty > best.entry.minQty ||
      (c.entry.minQty === best.entry.minQty &&
        (c.entry.validFrom ?? "") > (best.entry.validFrom ?? ""))
    ) {
      best = c;
    }
  }
  if (best === null) return { price: null, entryIndex: null };
  return {
    price: useBase ? scale(best.entry.price, q.factorNum, q.factorDen) : best.entry.price,
    entryIndex: best.index,
  };
}
