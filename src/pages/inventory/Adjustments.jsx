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
import { BarcodeIcon, Tag01Icon, ArchiveIcon, ShuffleIcon } from "@hugeicons/core-free-icons";

export default function Adjustments() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const checkLowStockAlerts = useStore((state) => state.checkLowStockAlerts);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "accountant";

  // Data State
  const [products, setProducts] = useState([]);
  const [adjustments, setAdjustments] = useState([]);
  const [loading, setLoading] = useState(true);

  // Modals State
  const [isAdjustmentModalOpen, setIsAdjustmentModalOpen] = useState(false);
  const [adjustmentForm, setAdjustmentForm] = useState({
    product_id: "",
    type: "ADD",
    quantity: 1,
    reason: "",
  });

  // Fetch Methods
  const fetchData = async () => {
    setLoading(true);
    try {
      const adjList = await invoke("list_stock_adjustments_cmd", { token });
      setAdjustments(adjList);
      const prodList = await invoke("list_products_cmd", { token });
      setProducts(prodList);
    } catch (e) {
      console.error("Failed to load stock adjustments:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  // Stock Adjustment Submit
  const handleAdjustmentSubmit = async (e) => {
    e.preventDefault();
    try {
      await invoke("adjust_stock_cmd", {
        token,
        productId: adjustmentForm.product_id,
        adjustmentType: adjustmentForm.type,
        quantity: parseFloat(adjustmentForm.quantity),
        reason: adjustmentForm.reason || null,
      });
      setIsAdjustmentModalOpen(false);
      fetchData();
      checkLowStockAlerts();
    } catch (e) {
      alert("Error submitting stock adjustment: " + e);
    }
  };

  return (
    <div className="p-4">
      {loading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-lg font-semibold">Stock Adjustment History</h3>
            {!isReadOnly && (
              <Button
                onClick={() => setIsAdjustmentModalOpen(true)}
                className="flex items-center gap-1"
              >
                <HugeiconsIcon icon={ShuffleIcon} size={16} /> Manual Stock Adjustment
              </Button>
            )}
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-left text-sm">
                  <thead>
                    <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                      <th className="p-4 font-medium">Timestamp</th>
                      <th className="p-4 font-medium">Product Name</th>
                      <th className="p-4 font-medium">Adjustment Type</th>
                      <th className="p-4 text-center font-medium">Qty</th>
                      <th className="p-4 font-medium">Reason</th>
                      <th className="p-4 font-medium">Adjusted By</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {adjustments.map((a) => (
                      <tr key={a.id} className="hover:bg-muted/10">
                        <td className="p-4 font-mono text-xs text-muted-foreground">
                          {new Date(a.created_at).toLocaleString()}
                        </td>
                        <td className="p-4 font-medium">{a.product_name}</td>
                        <td className="p-4 text-xs font-semibold">
                          <span
                            className={`rounded-full px-2 py-0.5 ${
                              a.adjustment_type === "ADD"
                                ? "border border-emerald-500/20 bg-emerald-500/10 text-emerald-600"
                                : a.adjustment_type === "SUBTRACT"
                                  ? "border border-rose-500/20 bg-rose-500/10 text-rose-600"
                                  : "border border-amber-500/20 bg-amber-500/10 text-amber-600"
                            }`}
                          >
                            {a.adjustment_type}
                          </span>
                        </td>
                        <td className="p-4 text-center font-mono font-bold">{a.quantity}</td>
                        <td className="p-4 text-xs text-muted-foreground">{a.reason || "-"}</td>
                        <td className="p-4 font-medium">{a.adjusted_by_name || "Staff"}</td>
                      </tr>
                    ))}
                    {adjustments.length === 0 && (
                      <tr>
                        <td colSpan={6} className="py-8 text-center text-xs text-muted-foreground">
                          No adjustments logged.
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

      {/* Stock Adjustment Modal */}
      <Dialog open={isAdjustmentModalOpen} onOpenChange={setIsAdjustmentModalOpen}>
        <DialogContent className="font-sans sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Manual Stock Adjustment</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleAdjustmentSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="adj_product">Product *</Label>
              <select
                id="adj_product"
                className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                value={adjustmentForm.product_id}
                onChange={(e) =>
                  setAdjustmentForm({ ...adjustmentForm, product_id: e.target.value })
                }
                required
              >
                <option value="" disabled>
                  Choose Product
                </option>
                {products.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} (In stock: {p.stock})
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="adj_type">Adjustment Type *</Label>
              <select
                id="adj_type"
                className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                value={adjustmentForm.type}
                onChange={(e) => setAdjustmentForm({ ...adjustmentForm, type: e.target.value })}
                required
              >
                <option value="ADD">Add Stock (+)</option>
                <option value="SUBTRACT">Subtract Stock (-)</option>
                <option value="SET">Set Explicit Stock (=)</option>
              </select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="adj_qty">Quantity *</Label>
              <Input
                id="adj_qty"
                type="number"
                step="1"
                value={adjustmentForm.quantity}
                onChange={(e) => setAdjustmentForm({ ...adjustmentForm, quantity: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="adj_reason">Reason / Notes</Label>
              <Input
                id="adj_reason"
                placeholder="Manual adjustment reason"
                value={adjustmentForm.reason}
                onChange={(e) => setAdjustmentForm({ ...adjustmentForm, reason: e.target.value })}
              />
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setIsAdjustmentModalOpen(false)}
              >
                Cancel
              </Button>
              <Button type="submit">Submit Adjustment</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
