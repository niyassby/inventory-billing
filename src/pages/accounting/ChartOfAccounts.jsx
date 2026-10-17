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
  BookOpen02Icon,
  ShuffleIcon,
  DollarSignIcon,
  BadgePercentIcon,
} from "@hugeicons/core-free-icons";

export default function ChartOfAccounts() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "staff";

  // Data States
  const [accounts, setAccounts] = useState([]);
  const [loading, setLoading] = useState(true);

  // Modals States
  const [isAccountModalOpen, setIsAccountModalOpen] = useState(false);
  const [accountForm, setAccountForm] = useState({
    code: "",
    name: "",
    type: "ASSET",
    parent_id: "",
  });

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const list = await invoke("list_chart_of_accounts_cmd", { token });
      setAccounts(list);
    } catch (e) {
      console.error("Failed to load chart of accounts data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  // Account submit
  const handleAccountSubmit = async (e) => {
    e.preventDefault();
    try {
      await invoke("create_chart_of_account_cmd", {
        token,
        code: accountForm.code,
        name: accountForm.name,
        accountType: accountForm.type,
        parentId: accountForm.parent_id || null,
      });
      setIsAccountModalOpen(false);
      setAccountForm({ code: "", name: "", type: "ASSET", parent_id: "" });
      fetchData();
    } catch (e) {
      alert("Error creating account: " + e);
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
            variant="default"
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
            variant="outline"
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

      {loading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-lg font-semibold">General Ledger Accounts</h3>
            {!isReadOnly && (
              <Button
                onClick={() => setIsAccountModalOpen(true)}
                className="flex items-center gap-1.5"
              >
                <HugeiconsIcon icon={Add01Icon} size={16} /> Create Account
              </Button>
            )}
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-left text-sm">
                  <thead>
                    <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                      <th className="p-4 font-medium">Code</th>
                      <th className="p-4 font-medium">Account Name</th>
                      <th className="p-4 font-medium">Account Type</th>
                      <th className="p-4 text-right font-medium">Balance</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {accounts.map((acc) => (
                      <tr key={acc.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs font-semibold">{acc.code}</td>
                        <td className="p-4 font-medium">{acc.name}</td>
                        <td className="p-4 text-xs font-semibold capitalize">{acc.account_type}</td>
                        <td
                          className={`p-4 text-right font-mono font-bold ${
                            acc.account_type === "REVENUE" || acc.account_type === "ASSET"
                              ? "text-emerald-600"
                              : "text-foreground"
                          }`}
                        >
                          {currencySymbol}
                          {acc.balance?.toFixed(2)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Create Account Modal */}
      <Dialog open={isAccountModalOpen} onOpenChange={setIsAccountModalOpen}>
        <DialogContent className="font-sans sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Create Account Code</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleAccountSubmit} className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="acc_code">Account Code *</Label>
                <Input
                  id="acc_code"
                  placeholder="e.g. 1010, 5010"
                  value={accountForm.code}
                  onChange={(e) => setAccountForm({ ...accountForm, code: e.target.value })}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="acc_name">Account Name *</Label>
                <Input
                  id="acc_name"
                  placeholder="e.g. Cash Account, Rent Expense"
                  value={accountForm.name}
                  onChange={(e) => setAccountForm({ ...accountForm, name: e.target.value })}
                  required
                />
              </div>
            </div>
            <div className="space-y-2">
              <Label htmlFor="acc_type">Account Type *</Label>
              <select
                id="acc_type"
                className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                value={accountForm.type}
                onChange={(e) => setAccountForm({ ...accountForm, type: e.target.value })}
                required
              >
                <option value="ASSET">Asset (Cash, Inventory)</option>
                <option value="LIABILITY">Liability (Creditors, Loans)</option>
                <option value="EQUITY">Equity (Share Capital)</option>
                <option value="REVENUE">Revenue (Product Sales)</option>
                <option value="EXPENSE">Expense (Rent, Wages)</option>
              </select>
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsAccountModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Create Account</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
