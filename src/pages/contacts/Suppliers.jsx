import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
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
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";

export default function Suppliers() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "accountant";
  const cannotDelete = userRole === "staff" || userRole === "accountant";

  // Data States
  const [suppliers, setSuppliers] = useState([]);
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
  const [isSupplierModalOpen, setIsSupplierModalOpen] = useState(false);
  const [editingSupplier, setEditingSupplier] = useState(null);
  const [supplierForm, setSupplierForm] = useState({
    name: "",
    email: "",
    phone: "",
    address: "",
    balance: 0,
  });

  // Fetch Methods
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_suppliers_filtered_cmd", {
        token,
        search: searchQuery || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setSuppliers(paginated.suppliers);
      setTotalCount(paginated.total_count);
    } catch (e) {
      console.error("Failed to load suppliers data:", e);
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

  // Supplier submit
  const handleSupplierSubmit = async (e) => {
    e.preventDefault();
    try {
      if (editingSupplier) {
        await invoke("update_supplier_cmd", {
          token,
          id: editingSupplier.id,
          name: supplierForm.name,
          email: supplierForm.email || null,
          phone: supplierForm.phone || null,
          address: supplierForm.address || null,
          balance: parseFloat(supplierForm.balance) || 0,
        });
      } else {
        await invoke("create_supplier_cmd", {
          token,
          name: supplierForm.name,
          email: supplierForm.email || null,
          phone: supplierForm.phone || null,
          address: supplierForm.address || null,
        });
      }
      setIsSupplierModalOpen(false);
      fetchData();
    } catch (e) {
      alert("Error saving supplier: " + e);
    }
  };

  const handleSupplierDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this supplier?")) return;
    try {
      await invoke("delete_supplier_cmd", { token, id });
      fetchData();
    } catch (e) {
      alert("Error deleting supplier: " + e);
    }
  };

  const openAddSupplier = () => {
    setEditingSupplier(null);
    setSupplierForm({ name: "", email: "", phone: "", address: "", balance: 0 });
    setIsSupplierModalOpen(true);
  };

  const openEditSupplier = (sup) => {
    setEditingSupplier(sup);
    setSupplierForm({
      name: sup.name,
      email: sup.email || "",
      phone: sup.phone || "",
      address: sup.address || "",
      balance: sup.balance,
    });
    setIsSupplierModalOpen(true);
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
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">CRM & Contacts</h1>
          <p className="text-muted-foreground">
            Manage customer profiles, supplier balances, and track CRM deals pipeline.
          </p>
        </div>
        <div className="flex gap-2 print:hidden">
          <Button
            variant="outline"
            onClick={() => navigate("/contacts/customers")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={UserGroupIcon} size={16} /> Customers CRM
          </Button>
          <Button
            variant="default"
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
        <div className="flex items-center justify-between gap-3">
          <div className="relative max-w-sm flex-1">
            <HugeiconsIcon
              icon={Search01Icon}
              size={18}
              strokeWidth={2}
              className="absolute top-2.5 left-3 text-muted-foreground"
            />
            <Input
              placeholder="Search suppliers..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="border-border/40 bg-card pl-9"
            />
          </div>
          {!isReadOnly && (
            <Button onClick={openAddSupplier} className="flex items-center gap-1.5">
              <HugeiconsIcon icon={Add01Icon} size={16} /> Add Supplier
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
                <table className="w-full border-collapse text-left text-sm">
                  <thead>
                    <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("name")}
                      >
                        Supplier Name {sortBy === "name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Contact Details</th>
                      <th className="p-4 font-medium">Address</th>
                      <th
                        className="cursor-pointer p-4 text-right font-medium select-none"
                        onClick={() => toggleSort("balance")}
                      >
                        Account Balance{" "}
                        {sortBy === "balance" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      {!isReadOnly && <th className="p-4 text-right font-medium">Actions</th>}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {suppliers.map((sup) => (
                      <tr key={sup.id} className="hover:bg-muted/10">
                        <td className="p-4 font-semibold">{sup.name}</td>
                        <td className="p-4 text-xs">
                          <div>
                            Phone: <span className="font-semibold">{sup.phone || "-"}</span>
                          </div>
                          <div>
                            Email: <span className="font-semibold">{sup.email || "-"}</span>
                          </div>
                        </td>
                        <td className="p-4 text-xs text-muted-foreground">{sup.address || "-"}</td>
                        <td
                          className={`p-4 text-right font-mono font-bold ${sup.balance > 0 ? "text-emerald-600" : "text-muted-foreground"}`}
                        >
                          {currencySymbol}
                          {sup.balance.toFixed(2)}
                        </td>
                        {!isReadOnly && (
                          <td className="p-4 text-right">
                            <div className="flex justify-end gap-1.5">
                              <Button
                                variant="ghost"
                                size="icon"
                                className="h-8 w-8 text-primary"
                                onClick={() => openEditSupplier(sup)}
                              >
                                <HugeiconsIcon icon={Edit02Icon} size={14} />
                              </Button>
                              {!cannotDelete && (
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-8 w-8 text-destructive"
                                  onClick={() => handleSupplierDelete(sup.id)}
                                >
                                  <HugeiconsIcon icon={Delete02Icon} size={14} />
                                </Button>
                              )}
                            </div>
                          </td>
                        )}
                      </tr>
                    ))}
                    {suppliers.length === 0 && (
                      <tr>
                        <td colSpan={5} className="py-8 text-center text-xs text-muted-foreground">
                          No suppliers registered.
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

      {/* Supplier Modal */}
      <Dialog open={isSupplierModalOpen} onOpenChange={setIsSupplierModalOpen}>
        <DialogContent className="font-sans sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {editingSupplier ? "Edit Supplier Info" : "Register Supplier"}
            </DialogTitle>
          </DialogHeader>
          <form onSubmit={handleSupplierSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="s_name">Company / Supplier Name *</Label>
              <Input
                id="s_name"
                value={supplierForm.name}
                onChange={(e) => setSupplierForm({ ...supplierForm, name: e.target.value })}
                required
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="s_phone">Phone</Label>
                <Input
                  id="s_phone"
                  value={supplierForm.phone}
                  onChange={(e) => setSupplierForm({ ...supplierForm, phone: e.target.value })}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="s_email">Email</Label>
                <Input
                  id="s_email"
                  type="email"
                  value={supplierForm.email}
                  onChange={(e) => setSupplierForm({ ...supplierForm, email: e.target.value })}
                />
              </div>
            </div>
            <div className="space-y-2">
              <Label htmlFor="s_addr">Address</Label>
              <Input
                id="s_addr"
                value={supplierForm.address}
                onChange={(e) => setSupplierForm({ ...supplierForm, address: e.target.value })}
              />
            </div>
            {editingSupplier && (
              <div className="space-y-2">
                <Label htmlFor="s_bal">Account Balance ({currencySymbol})</Label>
                <Input
                  id="s_bal"
                  type="number"
                  step="0.01"
                  value={supplierForm.balance}
                  onChange={(e) => setSupplierForm({ ...supplierForm, balance: e.target.value })}
                />
              </div>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsSupplierModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Save Supplier</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
