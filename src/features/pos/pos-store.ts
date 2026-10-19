import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { Api } from "@inventory/shared";
import type { Cart, CartParty } from "./cart";
import * as C from "./cart";

interface PosState {
  cart: Cart;
  /** Request id of the bill on screen: a retried "Pay" never makes a second invoice. */
  saleKey: string;
  add: (item: Api.SearchItem, qty?: number) => void;
  setQty: (index: number, qty: number) => void;
  step: (index: number, units: number) => void;
  remove: (index: number) => void;
  setLineDiscount: (index: number, d: Api.Discount | null) => void;
  setLinePrice: (index: number, price: number | null) => void;
  setBillDiscount: (d: Api.Discount | null) => void;
  setParty: (p: CartParty | null) => void;
  setNotes: (notes: string) => void;
  select: (index: number) => void;
  load: (cart: Cart) => void;
  /** After a sale (or "clear"): an empty bill with a new request id. */
  reset: () => void;
}

export const usePos = create<PosState>()(
  persist(
    (set) => ({
      cart: C.EMPTY_CART,
      saleKey: crypto.randomUUID(),
      add: (item, qty) => set((s) => ({ cart: C.addItem(s.cart, item, qty) })),
      setQty: (i, q) => set((s) => ({ cart: C.setQty(s.cart, i, q) })),
      step: (i, u) => set((s) => ({ cart: C.stepQty(s.cart, i, u) })),
      remove: (i) => set((s) => ({ cart: C.removeLine(s.cart, i) })),
      setLineDiscount: (i, d) => set((s) => ({ cart: C.setLineDiscount(s.cart, i, d) })),
      setLinePrice: (i, p) => set((s) => ({ cart: C.setLinePrice(s.cart, i, p) })),
      setBillDiscount: (d) => set((s) => ({ cart: C.setBillDiscount(s.cart, d) })),
      setParty: (p) => set((s) => ({ cart: C.setParty(s.cart, p) })),
      setNotes: (notes) => set((s) => ({ cart: { ...s.cart, notes } })),
      select: (i) => set((s) => ({ cart: C.select(s.cart, i) })),
      load: (cart) => set({ cart, saleKey: crypto.randomUUID() }),
      reset: () => set({ cart: C.EMPTY_CART, saleKey: crypto.randomUUID() }),
    }),
    {
      name: "v2.pos",
      storage: createJSONStorage(() => sessionStorage),
      partialize: (s) => ({ cart: s.cart, saleKey: s.saleKey }),
    },
  ),
);
