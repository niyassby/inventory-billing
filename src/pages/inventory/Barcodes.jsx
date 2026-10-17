import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import Barcode from "react-barcode";
import QRCode from "react-qr-code";
import { HugeiconsIcon } from "@hugeicons/react";
import { PrinterIcon } from "@hugeicons/core-free-icons";

export default function Barcodes() {
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);

  // Data State
  const [products, setProducts] = useState([]);
  const [loading, setLoading] = useState(true);

  // Barcode Printing State
  const [barcodeProduct, setBarcodeProduct] = useState(null);
  const [barcodeQty, setBarcodeQty] = useState(12);
  const [barcodeType, setBarcodeType] = useState("CODE128");

  // Fetch Methods
  const fetchData = async () => {
    setLoading(true);
    try {
      const prodList = await invoke("list_products_cmd", { token });
      setProducts(prodList);
    } catch (e) {
      console.error("Failed to load products for barcodes:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  return (
    <div className="p-4">
      {loading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="grid gap-6 md:grid-cols-3">
          {/* Product selector panel */}
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm print:hidden">
            <CardHeader>
              <CardTitle>Label Customizer</CardTitle>
              <CardDescription>
                Select a product to preview and generate sheet labels.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label>Select Product</Label>
                <select
                  className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                  onChange={(e) => {
                    const prod = products.find((p) => p.id === e.target.value);
                    setBarcodeProduct(prod || null);
                  }}
                  defaultValue=""
                >
                  <option value="" disabled>
                    Choose Product
                  </option>
                  {products.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name} ({p.sku})
                    </option>
                  ))}
                </select>
              </div>

              {barcodeProduct && (
                <>
                  <div className="space-y-2">
                    <Label>Label Format</Label>
                    <select
                      className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                      value={barcodeType}
                      onChange={(e) => setBarcodeType(e.target.value)}
                    >
                      <option value="CODE128">Barcode (CODE128)</option>
                      <option value="QR">QR Code (SKU Link)</option>
                    </select>
                  </div>

                  <div className="space-y-2">
                    <Label>Quantity to Print (Sheet layout)</Label>
                    <Input
                      type="number"
                      value={barcodeQty}
                      onChange={(e) => setBarcodeQty(Math.max(1, parseInt(e.target.value) || 1))}
                    />
                  </div>

                  <Button
                    className="flex w-full items-center justify-center gap-1.5"
                    onClick={() => window.print()}
                  >
                    <HugeiconsIcon icon={PrinterIcon} size={16} /> Print Sheet
                  </Button>
                </>
              )}
            </CardContent>
          </Card>

          {/* Preview Sheet Panel */}
          <div className="space-y-4 md:col-span-2">
            <h3 className="text-lg font-semibold print:hidden">Print Sheet Preview</h3>
            {barcodeProduct ? (
              <div className="rounded-lg border border-border bg-white p-6 text-black print:border-none print:p-0">
                {/* Printable Label Grid */}
                <div className="grid grid-cols-2 gap-6 p-4 md:grid-cols-3">
                  {Array.from({ length: barcodeQty }).map((_, i) => (
                    <div
                      key={i}
                      className="flex flex-col items-center justify-center rounded border border-gray-300 bg-white p-4 text-center"
                    >
                      <div className="max-w-full truncate text-[10px] font-bold text-black uppercase">
                        {barcodeProduct.name}
                      </div>
                      <div className="my-1 font-mono text-xs font-semibold text-black">
                        {currencySymbol}
                        {barcodeProduct.price.toFixed(2)}
                      </div>

                      <div className="my-2 flex items-center justify-center text-black">
                        {barcodeType === "CODE128" ? (
                          <Barcode
                            value={barcodeProduct.barcode || barcodeProduct.sku}
                            width={1.2}
                            height={40}
                            fontSize={10}
                            margin={2}
                            background="transparent"
                          />
                        ) : (
                          <QRCode
                            value={barcodeProduct.sku}
                            size={60}
                            bgColor="#ffffff"
                            fgColor="#000000"
                          />
                        )}
                      </div>

                      <div className="font-mono text-[9px] font-semibold text-gray-500">
                        SKU: {barcodeProduct.sku}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="flex h-64 items-center justify-center rounded-lg border-2 border-dashed bg-card/65 text-sm text-muted-foreground backdrop-blur-sm print:hidden">
                Select a product from the left to generate print sheets.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
