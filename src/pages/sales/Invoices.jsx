import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Search01Icon,
  PrinterIcon,
  Edit02Icon,
  ArrowLeft01Icon,
  ArrowRight01Icon
} from "@hugeicons/core-free-icons";

export default function Invoices() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [invoices, setInvoices] = useState([]);
  const [loading, setLoading] = useState(true);

  // Filter
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedStatus, setSelectedStatus] = useState("");

  // Pagination & Sorting State
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(10);
  const [sortBy, setSortBy] = useState("created_at");
  const [sortOrder, setSortOrder] = useState("DESC");

  // Modals
  const [isReceiptOpen, setIsReceiptOpen] = useState(false);
  const [printedInvoice, setPrintedInvoice] = useState(null);

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_invoices_filtered_cmd", {
        token,
        search: searchQuery || null,
        paymentStatus: selectedStatus || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setInvoices(paginated.invoices);
      setTotalCount(paginated.total_count);
    } catch (e) {
      console.error("Sales invoices load failed:", e);
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

  // Invoice Receipt Print opener
  const handlePrintReceipt = (inv) => {
    setPrintedInvoice(inv);
    setIsReceiptOpen(true);
  };

  const handleUpdateStatus = async (id, status) => {
    if (status === "Cancel" && !window.confirm("Are you sure you want to cancel this invoice? Doing so will return the items to stock and clear outstanding balances.")) {
      fetchData();
      return;
    }
    try {
      await invoke("update_invoice_status_cmd", { token, id, status });
      fetchData();
    } catch (e) {
      alert("Error updating invoice status: " + e);
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

  return (
    <div className="">
      

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
                  placeholder="Search by customer name or Invoice ID..."
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
                <option value="Pending">Pending</option>
                <option value="Paid">Paid</option>
                <option value="Cancel">Cancel</option>
              </select>
            </div>
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
                        Invoice ID {sortBy === "id" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("customer_name")}>
                        Customer Name {sortBy === "customer_name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Method</th>
                      <th className="p-4 font-medium text-right cursor-pointer select-none" onClick={() => toggleSort("total")}>
                        Net Total {sortBy === "total" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium text-right">Paid Amount</th>
                      <th className="p-4 font-medium text-right cursor-pointer select-none" onClick={() => toggleSort("due_amount")}>
                        Balance Due {sortBy === "due_amount" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium text-center">Status</th>
                      <th className="p-4 font-medium text-right">Actions</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {invoices.map((inv) => {
                      const due = inv.total - inv.amount_paid;
                      return (
                        <tr key={inv.id} className="hover:bg-muted/10">
                          <td className="p-4 font-mono text-xs text-muted-foreground">
                            {new Date(inv.created_at).toLocaleString()}
                          </td>
                          <td className="p-4 font-semibold font-mono text-xs text-primary">
                            #{inv.id.substring(0, 8).toUpperCase()}
                          </td>
                          <td className="p-4 font-medium">{inv.customer_name || "Walk-in Customer"}</td>
                          <td className="p-4 text-xs font-bold uppercase">{inv.payment_method}</td>
                          <td className="p-4 text-right font-mono font-bold">{currencySymbol}{inv.total.toFixed(2)}</td>
                          <td className="p-4 text-right font-mono text-emerald-600">{currencySymbol}{inv.amount_paid.toFixed(2)}</td>
                          <td className={`p-4 text-right font-mono ${due > 0 ? 'text-rose-600 font-bold' : 'text-muted-foreground'}`}>
                            {currencySymbol}{due.toFixed(2)}
                          </td>
                          <td className="p-4 text-center">
                            <select
                              value={inv.payment_status || "Pending"}
                              onChange={(e) => handleUpdateStatus(inv.id, e.target.value)}
                              className={`px-2.5 py-1 text-xs font-semibold rounded-full border bg-card text-foreground cursor-pointer focus:outline-none focus:ring-1 focus:ring-primary ${
                                inv.payment_status === "Paid"
                                  ? "bg-emerald-500/10 text-emerald-600 border-emerald-500/20"
                                  : inv.payment_status === "Cancel"
                                  ? "bg-rose-500/10 text-rose-600 border-rose-500/20"
                                  : "bg-amber-500/10 text-amber-600 border-amber-500/20"
                              }`}
                            >
                              <option value="Pending" className="bg-card text-foreground">Pending</option>
                              <option value="Paid" className="bg-card text-foreground">Paid</option>
                              <option value="Cancel" className="bg-card text-foreground">Cancel</option>
                            </select>
                          </td>
                          <td className="p-4 text-right">
                            <div className="flex justify-end gap-1.5">
                              {inv.payment_status !== "Cancel" && (
                                <Button variant="outline" size="sm" className="h-8 flex items-center gap-1 border-border/40 text-xs text-primary" onClick={() => navigate("/sales/pos", { state: { editInvoiceId: inv.id } })}>
                                  <HugeiconsIcon icon={Edit02Icon} size={12} /> Edit
                                </Button>
                              )}
                              <Button variant="outline" size="sm" className="h-8 flex items-center gap-1 border-border/40 text-xs" onClick={() => handlePrintReceipt(inv)}>
                                <HugeiconsIcon icon={PrinterIcon} size={12} /> Receipt Copy
                              </Button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                    {invoices.length === 0 && (
                      <tr>
                        <td colSpan={9} className="py-8 text-center text-muted-foreground text-xs">
                          No completed sales invoices found. Launch POS to compile invoices.
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

      {/* Copy of POS receipt printed copy */}
      {isReceiptOpen && printedInvoice && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 font-sans print:bg-white">
          <div className="bg-card w-full max-w-sm rounded-xl border p-5 shadow-2xl flex flex-col max-h-[90vh] print:border-none print:shadow-none print:max-h-none print:p-0 print:w-full print:max-w-none">
            {/* Header: Actions */}
            <div className="flex justify-between items-center mb-4 border-b pb-2 print:hidden">
              <h3 className="font-bold text-sm">Receipt Printed Copy</h3>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={() => window.print()} className="flex items-center gap-1">
                  <HugeiconsIcon icon={PrinterIcon} size={14} /> Print Copy
                </Button>
                <Button size="sm" onClick={() => setIsReceiptOpen(false)}>
                  Close
                </Button>
              </div>
            </div>

            <ScrollArea className="flex-1 bg-white text-black p-4 rounded-lg font-mono text-xs border border-dashed border-gray-300 print:border-none print:p-0">
              <div className="text-center space-y-1">
                <h2 className="text-base font-bold uppercase tracking-wide">Offline Billing Store</h2>
                <p className="text-[10px] text-gray-500">123 Local Street, City Central</p>
                <p className="text-[10px] text-gray-500">Phone: +1 (234) 567-890</p>
                <p className="text-[9px] font-bold bg-gray-200 text-gray-800 py-0.5 rounded tracking-widest mt-1">DUPLICATE RECEIPT</p>
              </div>

              <div className="my-4 border-t border-b border-gray-300 py-2 space-y-1 text-[10px]">
                <div><span className="font-bold">INVOICE:</span> #{printedInvoice.id.substring(0, 8).toUpperCase()}</div>
                <div><span className="font-bold">DATE:</span> {new Date(printedInvoice.created_at).toLocaleString()}</div>
                <div><span className="font-bold">CUSTOMER:</span> {printedInvoice.customer_name || "Walk-in"}</div>
                {printedInvoice.customer_phone && <div><span className="font-bold">PHONE:</span> {printedInvoice.customer_phone}</div>}
              </div>

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
