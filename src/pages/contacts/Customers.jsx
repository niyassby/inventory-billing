import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Search01Icon,
  Delete02Icon,
  Edit02Icon,
  UserGroupIcon,
  BookOpen01Icon,
  TargetIcon,
  ArrowLeft01Icon,
  ArrowRight01Icon
} from "@hugeicons/core-free-icons";

export default function Customers() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "accountant";
  const cannotDelete = userRole === "staff" || userRole === "accountant";

  // Data States
  const [customers, setCustomers] = useState([]);
  const [loading, setLoading] = useState(true);

  // Search
  const [searchQuery, setSearchQuery] = useState("");

  // Pagination & Sorting State
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(10);
  const [sortBy, setSortBy] = useState("name");
  const [sortOrder, setSortOrder] = useState("ASC");

  // Modals States
  const [isCustomerModalOpen, setIsCustomerModalOpen] = useState(false);
  const [editingCustomer, setEditingCustomer] = useState(null);
  const [customerForm, setCustomerForm] = useState({ name: "", email: "", phone: "", address: "", due_amount: 0 });

  // Fetch Methods
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_customers_filtered_cmd", {
        token,
        search: searchQuery || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setCustomers(paginated.customers);
      setTotalCount(paginated.total_count);
    } catch (e) {
      console.error("Failed to load customers data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token, sortBy, sortOrder, page, limit]);

  // Reset page to 1 when search changes
  useEffect(() => {
    setPage(1);
    fetchData();
  }, [searchQuery]);

  // Reset page to 1 when search changes
  useEffect(() => {

    const pressKeys = window.addEventListener("keydown", (e) => {
      console.log(e.key);
      if ((e.key === "Control" || e.key === "Command") && e.key === "N") {
        e.preventDefault();
        openAddCustomer()
      }
    });
    return () => {
      window.removeEventListener("keydown", pressKeys);
    }

  }, []);

  // Customer submit
  const handleCustomerSubmit = async (e) => {
    e.preventDefault();
    try {
      if (editingCustomer) {
        await invoke("update_customer_cmd", {
          token,
          id: editingCustomer.id,
          name: customerForm.name,
          email: customerForm.email || null,
          phone: customerForm.phone || null,
          address: customerForm.address || null,
          dueAmount: parseFloat(customerForm.due_amount) || 0
        });
      } else {
        await invoke("create_customer_cmd", {
          token,
          name: customerForm.name,
          email: customerForm.email || null,
          phone: customerForm.phone || null,
          address: customerForm.address || null
        });
      }
      setIsCustomerModalOpen(false);
      fetchData();
    } catch (e) {
      alert("Error saving customer: " + e);
    }
  };

  const handleCustomerDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this customer?")) return;
    try {
      await invoke("delete_customer_cmd", { token, id });
      fetchData();
    } catch (e) {
      alert("Error deleting customer: " + e);
    }
  };

  const openAddCustomer = () => {
    setEditingCustomer(null);
    setCustomerForm({ name: "", email: "", phone: "", address: "", due_amount: 0 });
    setIsCustomerModalOpen(true);
  };

  const openEditCustomer = (cust) => {
    setEditingCustomer(cust);
    setCustomerForm({
      name: cust.name,
      email: cust.email || "",
      phone: cust.phone || "",
      address: cust.address || "",
      due_amount: cust.due_amount
    });
    setIsCustomerModalOpen(true);
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
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">CRM & Contacts</h1>
          <p className="text-muted-foreground">Manage customer profiles, supplier balances, and track CRM deals pipeline.</p>
        </div>
        <div className="flex gap-2 print:hidden">
          <Button
            variant="default"
            onClick={() => navigate("/contacts/customers")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={UserGroupIcon} size={16} /> Customers CRM
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/contacts/suppliers")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BookOpen01Icon} size={16} /> Suppliers Ledger
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/contacts/leads")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={TargetIcon} size={16} /> CRM Leads
          </Button>
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex justify-between items-center gap-3">
          <div className="relative flex-1 max-w-sm">
            <HugeiconsIcon icon={Search01Icon} size={18} strokeWidth={2} className="absolute left-3 top-2.5 text-muted-foreground" />
            <Input
              placeholder="Search customers..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9 bg-card border-border/40"
            />
          </div>
          {!isReadOnly && (
            <Button onClick={openAddCustomer} className="flex items-center gap-1.5">
              <HugeiconsIcon icon={Add01Icon} size={16} /> Add Customer
            </Button>
          )}
        </div>

        {loading ? (
          <div className="flex h-[30vh] items-center justify-center">
            <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
          </div>
        ) : (
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm text-left border-collapse">
                  <thead>
                    <tr className="border-b border-border/40 text-muted-foreground text-xs uppercase bg-muted/20">
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("name")}>
                        Customer Name {sortBy === "name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Contact Details</th>
                      <th className="p-4 font-medium">Address</th>
                      <th className="p-4 font-medium text-right cursor-pointer select-none" onClick={() => toggleSort("due_amount")}>
                        Outstanding Debt {sortBy === "due_amount" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      {!isReadOnly && <th className="p-4 font-medium text-right">Actions</th>}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {customers.map((c) => (
                      <tr key={c.id} className="hover:bg-muted/10">
                        <td className="p-4 font-semibold">{c.name}</td>
                        <td className="p-4 text-xs">
                          <div>Phone: <span className="font-semibold">{c.phone || "-"}</span></div>
                          <div>Email: <span className="font-semibold">{c.email || "-"}</span></div>
                        </td>
                        <td className="p-4 text-xs text-muted-foreground">{c.address || "-"}</td>
                        <td className={`p-4 text-right font-mono font-bold ${c.due_amount > 0 ? 'text-rose-600' : 'text-muted-foreground'}`}>
                          {currencySymbol}{c.due_amount.toFixed(2)}
                        </td>
                        {!isReadOnly && (
                          <td className="p-4 text-right">
                            <div className="flex justify-end gap-1.5">
                              <Button variant="ghost" size="icon" className="h-8 w-8 text-primary" onClick={() => openEditCustomer(c)}>
                                <HugeiconsIcon icon={Edit02Icon} size={14} />
                              </Button>
                              {!cannotDelete && (
                                <Button variant="ghost" size="icon" className="h-8 w-8 text-destructive" onClick={() => handleCustomerDelete(c.id)}>
                                  <HugeiconsIcon icon={Delete02Icon} size={14} />
                                </Button>
                              )}
                            </div>
                          </td>
                        )}
                      </tr>
                    ))}
                    {customers.length === 0 && (
                      <tr>
                        <td colSpan={5} className="py-8 text-center text-muted-foreground text-xs">
                          No customers registered.
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
        )}
      </div>

      {/* Customer Modal */}
      <Dialog open={isCustomerModalOpen} onOpenChange={setIsCustomerModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>{editingCustomer ? "Edit Customer Info" : "Register Customer"}</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCustomerSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="c_name">Name *</Label>
              <Input
                id="c_name"
                value={customerForm.name}
                onChange={(e) => setCustomerForm({ ...customerForm, name: e.target.value })}
                required
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="c_phone">Phone</Label>
                <Input
                  id="c_phone"
                  value={customerForm.phone}
                  onChange={(e) => setCustomerForm({ ...customerForm, phone: e.target.value })}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="c_email">Email</Label>
                <Input
                  id="c_email"
                  type="email"
                  value={customerForm.email}
                  onChange={(e) => setCustomerForm({ ...customerForm, email: e.target.value })}
                />
              </div>
            </div>
            <div className="space-y-2">
              <Label htmlFor="c_addr">Address</Label>
              <Input
                id="c_addr"
                value={customerForm.address}
                onChange={(e) => setCustomerForm({ ...customerForm, address: e.target.value })}
              />
            </div>
            {editingCustomer && (
              <div className="space-y-2">
                <Label htmlFor="c_due">Outstanding Debt Amount ({currencySymbol})</Label>
                <Input
                  id="c_due"
                  type="number"
                  step="0.01"
                  value={customerForm.due_amount}
                  onChange={(e) => setCustomerForm({ ...customerForm, due_amount: e.target.value })}
                />
              </div>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsCustomerModalOpen(false)}>Cancel</Button>
              <Button type="submit">Save Customer</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
