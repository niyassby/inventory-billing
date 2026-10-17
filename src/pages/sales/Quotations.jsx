import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
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
  Add01Icon,
  Search01Icon,
  CheckmarkBadgeIcon,
  ArrowLeft01Icon,
  ArrowRight01Icon
} from "@hugeicons/core-free-icons";

export default function Quotations() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [quotations, setQuotations] = useState([]);
  const [customers, setCustomers] = useState([]);
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

  // Quotation Creator Modal
  const [isQuoteModalOpen, setIsQuoteModalOpen] = useState(false);
  const [selectedCustomerId, setSelectedCustomerId] = useState("");
  const [quoteItems, setQuoteItems] = useState([]);
  const [selectedProductId, setSelectedProductId] = useState("");
  const [selectedProductQty, setSelectedProductQty] = useState(1);

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_quotations_filtered_cmd", {
        token,
        search: searchQuery || null,
        status: selectedStatus || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setQuotations(paginated.quotations);
      setTotalCount(paginated.total_count);

      const custList = await invoke("list_customers_cmd", { token });
      setCustomers(custList);
      const prodList = await invoke("list_products_cmd", { token });
      setProducts(prodList);
    } catch (e) {
      console.error("Sales quotations load failed:", e);
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

  // Convert Quotation to Invoice (One-click!)
  const handleConvertQuote = async (quote) => {
    if (!window.confirm("Convert this quotation to a completed invoice? This updates inventory stock counts.")) return;

    // Parse quotation items
    let parsedItems = [];
    try {
      parsedItems = JSON.parse(quote.items);
    } catch (e) {
      console.error("Parse quotation items error:", e);
      return alert("Invalid quotation items format");
    }

    const items = parsedItems.map((item) => ({
      id: "",
      invoice_id: "",
      product_id: item.product_id,
      product_name: item.product_name,
      quantity: item.quantity,
      unit_price: item.price || item.unit_price,
      total: item.total
    }));

    try {
      const invoice = await invoke("create_invoice_cmd", {
        token,
        customerId: quote.customer_id,
        subtotal: quote.total, // approx subtotal
        discount: 0.0,
        tax: 0.0,
        total: quote.total,
        amountPaid: quote.total, // assume paid in cash fully on convert
        paymentMethod: "CASH",
        items
      });
      alert("Successfully converted quotation to Invoice #" + invoice.id.substring(0, 8));
      navigate("/sales/invoices");
    } catch (e) {
      alert("Failed to convert quotation: " + e);
    }
  };

  // Add Item to Quotation builder
  const handleAddQuoteItem = () => {
    if (!selectedProductId) return;
    const prod = products.find((p) => p.id === selectedProductId);
    if (!prod) return;

    const existing = quoteItems.find((item) => item.product_id === prod.id);
    if (existing) {
      setQuoteItems(
        quoteItems.map((item) =>
          item.product_id === prod.id
            ? { ...item, quantity: item.quantity + selectedProductQty, total: (item.quantity + selectedProductQty) * item.price }
            : item
        )
      );
    } else {
      setQuoteItems([
        ...quoteItems,
        {
          product_id: prod.id,
          product_name: prod.name,
          sku: prod.sku,
          quantity: selectedProductQty,
          price: prod.price,
          cost: prod.cost,
          total: prod.price * selectedProductQty
        }
      ]);
    }
    setSelectedProductId("");
    setSelectedProductQty(1);
  };

  // Submit Quotation
  const handleQuoteSubmit = async (e) => {
    e.preventDefault();
    if (quoteItems.length === 0) return alert("Quotation is empty!");
    if (!selectedCustomerId) return alert("Select a customer!");

    const total = quoteItems.reduce((acc, item) => acc + item.total, 0);

    const items = quoteItems.map((item) => ({
      id: "",
      quotation_id: "",
      product_id: item.product_id,
      product_name: item.product_name || null,
      quantity: parseFloat(item.quantity),
      unit_price: parseFloat(item.price || item.unit_price || 0),
      total: parseFloat(item.total),
    }));

    try {
      await invoke("create_quotation_cmd", {
        token,
        customerId: selectedCustomerId,
        total,
        status: "PENDING",
        items
      });
      setIsQuoteModalOpen(false);
      setQuoteItems([]);
      setSelectedCustomerId("");
      fetchData();
    } catch (e) {
      alert("Error saving quotation: " + e);
    }
  };

  return (
    <div>

      {loading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-3">
            <div className="flex flex-1 gap-2 w-full sm:w-auto">
              <div className="relative flex-1 max-w-sm">
                <HugeiconsIcon icon={Search01Icon} size={18} strokeWidth={2} className="absolute left-3 top-2.5 text-muted-foreground" />
                <Input
                  placeholder="Search quotations by customer name..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-9 bg-card border-border/40"
                />
              </div>
              <select
                value={selectedStatus}
                onChange={(e) => setSelectedStatus(e.target.value)}
                className="p-2 border rounded-md text-xs bg-card border-border/40 text-foreground max-w-xs focus:outline-none"
              >
                <option value="">All Statuses</option>
                <option value="PENDING">Pending</option>
                <option value="CONVERTED">Converted</option>
              </select>
            </div>
            <Button onClick={() => setIsQuoteModalOpen(true)} className="flex items-center gap-1.5">
              <HugeiconsIcon icon={Add01Icon} size={16} /> New Quotation
            </Button>
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm text-left border-collapse">
                  <thead>
                    <tr className="border-b border-border/40 text-muted-foreground text-xs uppercase bg-muted/20">
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("created_at")}>
                        Date {sortBy === "created_at" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("id")}>
                        Quotation ID {sortBy === "id" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("customer_name")}>
                        Customer Name {sortBy === "customer_name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium text-right cursor-pointer select-none" onClick={() => toggleSort("total")}>
                        Estimated Total {sortBy === "total" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium text-center cursor-pointer select-none" onClick={() => toggleSort("status")}>
                        Status {sortBy === "status" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium text-right">Actions</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {quotations.map((q) => (
                      <tr key={q.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs text-muted-foreground">
                          {new Date(q.created_at).toLocaleString()}
                        </td>
                        <td className="p-4 font-semibold font-mono text-xs">
                          #{q.id.substring(0, 8).toUpperCase()}
                        </td>
                        <td className="p-4 font-medium">{q.customer_name}</td>
                        <td className="p-4 text-right font-mono font-bold">{currencySymbol}{q.total.toFixed(2)}</td>
                        <td className="p-4 text-center">
                          <span className={`px-2 py-0.5 text-[10px] font-semibold rounded-full border ${
                            q.status === "PENDING"
                              ? "bg-amber-500/10 text-amber-600 border-amber-500/20"
                              : "bg-emerald-500/10 text-emerald-600 border-emerald-500/20"
                          }`}>
                            {q.status}
                          </span>
                        </td>
                        <td className="p-4 text-right">
                          {q.status === "PENDING" && (
                            <Button
                              size="sm"
                              variant="outline"
                              className="h-8 border-emerald-500/20 text-emerald-600 hover:bg-emerald-500/10 text-xs flex items-center gap-1 ml-auto"
                              onClick={() => handleConvertQuote(q)}
                            >
                              <HugeiconsIcon icon={CheckmarkBadgeIcon} size={12} /> Convert to Invoice
                            </Button>
                          )}
                        </td>
                      </tr>
                    ))}
                    {quotations.length === 0 && (
                      <tr>
                        <td colSpan={6} className="py-8 text-center text-muted-foreground text-xs">
                          No quotations active. Create customer quotes to display here.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>

              {/* Pagination controls */}
              <div className="p-4 border-t border-border/40 flex flex-col sm:flex-row justify-between items-center gap-4 text-xs text-muted-foreground print:hidden">
                <div className="flex items-center gap-2">
                  <span>Rows per page:</span>
                  <select
                    value={limit}
                    onChange={(e) => {
                      setLimit(parseInt(e.target.value));
                      setPage(1);
                    }}
                    className="p-1 border rounded bg-card border-border/40 text-foreground text-xs focus:outline-none"
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
                  <span className="px-2 font-medium">Page {page} of {Math.ceil(totalCount / limit) || 1}</span>
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
        </div>
      )}

      {/* Quotation Creator Dialog */}
      <Dialog open={isQuoteModalOpen} onOpenChange={setIsQuoteModalOpen}>
        <DialogContent className="sm:max-w-lg font-sans">
          <DialogHeader>
            <DialogTitle>New Sales Quotation</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleQuoteSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label>Customer *</Label>
              <select
                className="w-full p-2 border rounded-md text-sm bg-card border-border/40 text-foreground"
                value={selectedCustomerId}
                onChange={(e) => setSelectedCustomerId(e.target.value)}
                required
              >
                <option value="" disabled>Choose Customer</option>
                {customers.map((c) => (
                  <option key={c.id} value={c.id}>{c.name}</option>
                ))}
              </select>
            </div>

            {/* Item selector block */}
            <div className="border p-3 rounded-lg bg-muted/10 space-y-3">
              <h4 className="text-xs font-semibold text-muted-foreground uppercase">Add Quotation Items</h4>
              <div className="grid grid-cols-3 gap-2 items-end">
                <div className="col-span-2">
                  <Label className="text-[10px]">Select Product</Label>
                  <select
                    className="w-full mt-1 p-2 border rounded-md text-sm bg-card border-border/40 text-foreground"
                    value={selectedProductId}
                    onChange={(e) => setSelectedProductId(e.target.value)}
                  >
                    <option value="" disabled>Choose Product</option>
                    {products.map((p) => (
                      <option key={p.id} value={p.id}>{p.name} ({currencySymbol}{p.price.toFixed(2)})</option>
                    ))}
                  </select>
                </div>
                <div>
                  <Label className="text-[10px]">Qty</Label>
                  <Input
                    type="number"
                    min="1"
                    className="mt-1 bg-card border-border/40"
                    value={selectedProductQty}
                    onChange={(e) => setSelectedProductQty(parseInt(e.target.value) || 1)}
                  />
                </div>
              </div>
              <Button type="button" size="sm" className="w-full" onClick={handleAddQuoteItem}>
                Add Selected Item
              </Button>
            </div>

            {/* Current Quote Items Grid */}
            <div className="space-y-2">
              <Label className="text-xs font-semibold">Quotation Cart Items</Label>
              <ScrollArea className="h-32 border rounded-lg p-2 bg-card">
                {quoteItems.map((item) => (
                  <div key={item.product_id} className="flex justify-between items-center text-xs py-1.5 border-b last:border-b-0">
                    <span className="font-medium">{item.product_name} x {item.quantity}</span>
                    <div className="flex gap-2 items-center">
                      <span className="font-semibold">{currencySymbol}{item.total.toFixed(2)}</span>
                      <button
                        type="button"
                        className="text-destructive font-bold hover:underline"
                        onClick={() => setQuoteItems(quoteItems.filter((i) => i.product_id !== item.product_id))}
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                ))}
                {quoteItems.length === 0 && (
                  <p className="text-xs text-center text-muted-foreground py-6">No items added to quote.</p>
                )}
              </ScrollArea>
            </div>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsQuoteModalOpen(false)}>Cancel</Button>
              <Button type="submit">Create Quotation</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
