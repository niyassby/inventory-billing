import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01Icon, Search01Icon, ShoppingBagIcon, InvoiceIcon } from "@hugeicons/core-free-icons";

export default function Bills() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data States
  const [purchases, setPurchases] = useState([]);
  const [loading, setLoading] = useState(true);

  // Filters
  const [searchQuery, setSearchQuery] = useState("");

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const list = await invoke("list_purchases_cmd", { token });
      // Filter only RECEIVED purchases since they represent actual bills/obligations
      const receivedList = list.filter((p) => p.status === "RECEIVED");
      setPurchases(receivedList);
    } catch (e) {
      console.error("Purchases data load failed:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  // Filters
  const filteredPurchases = purchases.filter((p) => {
    return (
      p.supplier_name?.toLowerCase().includes(searchQuery.toLowerCase()) ||
      p.id.toLowerCase().includes(searchQuery.toLowerCase())
    );
  });

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Purchases & Restocks</h1>
          <p className="text-muted-foreground">
            Order restock inventory from suppliers, log bills, and track purchase expenses.
          </p>
        </div>
        <div className="flex gap-2 print:hidden">
          <Button
            variant="outline"
            onClick={() => navigate("/purchases/orders")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ShoppingBagIcon} size={16} strokeWidth={2} /> Purchase Orders
          </Button>
          <Button
            variant="default"
            onClick={() => navigate("/purchases/bills")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={InvoiceIcon} size={16} strokeWidth={2} /> Supplier Bills
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/purchases/returns")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Add01Icon} size={16} strokeWidth={2} /> Purchase Returns
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
              placeholder="Search by supplier name or Bill ID..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="border-border/40 bg-card pl-9"
            />
          </div>
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
                      <th className="p-4 font-medium">Date Received</th>
                      <th className="p-4 font-medium">Bill ID / reference</th>
                      <th className="p-4 font-medium">Supplier Name</th>
                      <th className="p-4 text-right font-medium">Bill Amount</th>
                      <th className="p-4 text-center font-medium">Status</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {filteredPurchases.map((purchase) => (
                      <tr key={purchase.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs text-muted-foreground">
                          {new Date(purchase.created_at).toLocaleString()}
                        </td>
                        <td className="p-4 font-mono text-xs font-semibold text-primary">
                          #BILL-{purchase.id.substring(0, 8).toUpperCase()}
                        </td>
                        <td className="p-4 font-medium">{purchase.supplier_name}</td>
                        <td className="p-4 text-right font-mono font-bold">
                          {currencySymbol}
                          {purchase.total.toFixed(2)}
                        </td>
                        <td className="p-4 text-center">
                          <span className="rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-600">
                            RECEIVED & LOGGED
                          </span>
                        </td>
                      </tr>
                    ))}
                    {filteredPurchases.length === 0 && (
                      <tr>
                        <td colSpan={5} className="py-8 text-center text-xs text-muted-foreground">
                          No supplier bills logged. Receive stock from Purchase Orders to generate
                          bills automatically.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
