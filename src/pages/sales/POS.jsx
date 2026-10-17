import React, { useState, useEffect, useRef } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Search01Icon,
  UserAdd01Icon,
  Delete02Icon,
  InvoiceIcon,
  ArrowLeft01Icon,
  Clock01Icon
} from "@hugeicons/core-free-icons";
import { PrinterIcon } from "@hugeicons/core-free-icons/index";
import { useCategories } from "@/hooks/tanstack/useCategorie&Brand";

export default function POS() {
  const navigate = useNavigate();
  const location = useLocation();
  const editInvoiceId = location.state?.editInvoiceId || null;
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Zustand POS Cart Actions
  const {
    posCart,
    posDiscount,
    posTaxRate,
    addToCart,
    updateCartQuantity,
    removeFromCart,
    clearCart,
    setPosDiscount,
    setPosTaxRate,
    heldOrders,
    holdCurrentOrder,
    recallOrder,
    fetchHeldOrders,
    deleteHeldOrder
  } = useStore();

  // Data States
  const [products, setProducts] = useState([]);
  const [customers, setCustomers] = useState([]);
  const [selectedCategory, setSelectedCategory] = useState("ALL");
  const [searchQuery, setSearchQuery] = useState("");
  
  const {data: categories}=useCategories({token});

  // Customer states
  const [selectedCustomerId, setSelectedCustomerId] = useState("");
  const [isCustomerModalOpen, setIsCustomerModalOpen] = useState(false);
  const [customerForm, setCustomerForm] = useState({ name: "", email: "", phone: "", address: "" });

  // Payment State
  const [paymentMethod, setPaymentMethod] = useState("CASH");
  const [amountPaid, setAmountPaid] = useState(0);

  // Receipt Modal State
  const [isReceiptOpen, setIsReceiptOpen] = useState(false);
  const [printedInvoice, setPrintedInvoice] = useState(null);

  // Held Orders Modal State
  const [isHoldModalOpen, setIsHoldModalOpen] = useState(false);
  const [holdCustomerName, setHoldCustomerName] = useState("Walk-in Customer");

  const searchInputRef = useRef(null);

  // Fetch initial data
  const loadData = async () => {
    try {
      const prodList = await invoke("list_products_cmd", { token });
      setProducts(prodList);
      const custList = await invoke("list_customers_cmd", { token });
      setCustomers(custList);
      await fetchHeldOrders();
      // Auto select walk-in customer if available
      const walkin = custList.find((c) => c.name.toLowerCase().includes("walk-in"));
      if (walkin) setSelectedCustomerId(walkin.id);
    } catch (e) {
      console.error("POS Data Loading Error:", e);
    }
  };

  useEffect(() => {
    loadData();
    // Focus search on mount
    if (searchInputRef.current) {
      searchInputRef.current.focus();
    }
  }, [token]);

  useEffect(() => {
    if (editInvoiceId && token) {
      const loadInvoiceForEditing = async () => {
        try {
          const invDetail = await invoke("get_invoice_cmd", { token, id: editInvoiceId });
          
          // Map invoice items to POS Cart format
          const mappedItems = invDetail.items.map((item) => ({
            id: item.product_id,
            name: item.product_name,
            sku: "",
            price: item.unit_price,
            cost: 0,
            quantity: item.quantity,
            total: item.total,
            tax_rate: item.tax_rate !== undefined && item.tax_rate !== null ? item.tax_rate : 0
          }));
          
          useStore.setState({
            posCart: mappedItems,
            posDiscount: invDetail.discount,
            posTaxRate: invDetail.subtotal > 0 ? (invDetail.tax * 100) / invDetail.subtotal : 18
          });
          
          setSelectedCustomerId(invDetail.customer_id);
          setPaymentMethod(invDetail.payment_method || "CASH");
          setAmountPaid(invDetail.amount_paid);
        } catch (e) {
          console.error("Failed to load invoice for editing:", e);
          alert("Failed to load invoice for editing: " + e);
        }
      };
      
      loadInvoiceForEditing();
    }
  }, [editInvoiceId, token]);

  // Set default amount paid when cart changes
  const subtotal = posCart.reduce((acc, item) => acc + item.total, 0);
  const taxAmount = posCart.reduce((acc, item) => acc + (item.total * (item.tax_rate || 0)) / 100, 0);
  const netTotal = Math.max(0, subtotal + taxAmount - posDiscount);

  useEffect(() => {
    setAmountPaid(netTotal);
  }, [netTotal]);

  // Handle barcode scanner input (if scanning matches SKU directly)
  const handleSearchKeyPress = (e) => {
    if (e.key === "Enter" && searchQuery.trim()) {
      const match = products.find(
        (p) =>
          p.sku.toLowerCase() === searchQuery.trim().toLowerCase() ||
          (p.barcode && p.barcode.toLowerCase() === searchQuery.trim().toLowerCase())
      );
      if (match) {
        const res = addToCart(match);
        if (!res.success) {
          alert(res.message);
        } else {
          setSearchQuery(""); // Clear search on match
        }
      }
    }
  };

  // Add Customer Submit
  const handleCustomerSubmit = async (e) => {
    e.preventDefault();
    try {
      const newCust = await invoke("create_customer_cmd", {
        token,
        name: customerForm.name,
        email: customerForm.email || null,
        phone: customerForm.phone || null,
        address: customerForm.address || null
      });
      setCustomers([...customers, newCust]);
      setSelectedCustomerId(newCust.id);
      setIsCustomerModalOpen(false);
      setCustomerForm({ name: "", email: "", phone: "", address: "" });
    } catch (e) {
      alert("Error adding customer: " + e);
    }
  };

  // Checkout Handler
  const handleCheckout = async () => {
    if (posCart.length === 0) return alert("Cart is empty!");
    if (!selectedCustomerId) return alert("Please select a customer!");

    // Map cart items into InvoiceItem format
    const items = posCart.map((item) => ({
      id: "", // generated in DB
      invoice_id: editInvoiceId || "",
      product_id: item.id,
      product_name: item.name,
      quantity: item.quantity,
      unit_price: item.price,
      total: item.total
    }));

    try {
      if (editInvoiceId) {
        const invoice = await invoke("update_invoice_cmd", {
          token,
          id: editInvoiceId,
          customerId: selectedCustomerId,
          subtotal,
          discount: posDiscount,
          tax: taxAmount,
          total: netTotal,
          amountPaid: parseFloat(amountPaid) || 0,
          paymentMethod: paymentMethod,
          items
        });

        setPrintedInvoice(invoice);
        setIsReceiptOpen(true);
        clearCart();
        navigate("/sales/invoices");
      } else {
        const invoice = await invoke("create_invoice_cmd", {
          token,
          customerId: selectedCustomerId,
          subtotal,
          discount: posDiscount,
          tax: taxAmount,
          total: netTotal,
          amountPaid: parseFloat(amountPaid) || 0,
          paymentMethod: paymentMethod,
          items
        });

        setPrintedInvoice(invoice);
        setIsReceiptOpen(true);
        clearCart();
        loadData(); // reload products to get updated stock figures
      }
    } catch (e) {
      console.log(e);
      alert("Error processing sale: " + e);
    }
  };

  // Hold current cart
  const handleHoldOrder = async () => {
    const res = await holdCurrentOrder(holdCustomerName);
    if (res.success) {
      setIsHoldModalOpen(false);
      setHoldCustomerName("Walk-in Customer");
    } else {
      alert(res.message);
    }
  };

  const handleRecall = async (holdId) => {
    const res = await recallOrder(holdId);
    if (res.success) {
      // close held orders modal if open
    } else {
      alert(res.message);
    }
  };

  // Filter products
  const filteredProducts = products.filter((p) => {
    const matchesSearch =
      p.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      p.sku.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (p.barcode && p.barcode.toLowerCase().includes(searchQuery.toLowerCase()));
    
    const matchesCategory = selectedCategory === "ALL" ? true : p.category_id === selectedCategory;
    
    return matchesSearch && matchesCategory;
  }).sort((a, b) => b.stock - a.stock);

  return (
    <div className="flex flex-col min- h-screen bg-muted/20 font-sans text-foreground overflow-hidden">
      {/* POS Top Navigation Bar */}
      <header className="flex h-16 shrink-0 bg-background border-b items-center justify-between px-6 border-border/40 print:hidden">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="icon" onClick={() => navigate(-1)} className="h-9 w-9">
            <HugeiconsIcon icon={ArrowLeft01Icon} size={18} />
          </Button>
          <h1 className="text-xl font-bold tracking-tight">Point of Sale Terminal</h1>
        </div>
        <div className="flex items-center gap-2">
          {/* Draft Orders list count toggle */}
          {heldOrders.length > 0 && (
            <Button
              variant="outline"
              onClick={() => setIsHoldModalOpen(true)}
              className="flex items-center gap-1.5 border-amber-500/30 bg-amber-500/5 text-amber-600 hover:bg-amber-500/10"
            >
              <HugeiconsIcon icon={Clock01Icon} size={16} /> Held Drafts ({heldOrders.length})
            </Button>
          )}
          <div className="text-xs px-3 py-1 rounded-full font-medium bg-emerald-500/10 text-emerald-600 border border-emerald-500/20">
            Terminal Online (Offline-First)
          </div>
        </div>
      </header>

      {/* Main Grid Workspace */}
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-12 overflow-hidden print:hidden">
        {/* Left Side: Product tiles catalog (cols-7) */}
        <div className="lg:col-span-7 flex flex-col h-full bg-background border-r border-border/40 overflow-hidden">
          {/* Filters Bar */}
          <div className="p-4 border-b border-border/40 flex flex-col md:flex-row gap-3">
            <div className="relative flex-1">
              <HugeiconsIcon icon={Search01Icon} size={18} strokeWidth={2} className="absolute left-3 top-2.5 text-muted-foreground" />
              <Input
                ref={searchInputRef}
                placeholder="Scan Barcode or Search Product name/SKU..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={handleSearchKeyPress}
                className="pl-9 bg-card border-border/40 focus-visible:ring-primary"
              />
            </div>
            {/* Quick Category tabs */}
            <select
              className="p-2 border rounded-md text-sm bg-card border-border/40 text-foreground"
              value={selectedCategory}
              onChange={(e) => setSelectedCategory(e.target.value)}
            >
              <option value="ALL">All Categories</option>
              {categories?.map((c) => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
          </div>

          {/* Tiles Area */}
          <ScrollArea className="flex-1 p-4 bg-muted/5 h-[60vh]">
            <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-3">
              {filteredProducts.map((p) => {
                const isLow = p.stock <= p.reorder_level;
                const isOut = p.stock <= 0;
                return (
                  <button
                    key={p.id}
                    disabled={isOut}
                    onClick={() => {
                      const res = addToCart(p);
                      if (!res.success) alert(res.message);
                    }}
                    className={`flex flex-col text-left p-3.5 bg-card border border-border/40 rounded-xl hover:shadow-md hover:border-primary/40 transition-all duration-200 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed group relative`}
                  >
                    <div className="flex-1">
                      <div className="font-semibold text-sm group-hover:text-primary transition-colors duration-150 line-clamp-2 leading-snug">
                        {p.name}
                      </div>
                      <div className="text-[10px] text-muted-foreground font-mono mt-1">
                        SKU: {p.sku}
                      </div>
                    </div>
                    <div className="mt-3 flex justify-between items-baseline w-full">
                      <span className="font-bold text-sm text-foreground font-mono">
                        {currencySymbol}{p.price.toFixed(2)}
                      </span>
                      <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded-full ${isOut ? 'bg-destructive/15 text-destructive' :
                        isLow ? 'bg-amber-500/10 text-amber-600' : 'bg-muted text-muted-foreground'
                        }`}>
                        Stock: {p.stock}
                      </span>
                    </div>
                  </button>
                );
              })}
              {filteredProducts.length === 0 && (
                <div className="col-span-full py-16 text-center text-muted-foreground text-sm border-2 border-dashed rounded-lg bg-card/40">
                  No products match query. Add items to inventory list first.
                </div>
              )}
            </div>
          </ScrollArea>
        </div>

        {/* Right Side: Cash Register Cart & Billing totals (cols-5) */}
        <div className="lg:col-span-5 flex flex-col h-full bg-card overflow-hidden relative">
          {/* Cart Header: Customer selector */}
          <div className="p-4 border-b border-border/40 flex gap-2 items-center">
            <div className="flex-1">
              <Label className="text-xs text-muted-foreground">Select CRM Customer</Label>
              <select
                className="w-full mt-1 p-2 border rounded-md text-sm bg-card border-border/40 text-foreground"
                value={selectedCustomerId}
                onChange={(e) => setSelectedCustomerId(e.target.value)}
              >
                <option value="" disabled>Select Customer</option>
                {customers.map((c) => (
                  <option key={c.id} value={c.id}>{c.name} {c.phone ? `(${c.phone})` : ''}</option>
                ))}
              </select>
            </div>
            <Button
              variant="outline"
              size="icon"
              className="mt-5 border-border/40 h-[38px] w-[38px]"
              onClick={() => setIsCustomerModalOpen(true)}
              title="Add New Customer"
            >
              <HugeiconsIcon icon={UserAdd01Icon} size={16} />
            </Button>
          </div>

          {/* Cart items list */}
          <ScrollArea className="flex-1 p-4  min-h-[20vh]">
            {posCart.map((item) => {
              const product = products.find((p) => p.id === item.id);
              const maxStock = product ? product.stock : 9999;
              return (
                <div key={item.id} className="py-3.5 flex justify-between items-center first:pt-0">
                  <div className="flex-1 min-w-0 pr-2">
                    <div className="font-semibold text-sm truncate">{item.name}</div>
                    <div className="text-xs text-muted-foreground font-mono mt-0.5">
                      {currencySymbol}{item.price.toFixed(2)}/unit
                    </div>
                  </div>
                  {/* Quantity adjusts */}
                  <div className="flex items-center gap-1.5 mr-4 shrink-0">
                    <Button
                      size='icon'
                      variant="outline"
                      onClick={() => updateCartQuantity(item.id, item.quantity - 1, maxStock)}
                    >
                      -
                    </Button>
                    <span className="w-8 text-center font-bold text-sm font-mono">{item.quantity}</span>
                    <Button
                      size='icon'
                      variant="outline"
                      onClick={() => updateCartQuantity(item.id, item.quantity + 1, maxStock)}
                    >
                      +
                    </Button>
                  </div>
                  <div className="text-right shrink-0">
                    <div className="font-bold text-sm font-mono">{currencySymbol}{item.total.toFixed(2)}</div>
                    <button
                      onClick={() => removeFromCart(item.id)}
                      className="text-xs text-destructive hover:underline mt-1 font-semibold flex items-center gap-0.5 ml-auto cursor-pointer"
                    >
                      <HugeiconsIcon icon={Delete02Icon} size={12} /> Remove
                    </button>
                  </div>
                </div>
              );
            })}
            {posCart.length === 0 && (
              <div className="flex flex-col items-center justify-center py-20 text-center text-muted-foreground text-sm">
                <HugeiconsIcon icon={InvoiceIcon} size={36} className="text-muted-foreground/45 mb-2" />
                <p>Checkout cart is currently empty.</p>
                <p className="text-xs mt-1 text-muted-foreground/80">Click on products on the left or scan item barcode.</p>
              </div>
            )}
          </ScrollArea>

          {/* Pricing Totals Section */}
          <div className="">
          <div className="p-4 border-t border-border/40 bg-muted/10 space-y-3 shrink-0">
            <div className="flex justify-between text-sm">
              <span className="text-muted-foreground">Subtotal</span>
              <span className="font-bold font-mono">{currencySymbol}{subtotal.toFixed(2)}</span>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div>
                <Label className="text-[10px] text-muted-foreground uppercase font-bold">Discount Amount</Label>
                <div className="relative mt-1">
                  <span className="absolute left-2.5 top-2.5 text-xs text-muted-foreground">{currencySymbol}</span>
                  <Input
                    type="number"
                    value={posDiscount}
                    onChange={(e) => setPosDiscount(parseFloat(e.target.value) || 0)}
                    className="pl-6 h-8 text-xs bg-card border-border/40 font-mono"
                  />
                </div>
              </div>
              <div>
                <Label className="text-[10px] text-muted-foreground uppercase font-bold">Tax Amount</Label>
                <div className="relative mt-1">
                  <span className="absolute left-2.5 top-2.5 text-xs text-muted-foreground">{currencySymbol}</span>
                  <Input
                    type="text"
                    value={taxAmount.toFixed(2)}
                    disabled
                    className="pl-6 h-8 text-xs bg-muted border-border/40 font-mono opacity-80 cursor-not-allowed"
                  />
                </div>
              </div>
            </div>

            <div className="flex justify-between border-t border-border/30 pt-3 text-lg font-bold">
              <span>Total Price</span>
              <span className="font-mono text-primary">{currencySymbol}{netTotal.toFixed(2)}</span>
            </div>
          </div>

          {/* Checkout Controls Drawer */}
          <div className="p-4 border-t border-border/40 bg-background space-y-4 shrink-0">
            {/* Payment options */}
            <div className="space-y-2">
              <Label className="text-xs text-muted-foreground">Select Payment Method</Label>
              <div className="grid grid-cols-4 gap-2">
                {["CASH", "CARD", "UPI", "CREDIT"].map((m) => (
                  <button
                    key={m}
                    onClick={() => setPaymentMethod(m)}
                    className={`p-2 text-xs font-bold rounded-lg border text-center transition-all cursor-pointer ${paymentMethod === m
                      ? "bg-primary border-primary text-primary-foreground shadow"
                      : "bg-card border-border/40 text-muted-foreground hover:bg-muted/40"
                      }`}
                  >
                    {m}
                  </button>
                ))}
              </div>
            </div>

            {/* Split payments / Amount Paid */}
            {paymentMethod !== "CREDIT" && (
              <div className="space-y-2">
                <div className="flex justify-between text-xs text-muted-foreground">
                  <Label>Amount Tendered (Paid)</Label>
                  <button
                    onClick={() => setAmountPaid(netTotal)}
                    className="text-primary underline font-medium hover:text-primary/95"
                  >
                    Exact Amount
                  </button>
                </div>
                <Input
                  type="number"
                  value={amountPaid}
                  onChange={(e) => setAmountPaid(parseFloat(e.target.value) || 0)}
                  className="bg-card border-border/40 font-mono"
                />
                {amountPaid > netTotal && (
                  <div className="flex justify-between text-xs text-emerald-600 font-semibold mt-1 bg-emerald-500/10 p-2 rounded">
                    <span>Change Return:</span>
                    <span className="font-mono">{currencySymbol}{(amountPaid - netTotal).toFixed(2)}</span>
                  </div>
                )}
              </div>
            )}

            {/* Action buttons */}
            <div className="flex gap-2">
              <Button
                variant="outline"
                className="w-1/3 border-border/40"
                onClick={() => {
                  if (posCart.length === 0) return alert("Cart is empty");
                  setIsHoldModalOpen(true);
                }}
              >
                Hold Order
              </Button>
              <Button
                onClick={handleCheckout}
                disabled={posCart.length === 0}
                className="flex-1 bg-primary text-primary-foreground font-bold shadow hover:bg-primary/95 flex items-center justify-center gap-1.5"
              >
                <HugeiconsIcon icon={InvoiceIcon} size={16} /> Complete Sale ({currencySymbol}{netTotal.toFixed(2)})
              </Button>
            </div>
          </div>
          </div>
        </div>
      </div>

      {/* ==========================================
          POS DIALOGS & RECEIPT MODALS
         ========================================== */}

      {/* Customer Addition Modal */}
      <Dialog open={isCustomerModalOpen} onOpenChange={setIsCustomerModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>Quick Register Customer</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCustomerSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="cust_name">Customer Name *</Label>
              <Input
                id="cust_name"
                value={customerForm.name}
                onChange={(e) => setCustomerForm({ ...customerForm, name: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="cust_phone">Phone Number</Label>
              <Input
                id="cust_phone"
                value={customerForm.phone}
                onChange={(e) => setCustomerForm({ ...customerForm, phone: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="cust_email">Email Address</Label>
              <Input
                id="cust_email"
                type="email"
                value={customerForm.email}
                onChange={(e) => setCustomerForm({ ...customerForm, email: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="cust_addr">Billing Address</Label>
              <Input
                id="cust_addr"
                value={customerForm.address}
                onChange={(e) => setCustomerForm({ ...customerForm, address: e.target.value })}
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsCustomerModalOpen(false)}>Cancel</Button>
              <Button type="submit">Create Customer</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Hold Order Modal */}
      <Dialog open={isHoldModalOpen} onOpenChange={setIsHoldModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>Hold Order / Recall Drafts</DialogTitle>
          </DialogHeader>

          {/* If holding current order */}
          {posCart.length > 0 && (
            <div className="space-y-4 pb-4 border-b">
              <h4 className="text-sm font-semibold">Hold Current Cart</h4>
              <div className="space-y-2">
                <Label>Tag/Customer Name</Label>
                <Input
                  value={holdCustomerName}
                  onChange={(e) => setHoldCustomerName(e.target.value)}
                />
              </div>
              <Button onClick={handleHoldOrder} className="w-full">
                Hold Current Order
              </Button>
            </div>
          )}

          {/* Held orders list */}
          <div className="space-y-3 mt-4">
            <h4 className="text-sm font-semibold">Active Held Orders</h4>
            <ScrollArea className="h-48 divide-y">
              {heldOrders.map((o) => (
                <div key={o.id} className="py-2.5 flex justify-between items-center">
                  <div>
                    <div className="font-semibold text-xs text-foreground">{o.customerName}</div>
                    <div className="text-[10px] text-muted-foreground mt-0.5">{o.heldAt} • {o.items.length} items</div>
                  </div>
                  <div className="flex gap-1.5">
                    <Button size="sm" variant="outline" className="text-xs py-1 h-7" onClick={() => {
                      handleRecall(o.id);
                      setIsHoldModalOpen(false);
                    }}>
                      Recall
                    </Button>
                    <Button size="icon" variant="ghost" className="h-7 w-7 text-destructive" onClick={() => {
                      if (window.confirm("Delete held draft?")) {
                        deleteHeldOrder(o.id);
                      }
                    }}>
                      <HugeiconsIcon icon={Delete02Icon} size={12} />
                    </Button>
                  </div>
                </div>
              ))}
              {heldOrders.length === 0 && (
                <p className="text-xs text-center text-muted-foreground py-6">No held orders found.</p>
              )}
            </ScrollArea>
          </div>
        </DialogContent>
      </Dialog>

      {/* Thermal Receipt Print Modal */}
      {isReceiptOpen && printedInvoice && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 font-sans print:bg-white">
          <div className="bg-card w-full max-w-sm rounded-xl border p-5 shadow-2xl flex flex-col max-h-[90vh] print:border-none print:shadow-none print:max-h-none print:p-0 print:w-full print:max-w-none">
            {/* Header: Actions */}
            <div className="flex justify-between items-center mb-4 border-b pb-2 print:hidden">
              <h3 className="font-bold text-sm">Receipt Printed</h3>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={() => window.print()} className="flex items-center gap-1">
                  <HugeiconsIcon icon={PrinterIcon} size={14} /> Print
                </Button>
                <Button size="sm" onClick={() => setIsReceiptOpen(false)}>
                  Close
                </Button>
              </div>
            </div>

            {/* Scrollable Receipt Body (80mm width look) */}
            <ScrollArea className="flex-1 bg-white text-black p-4 rounded-lg font-mono text-xs border border-dashed border-gray-300 print:border-none print:p-0">
              <div className="text-center space-y-1">
                <h2 className="text-base font-bold uppercase tracking-wide">Offline Billing Store</h2>
                <p className="text-[10px] text-gray-500">123 Local Street, City Central</p>
                <p className="text-[10px] text-gray-500">Phone: +1 (234) 567-890</p>
              </div>

              <div className="my-4 border-t border-b border-gray-300 py-2 space-y-1 text-[10px]">
                <div><span className="font-bold">INVOICE:</span> #{printedInvoice.id.substring(0, 8).toUpperCase()}</div>
                <div><span className="font-bold">DATE:</span> {new Date(printedInvoice.created_at).toLocaleString()}</div>
                <div><span className="font-bold">CUSTOMER:</span> {printedInvoice.customer_name || "Walk-in"}</div>
                {printedInvoice.customer_phone && <div><span className="font-bold">PHONE:</span> {printedInvoice.customer_phone}</div>}
              </div>

              {/* Items List */}
              <table className="w-full text-[10px] text-left border-collapse">
                <thead>
                  <tr className="border-b border-gray-400 font-bold">
                    <th className="pb-1.5">Item</th>
                    <th className="pb-1.5 text-center">Qty</th>
                    <th className="pb-1.5 text-right">Price</th>
                    <th className="pb-1.5 text-right">Total</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-200">
                  {(Array.isArray(printedInvoice.items) ? printedInvoice.items : (typeof printedInvoice.items === 'string' ? JSON.parse(printedInvoice.items) : [])).map((item) => (
                    <tr key={item.product_id}>
                      <td className="py-1.5 truncate max-w-[120px]">{item.product_name}</td>
                      <td className="py-1.5 text-center font-bold">{item.quantity}</td>
                      <td className="py-1.5 text-right">{currencySymbol}{(item.unit_price || item.price || 0).toFixed(2)}</td>
                      <td className="py-1.5 text-right">{currencySymbol}{item.total.toFixed(2)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>

              {/* Summary Totals */}
              <div className="mt-4 border-t border-gray-300 pt-2 space-y-1.5 text-[10px]">
                <div className="flex justify-between">
                  <span>Subtotal</span>
                  <span>{currencySymbol}{printedInvoice.subtotal.toFixed(2)}</span>
                </div>
                {printedInvoice.discount > 0 && (
                  <div className="flex justify-between text-gray-600">
                    <span>Discount</span>
                    <span>-{currencySymbol}{printedInvoice.discount.toFixed(2)}</span>
                  </div>
                )}
                {printedInvoice.tax > 0 && (
                  <div className="flex justify-between">
                    <span>Tax</span>
                    <span>{currencySymbol}{printedInvoice.tax.toFixed(2)}</span>
                  </div>
                )}
                <div className="flex justify-between text-sm font-bold border-t border-gray-400 pt-1.5">
                  <span>Net Amount</span>
                  <span>{currencySymbol}{printedInvoice.total.toFixed(2)}</span>
                </div>
                <div className="flex justify-between text-[10px] text-gray-500 pt-1">
                  <span>Amount Paid ({printedInvoice.payment_method})</span>
                  <span>{currencySymbol}{printedInvoice.amount_paid.toFixed(2)}</span>
                </div>
                {printedInvoice.total > printedInvoice.amount_paid && (
                  <div className="flex justify-between text-rose-600 font-bold">
                    <span>Balance Due</span>
                    <span>{currencySymbol}{(printedInvoice.total - printedInvoice.amount_paid).toFixed(2)}</span>
                  </div>
                )}
              </div>

              <div className="text-center mt-6 pt-4 border-t border-dashed border-gray-400 space-y-1 text-[10px]">
                <p className="font-bold">THANK YOU FOR YOUR VISIT!</p>
                <p className="text-gray-500">Offline-first Billing Engine</p>
              </div>
            </ScrollArea>
          </div>
        </div>
      )}
    </div>
  );
}
