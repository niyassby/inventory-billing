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
  BookOpen02Icon,
  ShuffleIcon,
  DollarSignIcon,
  BadgePercentIcon,
} from "@hugeicons/core-free-icons";

export default function Taxes() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "staff";

  // Data States
  const [taxes, setTaxes] = useState([]);
  const [loading, setLoading] = useState(true);

  // Modals States
  const [isTaxModalOpen, setIsTaxModalOpen] = useState(false);
  const [taxForm, setTaxForm] = useState({ name: "", rate: 0 });

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const list = await invoke("list_tax_rates_cmd", { token });
      setTaxes(list);
    } catch (e) {
      console.error("Failed to load taxes data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  // Tax Submit
  const handleTaxSubmit = async (e) => {
    e.preventDefault();
    try {
      await invoke("create_tax_rate_cmd", {
        token,
        name: taxForm.name,
        rate: parseFloat(taxForm.rate),
      });
      setIsTaxModalOpen(false);
      setTaxForm({ name: "", rate: 0 });
      fetchData();
    } catch (e) {
      alert("Error creating tax rate: " + e);
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
            variant="outline"
            onClick={() => navigate("/accounting/expenses")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={DollarSignIcon} size={16} /> Expense Logger
          </Button>
          <Button
            variant="default"
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
            <h3 className="text-lg font-semibold">Configured VAT / Sales Taxes</h3>
            {!isReadOnly && (
              <Button onClick={() => setIsTaxModalOpen(true)} className="flex items-center gap-1.5">
                <HugeiconsIcon icon={Add01Icon} size={16} /> Add Tax Rule
              </Button>
            )}
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-left text-sm">
                  <thead>
                    <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                      <th className="p-4 font-medium">Tax Rule Name</th>
                      <th className="p-4 text-right font-medium">Percentage Rate (%)</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {taxes.map((t) => (
                      <tr key={t.id} className="hover:bg-muted/10">
                        <td className="p-4 font-semibold">{t.name}</td>
                        <td className="p-4 text-right font-mono font-bold text-primary">
                          {t.rate.toFixed(2)}%
                        </td>
                      </tr>
                    ))}
                    {taxes.length === 0 && (
                      <tr>
                        <td colSpan={2} className="py-8 text-center text-xs text-muted-foreground">
                          No sales tax rates configured.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Tax Modal */}
      <Dialog open={isTaxModalOpen} onOpenChange={setIsTaxModalOpen}>
        <DialogContent className="font-sans sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Add Sales Tax Rule</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleTaxSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="tax_name">Tax Rule Name *</Label>
              <Input
                id="tax_name"
                placeholder="e.g. VAT 15%, CGST + SGST"
                value={taxForm.name}
                onChange={(e) => setTaxForm({ ...taxForm, name: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="tax_rate">Rate Percentage (%) *</Label>
              <Input
                id="tax_rate"
                type="number"
                step="0.01"
                placeholder="e.g. 18.0"
                value={taxForm.rate}
                onChange={(e) => setTaxForm({ ...taxForm, rate: e.target.value })}
                required
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsTaxModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Create Tax Rule</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
