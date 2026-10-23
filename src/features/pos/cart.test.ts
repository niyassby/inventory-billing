import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import {
  EMPTY_CART,
  addItem,
  fromHeld,
  itemCount,
  previewInput,
  removeLine,
  saleLines,
  select,
  setBillDiscount,
  setLineDiscount,
  setLineBatch,
  setLinePrice,
  setParty,
  setQty,
  settle,
  stepQty,
  toHeld,
  quickTenders,
} from "./cart";

function item(id: string, price = 10_000, unit = "pcs"): Api.SearchItem {
  return {
    variantId: id,
    productId: `p-${id}`,
    name: `Item ${id}`,
    sku: id.toUpperCase(),
    barcode: null,
    unitId: unit,
    unitCode: unit,
    factorNum: 1,
    factorDen: 1,
    salePrice: price,
    mrp: null,
    taxRateId: null,
    taxRate: 0,
    taxType: null,
    cessRate: 0,
    hsnSac: null,
    stock: 5000,
    isService: false,
    trackBatches: false,
  };
}

describe("cart", () => {
  it("merges repeated scans of the same item and unit", () => {
    let cart = addItem(EMPTY_CART, item("a"));
    cart = addItem(cart, item("b"));
    cart = addItem(cart, item("a"));
    expect(cart.lines.map((l) => [l.item.variantId, l.qty])).toEqual([
      ["a", 2000],
      ["b", 1000],
    ]);
    expect(cart.selected).toBe(0);
    expect(itemCount(cart)).toBe(3000);
    // Another unit of the same variant is its own line.
    cart = addItem(cart, item("a", 50_000, "box"));
    expect(cart.lines).toHaveLength(3);
  });

  it("keeps a changed line separate from new scans", () => {
    let cart = addItem(EMPTY_CART, item("a"));
    cart = setLineDiscount(cart, 0, { kind: "percent", rate: 100_000 });
    cart = addItem(cart, item("a"));
    expect(cart.lines).toHaveLength(2);
    expect(new Set(cart.lines.map((l) => l.key)).size).toBe(2);
  });

  it("changes quantities and removes lines at zero", () => {
    let cart = addItem(addItem(EMPTY_CART, item("a")), item("b"));
    cart = stepQty(cart, 0, 2);
    expect(cart.lines[0]?.qty).toBe(3000);
    cart = setQty(cart, 1, 0);
    expect(cart.lines).toHaveLength(1);
    cart = stepQty(cart, 0, -3);
    expect(cart.lines).toHaveLength(0);
    expect(stepQty(cart, 4, 1)).toBe(cart);
    expect(removeLine(cart, 9)).toBe(cart);
  });

  it("keeps the selection inside the list", () => {
    let cart = addItem(addItem(addItem(EMPTY_CART, item("a")), item("b")), item("c"));
    cart = select(cart, 10);
    expect(cart.selected).toBe(2);
    cart = removeLine(cart, 2);
    expect(cart.selected).toBe(1);
    expect(select(EMPTY_CART, 3).selected).toBe(0);
  });

  it("drops empty discounts and catalogue prices", () => {
    let cart = addItem(EMPTY_CART, item("a"));
    cart = setLineDiscount(cart, 0, { kind: "amount", amount: 0 });
    cart = setLinePrice(cart, 0, 10_000);
    expect(cart.lines[0]).toMatchObject({ discount: null, unitPrice: null });
    cart = setLinePrice(cart, 0, 9_000);
    cart = setBillDiscount(cart, { kind: "percent", rate: 0 });
    expect(cart.billDiscount).toBeNull();
    cart = setBillDiscount(cart, { kind: "amount", amount: 500 });
    cart = setParty(cart, { id: "c1", name: "Asha" });
    expect(saleLines(cart)).toEqual([
      { variantId: "a", unitId: "pcs", qty: 1000, unitPrice: 9_000, discount: null, batchId: null },
    ]);
    expect(previewInput(cart)).toMatchObject({
      partyId: "c1",
      billDiscount: { kind: "amount", amount: 500 },
    });
  });

  it("judges a price change against the price list's price", () => {
    let cart = addItem(EMPTY_CART, item("a", 10_000));
    // The customer's list charges 9,000: saying 9,000 is no change at all.
    cart = setLinePrice(cart, 0, 9_000, 9_000);
    expect(cart.lines[0]?.unitPrice).toBeNull();
    cart = setLinePrice(cart, 0, 10_000, 9_000);
    expect(cart.lines[0]?.unitPrice).toBe(10_000);
    expect(previewInput(cart, "session-1").registerSessionId).toBe("session-1");
    expect(previewInput(cart).registerSessionId).toBeNull();
  });

  it("sells a line from a chosen batch and keeps further scans apart", () => {
    let cart = addItem(EMPTY_CART, item("a"));
    cart = setLineBatch(cart, 0, "b-1", "B24");
    expect(saleLines(cart)[0]?.batchId).toBe("b-1");
    cart = addItem(cart, item("a"));
    expect(cart.lines).toHaveLength(2);
    expect(saleLines(cart)[1]?.batchId).toBeNull();
    cart = setLineBatch(cart, 0, null, "B24");
    expect(cart.lines[0]).toMatchObject({ batchId: null, batchNo: null });
    const held = setLineBatch(cart, 1, "b-2", "B25");
    const back = fromHeld(JSON.parse(JSON.stringify(toHeld(held))));
    expect(back.lines[1]).toMatchObject({ batchId: "b-2", batchNo: "B25" });
  });

  it("round-trips a held bill", () => {
    let cart = addItem(addItem(EMPTY_CART, item("a")), item("b"));
    cart = setLineDiscount(cart, 1, { kind: "amount", amount: 100 });
    cart = setLinePrice(cart, 0, 8_000);
    cart = setParty(setBillDiscount(cart, { kind: "percent", rate: 50_000 }), {
      id: "c",
      name: "C",
    });
    const back = fromHeld(JSON.parse(JSON.stringify(toHeld({ ...cart, notes: "deliver" }))));
    expect(saleLines(back)).toEqual(saleLines(cart));
    expect(back).toMatchObject({
      party: { id: "c" },
      notes: "deliver",
      billDiscount: { rate: 50_000 },
    });
    expect(fromHeld({ format: "other" })).toEqual(EMPTY_CART);
    expect(fromHeld(null)).toEqual(EMPTY_CART);
    expect(fromHeld({ format: "pos-cart-v1", lines: [null, { item: 1 }] }).lines).toEqual([]);
  });
});

describe("settle", () => {
  it("gives change for cash above the total", () => {
    expect(settle(11_800, [{ mode: "cash", amount: 20_000 }])).toEqual({
      payments: [{ mode: "cash", amount: 11_800, tendered: 20_000 }],
      paid: 11_800,
      change: 8_200,
      due: 0,
    });
  });

  it("applies card first and cash for the rest in a split payment", () => {
    const s = settle(10_000, [
      { mode: "cash", amount: 5_000 },
      { mode: "card", amount: 6_000 },
    ]);
    expect(s.payments).toEqual([
      { mode: "card", amount: 6_000, tendered: null },
      { mode: "cash", amount: 4_000, tendered: 5_000 },
    ]);
    expect(s.due).toBe(0);
    expect(s.change).toBe(1_000);
  });

  it("leaves the rest due and ignores empty tenders", () => {
    expect(
      settle(10_000, [
        { mode: "upi", amount: 3_000 },
        { mode: "cash", amount: 0 },
      ]),
    ).toMatchObject({
      paid: 3_000,
      due: 7_000,
      change: 0,
    });
    expect(settle(10_000, [{ mode: "card", amount: 12_000 }]).payments).toEqual([
      { mode: "card", amount: 10_000, tendered: null },
    ]);
  });
});

describe("quickTenders", () => {
  it("offers the exact amount, then round amounts above it", () => {
    expect(quickTenders(11_800, 100)).toEqual([11_800, 15_000, 20_000, 50_000]);
    expect(quickTenders(10_000, 100)).toEqual([10_000, 20_000, 50_000, 200_000]);
  });

  it("offers nothing when there is nothing to pay", () => {
    expect(quickTenders(0, 100)).toEqual([]);
    expect(quickTenders(-5, 100)).toEqual([]);
  });

  it("works for currencies without minor units", () => {
    expect(quickTenders(430, 1)).toEqual([430, 450, 500, 600]);
  });
});
