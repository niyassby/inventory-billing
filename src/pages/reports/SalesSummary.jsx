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

export default function SalesSummary() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [stats, setStats] = useState({
    revenue: 0,
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
      console.error("Failed to load sales summary data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchReportData();
  }, [token]);

  // Compute metrics
  const cashSales = invoices
    .filter((i) => i.payment_method === "CASH")
    .reduce((acc, i) => acc + i.amount_paid, 0);
  const cardSales = invoices
    .filter((i) => i.payment_method === "CARD")
    .reduce((acc, i) => acc + i.amount_paid, 0);
  const upiSales = invoices
    .filter((i) => i.payment_method === "UPI")
    .reduce((acc, i) => acc + i.amount_paid, 0);
  const debtSales = invoices.reduce((acc, i) => acc + (i.total - i.amount_paid), 0);

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
            variant="outline"
            onClick={() => navigate("/reports/profit-and-loss")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={CalculatorIcon} size={16} /> Profit & Loss
          </Button>
          <Button
            variant="default"
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
            <h3 className="text-lg font-semibold capitalize">Sales Summary Statement</h3>
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
              Report Section: SALES SUMMARY STATEMENT
            </h3>
          </div>

          <div className="grid gap-6 md:grid-cols-2">
            <Card className="border-border/40 bg-card/65 backdrop-blur-sm print:border-none print:shadow-none">
              <CardHeader>
                <CardTitle>Revenues by Collection Method</CardTitle>
                <CardDescription>Breakdown of payments logged at the POS checkout.</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex justify-between border-b py-2 text-sm">
                  <span className="font-medium">Cash Register Sales</span>
                  <span className="font-mono font-bold text-emerald-600">
                    {currencySymbol}
                    {cashSales.toFixed(2)}
                  </span>
                </div>
                <div className="flex justify-between border-b py-2 text-sm">
                  <span className="font-medium">Card Swipe Terminals</span>
                  <span className="font-mono font-bold text-primary">
                    {currencySymbol}
                    {cardSales.toFixed(2)}
                  </span>
                </div>
                <div className="flex justify-between border-b py-2 text-sm">
                  <span className="font-medium">UPI / QR Code Scans</span>
                  <span className="font-mono font-bold text-violet-600">
                    {currencySymbol}
                    {upiSales.toFixed(2)}
                  </span>
                </div>
                <div className="flex justify-between border-b py-2 text-sm">
                  <span className="font-medium">Customer Credit Account Debt</span>
                  <span className="font-mono font-bold text-rose-600">
                    {currencySymbol}
                    {debtSales.toFixed(2)}
                  </span>
                </div>
                <div className="flex justify-between pt-2 text-sm font-extrabold">
                  <span>Cumulative Total Sales Value</span>
                  <span className="font-mono">
                    {currencySymbol}
                    {stats.revenue.toFixed(2)}
                  </span>
                </div>
              </CardContent>
            </Card>

            <Card className="border-border/40 bg-card/65 backdrop-blur-sm print:hidden">
              <CardHeader>
                <CardTitle>General Invoicing Stats</CardTitle>
                <CardDescription>
                  Overall counts and sizes of client billing tickets.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="rounded-lg border bg-muted/15 p-4 text-center">
                  <div className="font-mono text-3xl font-extrabold text-primary">
                    {invoices.length}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Total completed billing orders
                  </p>
                </div>
                <div className="rounded-lg border bg-muted/15 p-4 text-center">
                  <div className="font-mono text-xl font-bold">
                    {currencySymbol}
                    {invoices.length > 0 ? (stats.revenue / invoices.length).toFixed(2) : "0.00"}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Average sale ticket basket value
                  </p>
                </div>
              </CardContent>
            </Card>
          </div>
        </div>
      )}
    </div>
  );
}
