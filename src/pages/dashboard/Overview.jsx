import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  BadgeDollarSignIcon,
  ShoppingBagIcon,
  ArchiveIcon,
  Alert01Icon,
  BookOpen02Icon,
  CreditCardIcon,
} from "@hugeicons/core-free-icons";
import { Button } from "@/components/ui/button";
import { useNavigate } from "react-router-dom";
import { DashboardChart1 } from "@/components/chart/dashboard-chart-1";

export default function Overview() {
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const navigate = useNavigate();

  const [stats, setStats] = useState({
    today_sales: 0,
    monthly_sales: 0,
    revenue: 0,
    profit: 0,
    expenses: 0,
    stock_value: 0,
    low_stock_count: 0,
  });

  const [chartData, setChartData] = useState([]);
  const [auditLogs, setAuditLogs] = useState([]);
  const [loading, setLoading] = useState(true);

  const fetchDashboardData = async () => {
    setLoading(true);
    try {
      const dbStats = await invoke("get_dashboard_stats_cmd", { token });
      setStats(dbStats);

      const points = await invoke("get_sales_chart_points_cmd", { token });
      setChartData(points);
      const logs = await invoke("get_audit_logs_cmd", { token });
      setAuditLogs(logs);
    } catch (e) {
      console.error("Dashboard data load failure:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchDashboardData();
  }, [token]);

  if (loading) {
    return (
      <div className="flex h-[60vh] items-center justify-center">
        <div className="flex flex-col items-center gap-2">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
          <span className="text-sm text-muted-foreground">Loading dashboard...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="animate-in space-y-6 p-4 font-sans duration-300 fade-in slide-in-from-bottom-2">
      <div className="flex justify-between gap-1">
        <div className="flex flex-col gap-1">
          <h1 className="text-3xl font-bold tracking-tight">Overview</h1>
          <p className="text-muted-foreground">
            Real-time summary of your store's inventory, sales performance, and accounting.
          </p>
        </div>
        <Button onClick={() => navigate("/sales/pos")}>Go to POS</Button>
      </div>

      {/* Stats Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {/* Today's Sales */}
        <Card className="bg-card/65 backdrop-blur-sm transition-shadow duration-200 hover:shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Today's Sales
            </CardTitle>
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <HugeiconsIcon icon={ShoppingBagIcon} size={18} strokeWidth={2} />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {currencySymbol}
              {stats.today_sales.toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
              })}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">Total billing transactions today</p>
          </CardContent>
        </Card>

        {/* Monthly Revenue */}
        <Card className="bg-card/65 backdrop-blur-sm transition-shadow duration-200 hover:shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Monthly Revenue
            </CardTitle>
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600">
              <HugeiconsIcon icon={BadgeDollarSignIcon} size={18} strokeWidth={2} />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {currencySymbol}
              {stats.monthly_sales.toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
              })}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">Sales in current calendar month</p>
          </CardContent>
        </Card>

        {/* Net Profit */}
        <Card className="bg-card/65 backdrop-blur-sm transition-shadow duration-200 hover:shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Estimated Profit
            </CardTitle>
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-600">
              <HugeiconsIcon icon={CreditCardIcon} size={18} strokeWidth={2} />
            </div>
          </CardHeader>
          <CardContent>
            <div
              className={`text-2xl font-bold ${stats.profit >= 0 ? "text-foreground" : "text-destructive"}`}
            >
              {stats.profit < 0 ? "-" : ""}
              {currencySymbol}
              {Math.abs(stats.profit).toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
              })}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">Revenue minus COGS and expenses</p>
          </CardContent>
        </Card>

        {/* Expenses */}
        <Card className="bg-card/65 backdrop-blur-sm transition-shadow duration-200 hover:shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Monthly Expenses
            </CardTitle>
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-rose-500/10 text-rose-600">
              <HugeiconsIcon icon={Alert01Icon} size={18} strokeWidth={2} />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-rose-600">
              {currencySymbol}
              {stats.expenses.toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
              })}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">Total expenses logged</p>
          </CardContent>
        </Card>
      </div>

      {/* Inventory & Low stock metrics */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-2">
        <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Total Stock Valuation
            </CardTitle>
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-500/10 text-amber-600">
              <HugeiconsIcon icon={ArchiveIcon} size={18} strokeWidth={2} />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-xl font-bold">
              {currencySymbol}
              {stats.stock_value.toLocaleString(undefined, {
                minimumFractionDigits: 2,
                maximumFractionDigits: 2,
              })}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              Calculated as stock quantity × cost price
            </p>
          </CardContent>
        </Card>

        <Card
          className={`border-border/40 bg-card/65 backdrop-blur-sm ${stats.low_stock_count > 0 ? "border-amber-500/30 bg-amber-500/5" : ""}`}
        >
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Low Stock Alert Items
            </CardTitle>
            <div
              className={`flex h-8 w-8 items-center justify-center rounded-lg ${stats.low_stock_count > 0 ? "bg-amber-500/20 text-amber-600" : "bg-muted text-muted-foreground"}`}
            >
              <HugeiconsIcon icon={Alert01Icon} size={18} strokeWidth={2} />
            </div>
          </CardHeader>
          <CardContent>
            <div
              className={`text-xl font-bold ${stats.low_stock_count > 0 ? "text-amber-600" : "text-foreground"}`}
            >
              {stats.low_stock_count} {stats.low_stock_count === 1 ? "Product" : "Products"}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">Items at or below reorder levels</p>
          </CardContent>
        </Card>
      </div>

      {/* Chart Section */}
      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-2">
        <DashboardChart1 chartData={chartData} />
        <DashboardChart1 chartData={chartData} />
      </div>

      {/* Audit Logs */}
      <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
        <CardHeader>
          <div className="flex items-center gap-2">
            <HugeiconsIcon
              icon={BookOpen02Icon}
              size={18}
              strokeWidth={2}
              className="text-primary"
            />
            <CardTitle>Security & System Audit Logs</CardTitle>
          </div>
          <CardDescription>
            Chronological logging of critical mutations (Offline compliance compliance).
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-left text-sm">
              <thead>
                <tr className="border-b border-border/40 text-xs text-muted-foreground uppercase">
                  <th className="py-2.5 font-medium">Timestamp</th>
                  <th className="py-2.5 font-medium">User</th>
                  <th className="py-2.5 font-medium">Action</th>
                  <th className="py-2.5 font-medium">Module</th>
                  <th className="py-2.5 font-medium">Details</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/20">
                {auditLogs.length > 0 ? (
                  auditLogs.slice(0, 7).map((log) => {
                    let actionColor = "text-foreground";
                    if (log.action === "CREATE") actionColor = "text-emerald-600 font-semibold";
                    if (log.action === "UPDATE") actionColor = "text-blue-600 font-semibold";
                    if (log.action === "DELETE") actionColor = "text-rose-600 font-semibold";
                    if (log.action.includes("ADJUST")) actionColor = "text-amber-600 font-semibold";

                    return (
                      <tr key={log.id} className="hover:bg-muted/10">
                        <td className="py-3 font-mono text-xs text-muted-foreground">
                          {new Date(log.timestamp).toLocaleString()}
                        </td>
                        <td className="py-3 font-medium">{log.user_name || "System"}</td>
                        <td className={`py-3 text-xs ${actionColor}`}>{log.action}</td>
                        <td className="py-3 text-xs font-semibold capitalize">{log.table_name}</td>
                        <td
                          className="max-w-xs truncate py-3 text-xs text-muted-foreground"
                          title={log.new_values || ""}
                        >
                          {log.new_values
                            ? log.new_values.substring(0, 60)
                            : `Record ID: ${log.record_id || "None"}`}
                        </td>
                      </tr>
                    );
                  })
                ) : (
                  <tr>
                    <td colSpan={5} className="py-8 text-center text-xs text-muted-foreground">
                      No audit logs generated yet.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
