import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export const useStore = create((set, get) => ({
  // Authentication State
  user: null,
  token: localStorage.getItem("session_token") || null,
  isAuthenticated: false,
  authLoading: true,
  authError: null,

  login: async (email, password) => {
    set({ authLoading: true, authError: null });
    try {
      const response = await invoke("login", { email, password });
      localStorage.setItem("session_token", response.token);
      set({
        user: response.user,
        token: response.token,
        isAuthenticated: true,
        authLoading: false,
      });
      // Load initial stock alerts after successful login
      get().checkLowStockAlerts();
      return { success: true };
    } catch (error) {
      set({ authError: error, authLoading: false });
      return { success: false, error };
    }
  },

  logout: async () => {
    const { token } = get();
    if (token) {
      try {
        await invoke("logout", { token });
      } catch (e) {
        console.error("Logout backend error:", e);
      }
    }
    localStorage.removeItem("session_token");
    set({
      user: null,
      token: null,
      isAuthenticated: false,
      authLoading: false,
    });
  },

  checkAuth: async () => {
    const { token } = get();
    if (!token) {
      set({ authLoading: false, isAuthenticated: false });
      return;
    }
    set({ authLoading: true });
    try {
      const user = await invoke("get_profile", { token });
      set({ user, isAuthenticated: true, authLoading: false });
      // Fetch low stock alerts
      get().checkLowStockAlerts();
    } catch (error) {
      console.error("Auth check failed:", error);
      localStorage.removeItem("session_token");
      set({ user: null, token: null, isAuthenticated: false, authLoading: false });
    }
  },

  // POS State
  posCart: [],
  posDiscount: 0, // flat amount
  posTaxRate: 18, // 18% default
  heldOrders: [],

  addToCart: (product) => {
    const { posCart } = get();
    const existing = posCart.find((item) => item.id === product.id);
    const priceToUse = (product.offer_price && product.offer_price > 0) ? product.offer_price : product.price;

    if (existing) {
      if (existing.quantity >= product.stock) {
        return { success: false, message: `Only ${product.stock} units available in stock.` };
      }
      set({
        posCart: posCart.map((item) =>
          item.id === product.id
            ? { ...item, quantity: item.quantity + 1, total: (item.quantity + 1) * priceToUse }
            : item
        ),
      });
    } else {
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
            tax_rate: product.tax_rate || 0,
          },
        ],
      });
    }
    return { success: true };
  },

  updateCartQuantity: (productId, quantity, maxStock) => {
    const { posCart } = get();
    if (quantity <= 0) {
      get().removeFromCart(productId);
      return { success: true };
    }
    if (quantity > maxStock) {
      return { success: false, message: `Only ${maxStock} units available in stock.` };
    }
    set({
      posCart: posCart.map((item) =>
        item.id === productId
          ? { ...item, quantity, total: quantity * item.price }
          : item
      ),
    });
    return { success: true };
  },

  removeFromCart: (productId) => {
    set({
      posCart: get().posCart.filter((item) => item.id !== productId),
    });
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
      const dbList = await invoke("list_held_orders_cmd", { token });
      const mapped = dbList.map((o) => ({
        id: o.id,
        customerName: o.customer_name,
        items: JSON.parse(o.items),
        discount: o.discount,
        taxRate: o.tax_rate,
        heldAt: new Date(o.created_at).toLocaleString(),
      }));
      set({ heldOrders: mapped });
    } catch (e) {
      console.error("Failed to fetch held orders:", e);
    }
  },

  holdCurrentOrder: async (customerName = "Walk-in Customer") => {
    const { posCart, posDiscount, posTaxRate, token } = get();
    if (posCart.length === 0) return { success: false, message: "Cart is empty" };

    try {
      const id = Math.random().toString(36).substr(2, 9);
      const itemsStr = JSON.stringify(posCart);
      await invoke("hold_order_cmd", {
        token,
        id,
        customerName,
        discount: parseFloat(posDiscount),
        taxRate: parseFloat(posTaxRate),
        items: itemsStr,
      });
      await get().fetchHeldOrders();
      set({ posCart: [], posDiscount: 0 });
      return { success: true };
    } catch (e) {
      console.error("Failed to hold order:", e);
      return { success: false, message: String(e) };
    }
  },

  recallOrder: async (holdId) => {
    const { heldOrders, token } = get();
    const order = heldOrders.find((o) => o.id === holdId);
    if (!order) return { success: false, message: "Order not found" };

    try {
      set({
        posCart: order.items,
        posDiscount: order.discount,
        posTaxRate: order.taxRate,
      });
      // await invoke("delete_held_order_cmd", { token, id: holdId });
      await get().fetchHeldOrders();
      return { success: true };
    } catch (e) {
      console.error("Failed to recall order:", e);
      return { success: false, message: String(e) };
    }
  },

  deleteHeldOrder: async (holdId) => {
    const { token } = get();
    try {
      await invoke("delete_held_order_cmd", { token, id: holdId });
      await get().fetchHeldOrders();
    } catch (e) {
      console.error("Failed to delete held order:", e);
    }
  },

  // Notification / Alerts State
  notifications: [],
  
  addNotification: (title, message, type = "info") => {
    const newNotif = {
      id: Math.random().toString(36).substr(2, 9),
      title,
      message,
      type, // 'info', 'warning', 'error'
      timestamp: new Date().toLocaleTimeString(),
    };
    set({ notifications: [newNotif, ...get().notifications].slice(0, 50) });
  },

  clearNotifications: () => {
    set({ notifications: [] });
  },

  checkLowStockAlerts: async () => {
    const { token } = get();
    if (!token) return;
    try {
      const products = await invoke("list_products_cmd", { token });
      const lowStockItems = products.filter((p) => p.stock <= p.reorder_level);
      
      // Clear previous warning notifications
      set({ notifications: get().notifications.filter(n => n.type !== "warning") });

      lowStockItems.forEach((p) => {
        get().addNotification(
          "Low Stock Alert",
          `Product '${p.name}' (${p.sku}) is low on stock. Current: ${p.stock}, Reorder level: ${p.reorder_level}`,
          "warning"
        );
      });
    } catch (e) {
      console.error("Failed to check low stock alerts:", e);
    }
  },

  // Preferences State
  currencySymbol: "$",
  setCurrencySymbol: (symbol) => set({ currencySymbol: symbol }),
}));
