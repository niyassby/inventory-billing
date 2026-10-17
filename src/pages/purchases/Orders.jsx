import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/stores/app-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Search01Icon,
  ShoppingBagIcon,
  CheckmarkBadgeIcon,
  InvoiceIcon,
  ArrowLeft01Icon,
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";

export default function Orders() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [purchases, setPurchases] = useState([]);
  const [suppliers, setSuppliers] = useState([]);
  const [products, setProducts] = useState([]);
  const [loading, setLoading] = useState(true);

  // Filters State
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedStatus, setSelectedStatus] = useState("");

  // Pagination & Sorting State
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(10);
  const [sortBy, setSortBy] = useState("created_at");
  const [sortOrder, setSortOrder] = useState("DESC");

  // Purchase Creator Modal
  const [isPurchaseModalOpen, setIsPurchaseModalOpen] = useState(false);
  const [selectedSupplierId, setSelectedSupplierId] = useState("");
  const [purchaseItems, setPurchaseItems] = useState([]);
  const [selectedProductId, setSelectedProductId] = useState("");
  const [selectedProductQty, setSelectedProductQty] = useState(10);
  const [customCost, setCustomCost] = useState(0);

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_purchases_filtered_cmd", {
        token,
        search: searchQuery || null,
        status: selectedStatus || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setPurchases(paginated.purchases);
      setTotalCount(paginated.total_count);

      const supplierList = await invoke("list_suppliers_cmd", { token });
      setSuppliers(supplierList);

      const prodList = await invoke("list_products_cmd", { token });
      setProducts(prodList);
    } catch (e) {
      console.error("Purchases data load failed:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token, selectedStatus, sortBy, sortOrder, page, limit]);

  // Reset page to 1 when search or status changes
  useEffect(() => {
    setPage(1);
    fetchData();
  }, [searchQuery, selectedStatus]);

  // Handle product cost update in dropdown
  const handleProductSelect = (id) => {
    setSelectedProductId(id);
    const prod = products.find((p) => p.id === id);
    if (prod) {
      setCustomCost(prod.cost);
    }
  };

  // Add Item to Purchase builder
  const handleAddPurchaseItem = () => {
    if (!selectedProductId) return;
    const prod = products.find((p) => p.id === selectedProductId);
    if (!prod) return;

    const existing = purchaseItems.find((item) => item.product_id === prod.id);
    if (existing) {
      setPurchaseItems(
        purchaseItems.map((item) =>
          item.product_id === prod.id
            ? {
                ...item,
                quantity: item.quantity + selectedProductQty,
                total: (item.quantity + selectedProductQty) * item.cost,
              }
            : item,
        ),
      );
    } else {
      setPurchaseItems([
        ...purchaseItems,
        {
          product_id: prod.id,
          product_name: prod.name,
          sku: prod.sku,
          quantity: selectedProductQty,
          cost: parseFloat(customCost) || prod.cost,
          total: (parseFloat(customCost) || prod.cost) * selectedProductQty,
        },
      ]);
    }
    setSelectedProductId("");
    setSelectedProductQty(10);
    setCustomCost(0);
  };

  // Submit Purchase Order
  const handlePurchaseSubmit = async (e) => {
    e.preventDefault();
    if (purchaseItems.length === 0) return alert("Purchase order is empty!");
    if (!selectedSupplierId) return alert("Select a supplier!");

    const total = purchaseItems.reduce((acc, item) => acc + item.total, 0);

    try {
      await invoke("create_purchase_cmd", {
        token,
        supplierId: selectedSupplierId,
        total,
        status: "PENDING", // PENDING = not received, stock not updated yet
        items: purchaseItems.map((item) => ({
          id: "",
          purchase_id: "",
          product_id: item.product_id,
          product_name: item.product_name,
          quantity: item.quantity,
          unit_price: item.cost,
          total: item.total,
        })),
      });
      setIsPurchaseModalOpen(false);
      setPurchaseItems([]);
      setSelectedSupplierId("");
      fetchData();
      alert("Purchase order drafted. Click 'Receive Stock' once products arrive.");
    } catch (e) {
      alert("Error saving purchase: " + e);
    }
  };

  // Receive Stock Handler (Sets status to RECEIVED and triggers inventory stock increment in backend!)
  const handleReceiveStock = async (purchase) => {
    if (
      !window.confirm(
        "Verify receipt of items? This will increment stock and update inventory cost basis.",
      )
    )
      return;
    try {
      await invoke("receive_purchase_cmd", { token, id: purchase.id });
      alert("Stock successfully received and added to inventory!");
      fetchData();
    } catch (e) {
      alert("Error receiving stock: " + e);
    }
  };

  const toggleSort = (field) => {
    if (sortBy === field) {
      setSortOrder(sortOrder === "ASC" ? "DESC" : "ASC");
    } else {
      setSortBy(field);
      setSortOrder("ASC");
    }
  };

  // Filters
  const filteredPurchases = purchases.filter((p) => {
    return (
      p.supplier_name?.toLowerCase().includes(searchQuery.toLowerCase()) ||
      p.id.toLowerCase().includes(searchQuery.toLowerCase())
    );
  });

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Purchases & Restocks</h1>
          <p className="text-muted-foreground">
            Order restock inventory from suppliers, log bills, and track purchase expenses.
          </p>
        </div>
        <div className="flex gap-2 print:hidden">
          <Button
            variant="default"
            onClick={() => navigate("/purchases/orders")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ShoppingBagIcon} size={16} strokeWidth={2} /> Purchase Orders
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/purchases/bills")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={InvoiceIcon} size={16} strokeWidth={2} /> Supplier Bills
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/purchases/returns")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Add01Icon} size={16} strokeWidth={2} /> Purchase Returns
          </Button>
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex flex-col items-start justify-between gap-3 sm:flex-row sm:items-center">
          <div className="flex w-full flex-1 gap-2 sm:w-auto">
            <div className="relative max-w-sm flex-1">
              <HugeiconsIcon
                icon={Search01Icon}
                size={18}
                strokeWidth={2}
                className="absolute top-2.5 left-3 text-muted-foreground"
              />
              <Input
                placeholder="Search by supplier name or Order ID..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="border-border/40 bg-card pl-9"
              />
            </div>
            <select
              value={selectedStatus}
              onChange={(e) => setSelectedStatus(e.target.value)}
              className="max-w-xs rounded-md border border-border/40 bg-card p-2 text-xs text-foreground focus:outline-none"
            >
              <option value="">All Statuses</option>
              <option value="PENDING">Pending</option>
              <option value="RECEIVED">Received</option>
            </select>
          </div>
          <Button
            onClick={() => setIsPurchaseModalOpen(true)}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Add01Icon} size={16} /> New Purchase Order
          </Button>
        </div>

        {loading ? (
          <div className="flex h-[30vh] items-center justify-center">
            <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
          </div>
        ) : (
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-left text-sm">
                  <thead>
                    <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("created_at")}
                      >
                        Date {sortBy === "created_at" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("id")}
                      >
                        Order ID {sortBy === "id" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("supplier_name")}
                      >
                        Supplier Name{" "}
                        {sortBy === "supplier_name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 text-right font-medium select-none"
                        onClick={() => toggleSort("total")}
                      >
                        Cost Total {sortBy === "total" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 text-center font-medium select-none"
                        onClick={() => toggleSort("status")}
                      >
                        Status {sortBy === "status" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 text-right font-medium">Actions</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {purchases.map((purchase) => (
                      <tr key={purchase.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs text-muted-foreground">
                          {new Date(purchase.created_at).toLocaleString()}
                        </td>
                        <td className="p-4 font-mono text-xs font-semibold">
                          #{purchase.id.substring(0, 8).toUpperCase()}
                        </td>
                        <td className="p-4 font-medium">{purchase.supplier_name}</td>
                        <td className="p-4 text-right font-mono font-bold">
                          {currencySymbol}
                          {purchase.total.toFixed(2)}
                        </td>
                        <td className="p-4 text-center">
                          <span
                            className={`rounded-full border px-2 py-0.5 text-[10px] font-semibold ${
                              purchase.status === "PENDING"
                                ? "border-amber-500/20 bg-amber-500/10 text-amber-600"
                                : "border-emerald-500/20 bg-emerald-500/10 text-emerald-600"
                            }`}
                          >
                            {purchase.status}
                          </span>
                        </td>
                        <td className="p-4 text-right">
                          {purchase.status === "PENDING" && (
                            <Button
                              size="sm"
                              className="ml-auto flex h-8 items-center gap-1 bg-emerald-600 text-xs text-white hover:bg-emerald-500"
                              onClick={() => handleReceiveStock(purchase)}
                            >
                              <HugeiconsIcon icon={CheckmarkBadgeIcon} size={12} /> Receive Stock
                            </Button>
                          )}
                        </td>
                      </tr>
                    ))}
                    {purchases.length === 0 && (
                      <tr>
                        <td colSpan={6} className="py-8 text-center text-xs text-muted-foreground">
                          No purchase orders drafted.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>

              {/* Pagination controls */}
              <div className="flex flex-col items-center justify-between gap-4 border-t border-border/40 p-4 text-xs text-muted-foreground sm:flex-row print:hidden">
                <div className="flex items-center gap-2">
                  <span>Rows per page:</span>
                  <select
                    value={limit}
                    onChange={(e) => {
                      setLimit(parseInt(e.target.value));
                      setPage(1);
                    }}
                    className="rounded border border-border/40 bg-card p-1 text-xs text-foreground focus:outline-none"
                  >
                    <option value={5}>5</option>
                    <option value={10}>10</option>
                    <option value={20}>20</option>
                    <option value={50}>50</option>
                  </select>
                </div>
                <div className="flex items-center gap-1">
                  <Button
                    variant="outline"
                    size="icon"
                    onClick={() => setPage(Math.max(1, page - 1))}
                    disabled={page === 1}
                  >
                    <HugeiconsIcon icon={ArrowLeft01Icon} size={14} />
                  </Button>
                  <span className="px-2 font-medium">
                    Page {page} of {Math.ceil(totalCount / limit) || 1}
                  </span>
                  <Button
                    variant="outline"
                    size="icon"
                    onClick={() => setPage(Math.min(Math.ceil(totalCount / limit), page + 1))}
                    disabled={page >= Math.ceil(totalCount / limit)}
                  >
                    <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        )}
      </div>

      {/* Purchase Order Creator Dialog */}
      <Dialog open={isPurchaseModalOpen} onOpenChange={setIsPurchaseModalOpen}>
        <DialogContent className="font-sans sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>New Purchase Order</DialogTitle>
          </DialogHeader>
          <form onSubmit={handlePurchaseSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label>Supplier *</Label>
              <select
                className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                value={selectedSupplierId}
                onChange={(e) => setSelectedSupplierId(e.target.value)}
                required
              >
                <option value="" disabled>
                  Choose Supplier
                </option>
                {suppliers.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </div>

            {/* Item selector block */}
            <div className="space-y-3 rounded-lg border bg-muted/10 p-3">
              <h4 className="text-xs font-semibold text-muted-foreground uppercase">
                Add Inventory Items
              </h4>
              <div className="grid grid-cols-3 items-end gap-2">
                <div className="col-span-2">
                  <Label className="text-[10px]">Select Product</Label>
                  <select
                    className="mt-1 w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                    value={selectedProductId}
                    onChange={(e) => handleProductSelect(e.target.value)}
                  >
                    <option value="" disabled>
                      Choose Product
                    </option>
                    {products.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name} (Current Cost: {currencySymbol}
                        {p.cost.toFixed(2)})
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <Label className="text-[10px]">Purchase Cost</Label>
                  <Input
                    type="number"
                    step="0.01"
                    className="mt-1 border-border/40 bg-card"
                    value={customCost}
                    onChange={(e) => setCustomCost(e.target.value)}
                  />
                </div>
                <div>
                  <Label className="text-[10px]">Qty</Label>
                  <Input
                    type="number"
                    min="1"
                    className="mt-1 border-border/40 bg-card"
                    value={selectedProductQty}
                    onChange={(e) => setSelectedProductQty(parseInt(e.target.value) || 1)}
                  />
                </div>
              </div>
              <Button type="button" size="sm" className="w-full" onClick={handleAddPurchaseItem}>
                Add Item
              </Button>
            </div>

            {/* Current Purchase Items Grid */}
            <div className="space-y-2">
              <Label className="text-xs font-semibold">Ordered Products Cart</Label>
              <ScrollArea className="h-32 rounded-lg border bg-card p-2">
                {purchaseItems.map((item) => (
                  <div
                    key={item.product_id}
                    className="flex items-center justify-between border-b py-1.5 text-xs last:border-b-0"
                  >
                    <span className="font-medium">
                      {item.product_name} x {item.quantity} (Cost: {currencySymbol}
                      {item.cost})
                    </span>
                    <div className="flex items-center gap-2">
                      <span className="font-semibold">
                        {currencySymbol}
                        {item.total.toFixed(2)}
                      </span>
                      <button
                        type="button"
                        className="font-bold text-destructive hover:underline"
                        onClick={() =>
                          setPurchaseItems(
                            purchaseItems.filter((i) => i.product_id !== item.product_id),
                          )
                        }
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                ))}
                {purchaseItems.length === 0 && (
                  <p className="py-6 text-center text-xs text-muted-foreground">
                    No products drafted yet.
                  </p>
                )}
              </ScrollArea>
            </div>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsPurchaseModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Draft Purchase Order</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
