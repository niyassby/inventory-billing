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
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Delete02Icon,
  BookOpen02Icon,
  ShuffleIcon,
  DollarSignIcon,
  BadgePercentIcon,
  Search01Icon,
  ArrowLeft01Icon,
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";

export default function Expenses() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "staff";
  const cannotDelete = userRole === "staff";

  // Data States
  const [expenses, setExpenses] = useState([]);
  const [loading, setLoading] = useState(true);

  // Filters State
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState("");

  // Pagination & Sorting State
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(10);
  const [sortBy, setSortBy] = useState("date");
  const [sortOrder, setSortOrder] = useState("DESC");

  // Modals States
  const [isExpenseModalOpen, setIsExpenseModalOpen] = useState(false);
  const [expenseForm, setExpenseForm] = useState({
    title: "",
    category: "Rent",
    amount: 0,
    date: new Date().toISOString().split("T")[0],
    description: "",
  });

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_expenses_filtered_cmd", {
        token,
        search: searchQuery || null,
        category: selectedCategory || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setExpenses(paginated.expenses);
      setTotalCount(paginated.total_count);
    } catch (e) {
      console.error("Failed to load expenses data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token, selectedCategory, sortBy, sortOrder, page, limit]);

  // Reset page to 1 when search or category changes
  useEffect(() => {
    setPage(1);
    fetchData();
  }, [searchQuery, selectedCategory]);

  // Expense Submit
  const handleExpenseSubmit = async (e) => {
    e.preventDefault();
    try {
      await invoke("create_expense_cmd", {
        token,
        title: expenseForm.title,
        category: expenseForm.category,
        amount: parseFloat(expenseForm.amount),
        date: expenseForm.date,
        description: expenseForm.description || null,
      });
      setIsExpenseModalOpen(false);
      setExpenseForm({
        title: "",
        category: "Rent",
        amount: 0,
        date: new Date().toISOString().split("T")[0],
        description: "",
      });
      fetchData();
    } catch (e) {
      alert("Error creating expense: " + e);
    }
  };

  const handleExpenseDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this expense?")) return;
    try {
      await invoke("delete_expense_cmd", { token, id });
      fetchData();
    } catch (e) {
      alert("Error deleting expense: " + e);
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
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Ledger & Accounting</h1>
          <p className="text-muted-foreground">
            Manage chart of accounts, draft balancing journal entries, log expenses, and configure
            tax rules.
          </p>
        </div>
        <div className="flex flex-wrap gap-2 print:hidden">
          <Button
            variant="outline"
            onClick={() => navigate("/accounting/chart-of-accounts")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BookOpen02Icon} size={16} /> Chart of Accounts
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/accounting/journals")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ShuffleIcon} size={16} /> Journal Entries
          </Button>
          <Button
            variant="default"
            onClick={() => navigate("/accounting/expenses")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={DollarSignIcon} size={16} /> Expense Logger
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/accounting/taxes")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BadgePercentIcon} size={16} /> Tax Rules
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
                placeholder="Search by expense title or description..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="border-border/40 bg-card pl-9"
              />
            </div>
            <select
              value={selectedCategory}
              onChange={(e) => setSelectedCategory(e.target.value)}
              className="max-w-xs rounded-md border border-border/40 bg-card p-2 text-xs text-foreground focus:outline-none"
            >
              <option value="">All Categories</option>
              <option value="Rent">Rent</option>
              <option value="Utilities">Utilities</option>
              <option value="Salary">Salary</option>
              <option value="Marketing">Marketing</option>
              <option value="Other">Other Operational</option>
            </select>
          </div>
          {!isReadOnly && (
            <Button
              onClick={() => setIsExpenseModalOpen(true)}
              className="flex shrink-0 items-center gap-1.5"
            >
              <HugeiconsIcon icon={Add01Icon} size={16} /> Log Expense
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
                        onClick={() => toggleSort("date")}
                      >
                        Date {sortBy === "date" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("title")}
                      >
                        Expense Title {sortBy === "title" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 font-medium select-none"
                        onClick={() => toggleSort("category")}
                      >
                        Category {sortBy === "category" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th
                        className="cursor-pointer p-4 text-right font-medium select-none"
                        onClick={() => toggleSort("amount")}
                      >
                        Amount {sortBy === "amount" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Description</th>
                      {!cannotDelete && <th className="p-4 text-right font-medium">Actions</th>}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {expenses.map((e) => (
                      <tr key={e.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs text-muted-foreground">{e.date}</td>
                        <td className="p-4 font-semibold">{e.title}</td>
                        <td className="p-4 text-xs font-semibold uppercase">{e.category}</td>
                        <td className="p-4 text-right font-mono font-bold text-rose-600">
                          {currencySymbol}
                          {e.amount.toFixed(2)}
                        </td>
                        <td className="p-4 text-xs text-muted-foreground">
                          {e.description || "-"}
                        </td>
                        {!cannotDelete && (
                          <td className="p-4 text-right">
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-8 w-8 text-destructive"
                              onClick={() => handleExpenseDelete(e.id)}
                            >
                              <HugeiconsIcon icon={Delete02Icon} size={14} />
                            </Button>
                          </td>
                        )}
                      </tr>
                    ))}
                    {expenses.length === 0 && (
                      <tr>
                        <td
                          colSpan={cannotDelete ? 5 : 6}
                          className="py-8 text-center text-xs text-muted-foreground"
                        >
                          No operational expenses logged.
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

      {/* Log Expense Modal */}
      <Dialog open={isExpenseModalOpen} onOpenChange={setIsExpenseModalOpen}>
        <DialogContent className="font-sans sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Log Store Expense</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleExpenseSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="exp_title">Expense Title *</Label>
              <Input
                id="exp_title"
                placeholder="e.g. Electric bill, Office stationery"
                value={expenseForm.title}
                onChange={(e) => setExpenseForm({ ...expenseForm, title: e.target.value })}
                required
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="exp_cat">Category *</Label>
                <select
                  id="exp_cat"
                  className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                  value={expenseForm.category}
                  onChange={(e) => setExpenseForm({ ...expenseForm, category: e.target.value })}
                  required
                >
                  <option value="Rent">Rent</option>
                  <option value="Utilities">Utilities</option>
                  <option value="Salary">Salary</option>
                  <option value="Marketing">Marketing</option>
                  <option value="Other">Other Operational</option>
                </select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="exp_amt">Amount *</Label>
                <Input
                  id="exp_amt"
                  type="number"
                  step="0.01"
                  value={expenseForm.amount}
                  onChange={(e) => setExpenseForm({ ...expenseForm, amount: e.target.value })}
                  required
                />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="exp_date">Date *</Label>
                <Input
                  id="exp_date"
                  type="date"
                  value={expenseForm.date}
                  onChange={(e) => setExpenseForm({ ...expenseForm, date: e.target.value })}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="exp_desc">Notes</Label>
                <Input
                  id="exp_desc"
                  placeholder="Notes/details"
                  value={expenseForm.description}
                  onChange={(e) => setExpenseForm({ ...expenseForm, description: e.target.value })}
                />
              </div>
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsExpenseModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Log Expense</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
