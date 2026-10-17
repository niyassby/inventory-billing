import { create } from "zustand";
import type { Legacy } from "@inventory/shared";
import { api } from "@/api/client";
import { toApiError } from "@/api/errors";

/**
 * Global client state. Typed port of the original `hooks/use-store.js`, with the same behaviour so
 * the legacy pages keep working. Phase 5 moves the POS cart into the POS feature and the server
 * state (held orders, alerts) into TanStack Query.
 */

const TOKEN_KEY = "session_token";

function readStoredToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

function writeStoredToken(token: string | null): void {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    // Storage can be unavailable (private mode); the session then lasts until the window closes.
  }
}

export interface CartItem {
  id: string;
  name: string | null;
  sku: string;
  price: number;
  cost: number;
  quantity: number;
  total: number;
  tax_rate: number;
}

export interface HeldOrderView {
  id: string;
  customerName: string;
  items: CartItem[];
  discount: number;
  taxRate: number;
  heldAt: string;
}

export type NotificationType = "info" | "warning" | "error";

export interface AppNotification {
  id: string;
  title: string;
  message: string;
  type: NotificationType;
  timestamp: string;
}

export type ActionResult = { success: true } | { success: false; message: string };

type LoginResult = { success: true } | { success: false; error: string };

interface AppState {
  user: Legacy.User | null;
  token: string | null;
  isAuthenticated: boolean;
  authLoading: boolean;
  authError: string | null;
  login: (email: string, password: string) => Promise<LoginResult>;
  logout: () => Promise<void>;
  checkAuth: () => Promise<void>;

  posCart: CartItem[];
  /** Flat bill discount in major units (legacy float). */
  posDiscount: number;
  posTaxRate: number;
  heldOrders: HeldOrderView[];
  addToCart: (product: Legacy.Product) => ActionResult;
  updateCartQuantity: (productId: string, quantity: number, maxStock: number) => ActionResult;
  removeFromCart: (productId: string) => void;
  clearCart: () => void;
  setPosDiscount: (discount: number) => void;
  setPosTaxRate: (rate: number) => void;
  fetchHeldOrders: () => Promise<void>;
  holdCurrentOrder: (customerName?: string) => Promise<ActionResult>;
  recallOrder: (holdId: string) => Promise<ActionResult>;
  deleteHeldOrder: (holdId: string) => Promise<void>;

  notifications: AppNotification[];
  addNotification: (title: string, message: string, type?: NotificationType) => void;
  clearNotifications: () => void;
  checkLowStockAlerts: () => Promise<void>;

  currencySymbol: string;
  setCurrencySymbol: (symbol: string) => void;
}

function parseHeldItems(json: string): CartItem[] {
  try {
    const parsed: unknown = JSON.parse(json);
    return Array.isArray(parsed) ? (parsed as CartItem[]) : [];
  } catch {
    return [];
  }
}

export const useStore = create<AppState>()((set, get) => ({
  user: null,
  token: readStoredToken(),
  isAuthenticated: false,
  authLoading: true,
  authError: null,

  login: async (email, password) => {
    set({ authLoading: true, authError: null });
    try {
      const response = await api.call("login", { email, password });
      writeStoredToken(response.token);
      set({
        user: response.user,
        token: response.token,
        isAuthenticated: true,
        authLoading: false,
      });
      void get().checkLowStockAlerts();
      return { success: true };
    } catch (error) {
      const message = toApiError(error).message;
      set({ authError: message, authLoading: false });
      return { success: false, error: message };
    }
  },

  logout: async () => {
    const { token } = get();
    if (token) {
      try {
        await api.call("logout", { token });
      } catch (error) {
        console.error("Logout backend error:", error);
      }
    }
    writeStoredToken(null);
    set({ user: null, token: null, isAuthenticated: false, authLoading: false });
  },

  checkAuth: async () => {
    const { token } = get();
    if (!token) {
      set({ authLoading: false, isAuthenticated: false });
      return;
    }
    set({ authLoading: true });
    try {
      const user = await api.call("get_profile", { token });
      set({ user, isAuthenticated: true, authLoading: false });
      void get().checkLowStockAlerts();
    } catch (error) {
      console.error("Auth check failed:", error);
      writeStoredToken(null);
      set({ user: null, token: null, isAuthenticated: false, authLoading: false });
    }
  },

  posCart: [],
  posDiscount: 0,
  posTaxRate: 18,
  heldOrders: [],

  addToCart: (product) => {
    const { posCart } = get();
    const existing = posCart.find((item) => item.id === product.id);
    const priceToUse =
      product.offer_price !== null && product.offer_price > 0 ? product.offer_price : product.price;

    if (existing) {
      if (existing.quantity >= product.stock) {
        return { success: false, message: `Only ${product.stock} units available in stock.` };
      }
      set({
        posCart: posCart.map((item) =>
          item.id === product.id
            ? { ...item, quantity: item.quantity + 1, total: (item.quantity + 1) * priceToUse }
            : item,
        ),
      });
      return { success: true };
    }

    if (product.stock <= 0) {
      return { success: false, message: "Product is out of stock!" };
    }
    set({
      posCart: [
        ...posCart,
        {
          id: product.id,
          name: product.name,
          sku: product.sku,
          price: priceToUse,
          cost: product.cost,
          quantity: 1,
          total: priceToUse,
          tax_rate: product.tax_rate ?? 0,
        },
      ],
    });
    return { success: true };
  },

  updateCartQuantity: (productId, quantity, maxStock) => {
    if (quantity <= 0) {
      get().removeFromCart(productId);
      return { success: true };
    }
    if (quantity > maxStock) {
      return { success: false, message: `Only ${maxStock} units available in stock.` };
    }
    set({
      posCart: get().posCart.map((item) =>
        item.id === productId ? { ...item, quantity, total: quantity * item.price } : item,
      ),
    });
    return { success: true };
  },

  removeFromCart: (productId) => {
    set({ posCart: get().posCart.filter((item) => item.id !== productId) });
  },

  clearCart: () => {
    set({ posCart: [], posDiscount: 0 });
  },

  setPosDiscount: (discount) => {
    set({ posDiscount: Math.max(0, discount) });
  },

  setPosTaxRate: (rate) => {
    set({ posTaxRate: Math.max(0, rate) });
  },

  fetchHeldOrders: async () => {
    const { token } = get();
    if (!token) return;
    try {
      const list = await api.call("list_held_orders_cmd", { token });
      set({
        heldOrders: list.map((o) => ({
          id: o.id,
          customerName: o.customer_name,
          items: parseHeldItems(o.items),
          discount: o.discount,
          taxRate: o.tax_rate,
          heldAt: new Date(o.created_at).toLocaleString(),
        })),
      });
    } catch (error) {
      console.error("Failed to fetch held orders:", error);
    }
  },

  holdCurrentOrder: async (customerName = "Walk-in Customer") => {
    const { posCart, posDiscount, posTaxRate, token } = get();
    if (posCart.length === 0) return { success: false, message: "Cart is empty" };
    if (!token) return { success: false, message: "Not signed in" };

    try {
      await api.call("hold_order_cmd", {
        token,
        id: crypto.randomUUID(),
        customerName,
        discount: posDiscount,
        taxRate: posTaxRate,
        items: JSON.stringify(posCart),
      });
      await get().fetchHeldOrders();
      set({ posCart: [], posDiscount: 0 });
      return { success: true };
    } catch (error) {
      console.error("Failed to hold order:", error);
      return { success: false, message: toApiError(error).message };
    }
  },

  recallOrder: async (holdId) => {
    const order = get().heldOrders.find((o) => o.id === holdId);
    if (!order) return { success: false, message: "Order not found" };

    // Known issue (audit B-16): the held order is not deleted on recall. Fixed in Phase 5.
    set({ posCart: order.items, posDiscount: order.discount, posTaxRate: order.taxRate });
    await get().fetchHeldOrders();
    return { success: true };
  },

  deleteHeldOrder: async (holdId) => {
    const { token } = get();
    if (!token) return;
    try {
      await api.call("delete_held_order_cmd", { token, id: holdId });
      await get().fetchHeldOrders();
    } catch (error) {
      console.error("Failed to delete held order:", error);
    }
  },

  notifications: [],

  addNotification: (title, message, type = "info") => {
    const notification: AppNotification = {
      id: crypto.randomUUID(),
      title,
      message,
      type,
      timestamp: new Date().toLocaleTimeString(),
    };
    set({ notifications: [notification, ...get().notifications].slice(0, 50) });
  },

  clearNotifications: () => {
    set({ notifications: [] });
  },

  checkLowStockAlerts: async () => {
    const { token } = get();
    if (!token) return;
    try {
      const products = await api.call("list_products_cmd", { token });
      set({ notifications: get().notifications.filter((n) => n.type !== "warning") });
      for (const p of products.filter((item) => item.stock <= item.reorder_level)) {
        get().addNotification(
          "Low Stock Alert",
          `Product '${p.name}' (${p.sku}) is low on stock. Current: ${p.stock}, Reorder level: ${p.reorder_level}`,
          "warning",
        );
      }
    } catch (error) {
      console.error("Failed to check low stock alerts:", error);
    }
  },

  currencySymbol: "$",
  setCurrencySymbol: (symbol) => {
    set({ currencySymbol: symbol });
  },
}));
