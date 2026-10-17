import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/stores/app-store";
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
  ArrowRight01Icon,
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
    if (
      status === "Cancel" &&
      !window.confirm(
        "Are you sure you want to cancel this invoice? Doing so will return the items to stock and clear outstanding balances.",
      )
    ) {
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
                  placeholder="Search by customer name or Invoice ID..."
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
                <option value="Pending">Pending</option>
                <option value="Paid">Paid</option>
                <option value="Cancel">Cancel</option>
              </select>
            </div>
          </div>

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
                        Invoice ID {sortBy === "id" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("customer_name")}
                      >
                        Customer Name{" "}
                        {sortBy === "customer_name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Method</th>
                      <th
                        className="cursor-pointer p-4 text-right font-medium select-none"
                        onClick={() => toggleSort("total")}
                      >
                        Net Total {sortBy === "total" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 text-right font-medium">Paid Amount</th>
                      <th
                        className="cursor-pointer p-4 text-right font-medium select-none"
                        onClick={() => toggleSort("due_amount")}
                      >
                        Balance Due{" "}
                        {sortBy === "due_amount" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 text-center font-medium">Status</th>
                      <th className="p-4 text-right font-medium">Actions</th>
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
                          <td className="p-4 font-mono text-xs font-semibold text-primary">
                            #{inv.id.substring(0, 8).toUpperCase()}
                          </td>
                          <td className="p-4 font-medium">
                            {inv.customer_name || "Walk-in Customer"}
                          </td>
                          <td className="p-4 text-xs font-bold uppercase">{inv.payment_method}</td>
                          <td className="p-4 text-right font-mono font-bold">
                            {currencySymbol}
                            {inv.total.toFixed(2)}
                          </td>
                          <td className="p-4 text-right font-mono text-emerald-600">
                            {currencySymbol}
                            {inv.amount_paid.toFixed(2)}
                          </td>
                          <td
                            className={`p-4 text-right font-mono ${due > 0 ? "font-bold text-rose-600" : "text-muted-foreground"}`}
                          >
                            {currencySymbol}
                            {due.toFixed(2)}
                          </td>
                          <td className="p-4 text-center">
                            <select
                              value={inv.payment_status || "Pending"}
                              onChange={(e) => handleUpdateStatus(inv.id, e.target.value)}
                              className={`cursor-pointer rounded-full border bg-card px-2.5 py-1 text-xs font-semibold text-foreground focus:ring-1 focus:ring-primary focus:outline-none ${
                                inv.payment_status === "Paid"
                                  ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-600"
                                  : inv.payment_status === "Cancel"
                                    ? "border-rose-500/20 bg-rose-500/10 text-rose-600"
                                    : "border-amber-500/20 bg-amber-500/10 text-amber-600"
                              }`}
                            >
                              <option value="Pending" className="bg-card text-foreground">
                                Pending
                              </option>
                              <option value="Paid" className="bg-card text-foreground">
                                Paid
                              </option>
                              <option value="Cancel" className="bg-card text-foreground">
                                Cancel
                              </option>
                            </select>
                          </td>
                          <td className="p-4 text-right">
                            <div className="flex justify-end gap-1.5">
                              {inv.payment_status !== "Cancel" && (
                                <Button
                                  variant="outline"
                                  size="sm"
                                  className="flex h-8 items-center gap-1 border-border/40 text-xs text-primary"
                                  onClick={() =>
                                    navigate("/sales/pos", { state: { editInvoiceId: inv.id } })
                                  }
                                >
                                  <HugeiconsIcon icon={Edit02Icon} size={12} /> Edit
                                </Button>
                              )}
                              <Button
                                variant="outline"
                                size="sm"
                                className="flex h-8 items-center gap-1 border-border/40 text-xs"
                                onClick={() => handlePrintReceipt(inv)}
                              >
                                <HugeiconsIcon icon={PrinterIcon} size={12} /> Receipt Copy
                              </Button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                    {invoices.length === 0 && (
                      <tr>
                        <td colSpan={9} className="py-8 text-center text-xs text-muted-foreground">
                          No completed sales invoices found. Launch POS to compile invoices.
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
        </div>
      )}

      {/* Copy of POS receipt printed copy */}
      {isReceiptOpen && printedInvoice && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 font-sans print:bg-white">
          <div className="flex max-h-[90vh] w-full max-w-sm flex-col rounded-xl border bg-card p-5 shadow-2xl print:max-h-none print:w-full print:max-w-none print:border-none print:p-0 print:shadow-none">
            {/* Header: Actions */}
            <div className="mb-4 flex items-center justify-between border-b pb-2 print:hidden">
              <h3 className="text-sm font-bold">Receipt Printed Copy</h3>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => window.print()}
                  className="flex items-center gap-1"
                >
                  <HugeiconsIcon icon={PrinterIcon} size={14} /> Print Copy
                </Button>
                <Button size="sm" onClick={() => setIsReceiptOpen(false)}>
                  Close
                </Button>
              </div>
            </div>

            <ScrollArea className="flex-1 rounded-lg border border-dashed border-gray-300 bg-white p-4 font-mono text-xs text-black print:border-none print:p-0">
              <div className="space-y-1 text-center">
                <h2 className="text-base font-bold tracking-wide uppercase">
                  Offline Billing Store
                </h2>
                <p className="text-[10px] text-gray-500">123 Local Street, City Central</p>
                <p className="text-[10px] text-gray-500">Phone: +1 (234) 567-890</p>
                <p className="mt-1 rounded bg-gray-200 py-0.5 text-[9px] font-bold tracking-widest text-gray-800">
                  DUPLICATE RECEIPT
                </p>
              </div>

              <div className="my-4 space-y-1 border-t border-b border-gray-300 py-2 text-[10px]">
                <div>
                  <span className="font-bold">INVOICE:</span> #
                  {printedInvoice.id.substring(0, 8).toUpperCase()}
                </div>
                <div>
                  <span className="font-bold">DATE:</span>{" "}
                  {new Date(printedInvoice.created_at).toLocaleString()}
                </div>
                <div>
                  <span className="font-bold">CUSTOMER:</span>{" "}
                  {printedInvoice.customer_name || "Walk-in"}
                </div>
                {printedInvoice.customer_phone && (
                  <div>
                    <span className="font-bold">PHONE:</span> {printedInvoice.customer_phone}
                  </div>
                )}
              </div>

              <table className="w-full border-collapse text-left text-[10px]">
                <thead>
                  <tr className="border-b border-gray-400 font-bold">
                    <th className="pb-1.5">Item</th>
                    <th className="pb-1.5 text-center">Qty</th>
                    <th className="pb-1.5 text-right">Price</th>
                    <th className="pb-1.5 text-right">Total</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-200">
                  {(Array.isArray(printedInvoice.items)
                    ? printedInvoice.items
                    : typeof printedInvoice.items === "string"
                      ? JSON.parse(printedInvoice.items)
                      : []
                  ).map((item) => (
                    <tr key={item.product_id}>
                      <td className="max-w-[120px] truncate py-1.5">{item.product_name}</td>
                      <td className="py-1.5 text-center font-bold">{item.quantity}</td>
                      <td className="py-1.5 text-right">
                        {currencySymbol}
                        {(item.unit_price || item.price || 0).toFixed(2)}
                      </td>
                      <td className="py-1.5 text-right">
                        {currencySymbol}
                        {item.total.toFixed(2)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>

              <div className="mt-4 space-y-1.5 border-t border-gray-300 pt-2 text-[10px]">
                <div className="flex justify-between">
                  <span>Subtotal</span>
                  <span>
                    {currencySymbol}
                    {printedInvoice.subtotal.toFixed(2)}
                  </span>
                </div>
                {printedInvoice.discount > 0 && (
                  <div className="flex justify-between text-gray-600">
                    <span>Discount</span>
                    <span>
                      -{currencySymbol}
                      {printedInvoice.discount.toFixed(2)}
                    </span>
                  </div>
                )}
                {printedInvoice.tax > 0 && (
                  <div className="flex justify-between">
                    <span>Tax</span>
                    <span>
                      {currencySymbol}
                      {printedInvoice.tax.toFixed(2)}
                    </span>
                  </div>
                )}
                <div className="flex justify-between border-t border-gray-400 pt-1.5 text-sm font-bold">
                  <span>Net Amount</span>
                  <span>
                    {currencySymbol}
                    {printedInvoice.total.toFixed(2)}
                  </span>
                </div>
                <div className="flex justify-between pt-1 text-[10px] text-gray-500">
                  <span>Amount Paid ({printedInvoice.payment_method})</span>
                  <span>
                    {currencySymbol}
                    {printedInvoice.amount_paid.toFixed(2)}
                  </span>
                </div>
                {printedInvoice.total > printedInvoice.amount_paid && (
                  <div className="flex justify-between font-bold text-rose-600">
                    <span>Balance Due</span>
                    <span>
                      {currencySymbol}
                      {(printedInvoice.total - printedInvoice.amount_paid).toFixed(2)}
                    </span>
                  </div>
                )}
              </div>

              <div className="mt-6 space-y-1 border-t border-dashed border-gray-400 pt-4 text-center text-[10px]">
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
