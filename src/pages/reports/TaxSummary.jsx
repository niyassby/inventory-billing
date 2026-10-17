import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PrinterIcon,
  CalculatorIcon,
  ShoppingBagIcon,
  ArchiveIcon,
  BadgePercentIcon
} from "@hugeicons/core-free-icons";

export default function TaxSummary() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [taxSummary, setTaxSummary] = useState([]);
  const [loading, setLoading] = useState(true);

  // Fetch report data
  const fetchReportData = async () => {
    setLoading(true);
    try {
      const taxSummaryList = await invoke("get_tax_summary_cmd", { token });
      setTaxSummary(taxSummaryList);
    } catch (e) {
      console.error("Failed to load tax summary data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchReportData();
  }, [token]);

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div className="print:hidden">
          <h1 className="text-3xl font-bold tracking-tight">Reports Ledger</h1>
          <p className="text-muted-foreground">Generate commercial financial statements, check taxes, and value inventory assets.</p>
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
            variant="outline"
            onClick={() => navigate("/reports/stock-valuation")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ArchiveIcon} size={16} /> Stock Valuation
          </Button>
          <Button
            variant="default"
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
          <div className="flex justify-between items-center print:hidden">
            <h3 className="text-lg font-semibold capitalize">Tax Summary Statement</h3>
            <Button variant="outline" className="flex items-center gap-1 border-border/40" onClick={() => window.print()}>
              <HugeiconsIcon icon={PrinterIcon} size={14} /> Print Statement
            </Button>
          </div>

          {/* PRINT HEADER */}
          <div className="hidden print:block text-center border-b pb-6 mb-6">
            <h1 className="text-2xl font-bold uppercase tracking-wider">Business Audit Report Sheet</h1>
            <p className="text-xs text-muted-foreground mt-1">Generated: {new Date().toLocaleString()}</p>
            <h3 className="text-sm font-semibold capitalize mt-4 bg-muted p-2 rounded">
              Report Section: TAX SUMMARY STATEMENT
            </h3>
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm print:border-none print:shadow-none">
            <CardHeader className="print:px-0">
              <CardTitle>Sales Taxes Collected Statement</CardTitle>
              <CardDescription>Assessed output tax amounts parsed from active billing tickets.</CardDescription>
            </CardHeader>
            <CardContent className="p-0 print:px-0">
              <table className="w-full text-sm text-left border-collapse">
                <thead>
                  <tr className="border-b border-border/40 text-muted-foreground text-xs uppercase bg-muted/20">
                    <th className="p-4 font-medium">Output Period</th>
                    <th className="p-4 font-medium text-right">Tax Collected</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border/20">
                  {taxSummary.map((pt, idx) => (
                    <tr key={idx} className="hover:bg-muted/10">
                      <td className="p-4 font-semibold">{pt.label} (Aggregate Output)</td>
                      <td className="p-4 text-right font-mono font-bold text-primary">
                        {currencySymbol}{pt.value.toFixed(2)}
                      </td>
                    </tr>
                  ))}
                  {taxSummary.length === 0 && (
                    <tr>
                      <td colSpan={2} className="py-8 text-center text-muted-foreground text-xs">
                        No sales tax reports captured yet.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
