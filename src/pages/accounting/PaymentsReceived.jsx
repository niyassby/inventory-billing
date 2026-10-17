import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  BookOpen02Icon,
  ShuffleIcon,
  DollarSignIcon,
  BadgePercentIcon,
  InvoiceIcon
} from "@hugeicons/core-free-icons";

export default function PaymentsReceived() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [payments, setPayments] = useState([]);
  const [loading, setLoading] = useState(true);

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const list = await invoke("list_invoices_cmd", { token });
      // Filter invoices that have had some amount paid
      const loggedPayments = list
        .filter((inv) => inv.amount_paid > 0)
        .map((inv) => ({
          id: inv.id,
          date: inv.created_at,
          customer_name: inv.customer_name || "Walk-in Customer",
          amount: inv.amount_paid,
          method: inv.payment_method || "CASH",
          invoice_ref: inv.id.substring(0, 8).toUpperCase()
        }));
      setPayments(loggedPayments);
    } catch (e) {
      console.error("Failed to load payments data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Ledger & Accounting</h1>
          <p className="text-muted-foreground">Manage chart of accounts, draft balancing journal entries, log expenses, and configure tax rules.</p>
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
          <h3 className="text-lg font-semibold">Customer Payments Received</h3>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm text-left border-collapse">
                  <thead>
                    <tr className="border-b border-border/40 text-muted-foreground text-xs uppercase bg-muted/20">
                      <th className="p-4 font-medium">Payment Date</th>
                      <th className="p-4 font-medium">Invoice Reference</th>
                      <th className="p-4 font-medium">Customer</th>
                      <th className="p-4 font-medium">Payment Method</th>
                      <th className="p-4 font-medium text-right">Amount Received</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {payments.map((p) => (
                      <tr key={p.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs text-muted-foreground">
                          {new Date(p.date).toLocaleString()}
                        </td>
                        <td className="p-4 font-semibold font-mono text-xs text-primary">
                          #INV-{p.invoice_ref}
                        </td>
                        <td className="p-4 font-medium">{p.customer_name}</td>
                        <td className="p-4 text-xs font-semibold uppercase">{p.method}</td>
                        <td className="p-4 text-right font-mono font-bold text-emerald-600">
                          {currencySymbol}{p.amount.toFixed(2)}
                        </td>
                      </tr>
                    ))}
                    {payments.length === 0 && (
                      <tr>
                        <td colSpan={5} className="py-8 text-center text-muted-foreground text-xs">
                          No customer payments logged yet. Complete sales at POS or Invoice workspace.
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
    </div>
  );
}
