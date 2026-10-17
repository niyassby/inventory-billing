import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/stores/app-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PrinterIcon,
  CalculatorIcon,
  ShoppingBagIcon,
  ArchiveIcon,
  BadgePercentIcon,
} from "@hugeicons/core-free-icons";

export default function ProfitAndLoss() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [stats, setStats] = useState({
    revenue: 0,
    expenses: 0,
    profit: 0,
  });
  const [invoices, setInvoices] = useState([]);
  const [loading, setLoading] = useState(true);

  // Fetch report data
  const fetchReportData = async () => {
    setLoading(true);
    try {
      const dbStats = await invoke("get_dashboard_stats_cmd", { token });
      setStats(dbStats);

      const invList = await invoke("list_invoices_cmd", { token });
      setInvoices(invList);
    } catch (e) {
      console.error("Failed to load profit and loss data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchReportData();
  }, [token]);

  // Compute metrics
  const cogs = invoices.reduce((acc, inv) => {
    try {
      const items = JSON.parse(inv.items || "[]");
      return acc + items.reduce((sum, item) => sum + item.cost * item.quantity, 0);
    } catch (e) {
      return acc;
    }
  }, 0);

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div className="print:hidden">
          <h1 className="text-3xl font-bold tracking-tight">Reports Ledger</h1>
          <p className="text-muted-foreground">
            Generate commercial financial statements, check taxes, and value inventory assets.
          </p>
        </div>
        <div className="flex flex-wrap gap-2 print:hidden">
          <Button
            variant="default"
            onClick={() => navigate("/reports/profit-and-loss")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={CalculatorIcon} size={16} /> Profit & Loss
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/reports/sales-summary")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ShoppingBagIcon} size={16} /> Sales Summary
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/reports/stock-valuation")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ArchiveIcon} size={16} /> Stock Valuation
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/reports/tax-summary")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BadgePercentIcon} size={16} /> Tax Summary
          </Button>
        </div>
      </div>

      {loading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex items-center justify-between print:hidden">
            <h3 className="text-lg font-semibold capitalize">Profit & Loss Statement</h3>
            <Button
              variant="outline"
              className="flex items-center gap-1 border-border/40"
              onClick={() => window.print()}
            >
              <HugeiconsIcon icon={PrinterIcon} size={14} /> Print Statement
            </Button>
          </div>

          {/* PRINT HEADER */}
          <div className="mb-6 hidden border-b pb-6 text-center print:block">
            <h1 className="text-2xl font-bold tracking-wider uppercase">
              Business Audit Report Sheet
            </h1>
            <p className="mt-1 text-xs text-muted-foreground">
              Generated: {new Date().toLocaleString()}
            </p>
            <h3 className="mt-4 rounded bg-muted p-2 text-sm font-semibold capitalize">
              Report Section: PROFIT & LOSS STATEMENT
            </h3>
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm print:border-none print:shadow-none">
            <CardHeader className="print:px-0">
              <CardTitle>Profit and Loss Statement (P&L)</CardTitle>
              <CardDescription>
                Estimated financial operating outcomes representing all logged transactions.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 print:px-0">
              <div className="divide-y divide-border/20 text-sm">
                <div className="flex justify-between py-3 font-semibold">
                  <span>Operating Revenues (Sales Invoices)</span>
                  <span className="font-mono text-emerald-600">
                    +{currencySymbol}
                    {stats.revenue.toFixed(2)}
                  </span>
                </div>

                <div className="flex justify-between py-3 pl-4 text-muted-foreground">
                  <span>Less: Cost of Goods Sold (COGS)</span>
                  <span className="font-mono text-rose-600">
                    -{currencySymbol}
                    {cogs.toFixed(2)}
                  </span>
                </div>

                <div className="flex justify-between rounded border-t-2 border-border/40 bg-muted/10 px-2 py-3 font-bold">
                  <span>GROSS PROFIT ESTIMATE</span>
                  <span className="font-mono">
                    {currencySymbol}
                    {(stats.revenue - cogs).toFixed(2)}
                  </span>
                </div>

                <div className="flex justify-between py-3 pl-4 text-muted-foreground">
                  <span>Less: Operating expenses logged</span>
                  <span className="font-mono text-rose-600">
                    -{currencySymbol}
                    {stats.expenses.toFixed(2)}
                  </span>
                </div>

                <div className="flex justify-between rounded border-t-2 border-double border-primary bg-primary/5 px-2 py-4 text-base font-extrabold">
                  <span>ESTIMATED NET OPERATING INCOME</span>
                  <span className="font-mono text-primary">
                    {currencySymbol}
                    {stats.profit.toFixed(2)}
                  </span>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
