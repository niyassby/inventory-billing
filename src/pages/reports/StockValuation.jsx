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

export default function StockValuation() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [stats, setStats] = useState({
    stock_value: 0,
  });
  const [products, setProducts] = useState([]);
  const [loading, setLoading] = useState(true);

  // Fetch report data
  const fetchReportData = async () => {
    setLoading(true);
    try {
      const dbStats = await invoke("get_dashboard_stats_cmd", { token });
      setStats(dbStats);

      const prodList = await invoke("list_products_cmd", { token });
      setProducts(prodList);
    } catch (e) {
      console.error("Failed to load stock valuation data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchReportData();
  }, [token]);

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
            variant="outline"
            onClick={() => navigate("/reports/sales-summary")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ShoppingBagIcon} size={16} /> Sales Summary
          </Button>
          <Button
            variant="default"
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
            <h3 className="text-lg font-semibold capitalize">Stock Valuation Statement</h3>
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
              Report Section: STOCK VALUATION STATEMENT
            </h3>
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm print:border-none print:shadow-none">
            <CardHeader className="print:px-0">
              <CardTitle>Inventory Asset Valuations</CardTitle>
              <CardDescription>
                Product-by-product assessment of stock valuation based on cost basis.
              </CardDescription>
            </CardHeader>
            <CardContent className="p-0 print:px-0">
              <table className="w-full border-collapse text-left text-sm">
                <thead>
                  <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                    <th className="p-4 font-medium">SKU</th>
                    <th className="p-4 font-medium">Product Name</th>
                    <th className="p-4 text-center font-medium">Stock On Hand</th>
                    <th className="p-4 text-right font-medium">Unit Cost</th>
                    <th className="p-4 text-right font-medium">Asset Value</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border/20">
                  {products.map((p) => {
                    const assetValue = p.stock * p.cost;
                    return (
                      <tr key={p.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs font-semibold">{p.sku}</td>
                        <td className="p-4 font-medium">{p.name}</td>
                        <td className="p-4 text-center font-mono font-bold">{p.stock}</td>
                        <td className="p-4 text-right font-mono">
                          {currencySymbol}
                          {p.cost.toFixed(2)}
                        </td>
                        <td className="p-4 text-right font-mono font-bold text-emerald-600">
                          {currencySymbol}
                          {assetValue.toFixed(2)}
                        </td>
                      </tr>
                    );
                  })}
                  <tr className="border-t-2 bg-muted/10 font-bold">
                    <td colSpan={2} className="p-4">
                      Aggregate Valuation Summary
                    </td>
                    <td className="p-4 text-center font-mono">
                      {products.reduce((acc, p) => acc + p.stock, 0)} Units
                    </td>
                    <td></td>
                    <td className="p-4 text-right font-mono text-base text-emerald-600">
                      {currencySymbol}
                      {stats.stock_value.toLocaleString(undefined, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      })}
                    </td>
                  </tr>
                </tbody>
              </table>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
