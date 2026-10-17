import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import * as XLSX from "xlsx";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Search01Icon,
  Delete02Icon,
  Edit02Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { ArrowDown01Icon, ArrowLeft01Icon, ArrowRight01Icon, ArrowUp01Icon } from "@hugeicons/core-free-icons/index";
import { useDeleteProduct, useProducts } from "@/hooks/tanstack/useProducts";
import { useBrands, useCategories } from "@/hooks/tanstack/useCategorie&Brand";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Checkbox } from "@/components/ui/checkbox";
import { ArrowUp, ChevronDown, ChevronUp, History } from "lucide-react";

export default function Products() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const checkLowStockAlerts = useStore((state) => state.checkLowStockAlerts);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "accountant";
  const cannotDelete = userRole === "staff" || userRole === "accountant";

  // Excel Upload States
  const [isImportModalOpen, setIsImportModalOpen] = useState(false);
  const [parsedProducts, setParsedProducts] = useState([]);
  const [isImporting, setIsImporting] = useState(false);
  const [importError, setImportError] = useState(null);

  const handleDownloadTemplate = () => {
    const headers = [
      "Name",
      "SKU",
      "Barcode",
      "Category",
      "Brand",
      "Cost",
      "Price",
      "Stock",
      "Reorder Level",
      "Unit"
    ];
    const sampleData = [
      [
        "Sample Product A",
        "SKU-001",
        "123456789012",
        "Electronics",
        "BrandX",
        "45.50",
        "79.99",
        "100",
        "10",
        "pcs"
      ],
      [
        "Sample Product B",
        "SKU-002",
        "987654321098",
        "Apparel",
        "BrandY",
        "12.00",
        "29.99",
        "250",
        "20",
        "pcs"
      ]
    ];
    const ws = XLSX.utils.aoa_to_sheet([headers, ...sampleData]);
    const wb = XLSX.utils.book_new();
    XLSX.utils.book_append_sheet(wb, ws, "Products Template");
    XLSX.writeFile(wb, "products_import_template.xlsx");
    toast.success("Sample import template downloaded!");
  };

  const handleExcelUpload = (e) => {
    const file = e.target.files[0];
    if (!file) return;
    setImportError(null);

    const reader = new FileReader();
    reader.onload = async (evt) => {
      try {
        const data = new Uint8Array(evt.target.result);
        const workbook = XLSX.read(data, { type: "array" });
        const sheetName = workbook.SheetNames[0];
        const worksheet = workbook.Sheets[sheetName];
        const rows = XLSX.utils.sheet_to_json(worksheet, { header: 1 });
        if (rows.length < 2) {
          setImportError("The Excel sheet is empty or lacks headers.");
          return;
        }

        const headers = rows[0].map(h => String(h).trim().toLowerCase());
        
        const nameIdx = headers.findIndex(h => ["name", "product name", "product_name", "title"].includes(h));
        const skuIdx = headers.findIndex(h => ["sku", "sku code", "product code", "item code"].includes(h));
        const barcodeIdx = headers.findIndex(h => ["barcode", "bar code", "upc", "ean"].includes(h));
        const categoryIdx = headers.findIndex(h => ["category", "category name", "category_name", "type"].includes(h));
        const brandIdx = headers.findIndex(h => ["brand", "brand name", "brand_name", "manufacturer"].includes(h));
        const costIdx = headers.findIndex(h => ["cost", "cost price", "cost_price", "purchase price", "purchase_price"].includes(h));
        const priceIdx = headers.findIndex(h => ["price", "selling price", "selling_price", "retail price", "retail_price"].includes(h));
        const stockIdx = headers.findIndex(h => ["stock", "quantity", "qty", "initial stock", "initial_stock"].includes(h));
        const reorderIdx = headers.findIndex(h => ["reorder level", "reorder_level", "alert quantity", "alert_qty"].includes(h));
        const unitIdx = headers.findIndex(h => ["unit", "uom"].includes(h));

        if (nameIdx === -1) {
          setImportError("Could not find a 'Name' column in Excel headers.");
          return;
        }
        if (skuIdx === -1) {
          setImportError("Could not find a 'SKU' column in Excel headers.");
          return;
        }

        const items = [];
        for (let i = 1; i < rows.length; i++) {
          const row = rows[i];
          if (!row || row.length === 0) continue;
          
          const nameVal = row[nameIdx] ? String(row[nameIdx]).trim() : "";
          const skuVal = row[skuIdx] ? String(row[skuIdx]).trim() : "";

          if (!nameVal && !skuVal) continue;

          if (!nameVal) {
            setImportError(`Row ${i + 1} is missing a Name.`);
            return;
          }
          if (!skuVal) {
            setImportError(`Row ${i + 1} is missing a SKU.`);
            return;
          }

          items.push({
            name: nameVal,
            sku: skuVal,
            barcode: barcodeIdx !== -1 && row[barcodeIdx] ? String(row[barcodeIdx]).trim() : null,
            category_name: categoryIdx !== -1 && row[categoryIdx] ? String(row[categoryIdx]).trim() : null,
            brand_name: brandIdx !== -1 && row[brandIdx] ? String(row[brandIdx]).trim() : null,
            cost: costIdx !== -1 && row[costIdx] ? parseFloat(row[costIdx]) || 0 : 0,
            price: priceIdx !== -1 && row[priceIdx] ? parseFloat(row[priceIdx]) || 0 : 0,
            stock: stockIdx !== -1 && row[stockIdx] ? parseFloat(row[stockIdx]) || 0 : 0,
            reorder_level: reorderIdx !== -1 && row[reorderIdx] ? parseFloat(row[reorderIdx]) || 0 : 0,
            unit: unitIdx !== -1 && row[unitIdx] ? String(row[unitIdx]).trim() : "pcs"
          });
        }

        if (items.length === 0) {
          setImportError("No valid product rows were found in the sheet.");
        } else {
          setParsedProducts(items);
        }
      } catch (err) {
        console.error("Excel Parse error:", err);
        setImportError("Error reading Excel file: " + String(err));
      }
    };
    reader.readAsArrayBuffer(file);
  };

  const handleBulkImportSubmit = async () => {
    if (parsedProducts.length === 0) return;
    setIsImporting(true);
    try {
      await invoke("bulk_import_products_cmd", { token, products: parsedProducts });
      toast.success(`Successfully imported/updated ${parsedProducts.length} products!`);
      setIsImportModalOpen(false);
      setParsedProducts([]);
      setTimeout(() => {
        window.location.reload();
      }, 500);
    } catch (err) {
      toast.error("Import failed: " + err);
    } finally {
      setIsImporting(false);
    }
  };

  // Data State
  const [products, setProducts] = useState([]);
  const {data:categories}=useCategories({ token })
  const {data:brands}=useBrands({ token })


  // Search & Filters
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState("");
  const [selectedBrand, setSelectedBrand] = useState("");
  
  // Pagination & Sorting State
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(10);
  const [sortBy, setSortBy] = useState("name");
  const [sortOrder, setSortOrder] = useState("ASC");
  const {mutate,isPending:isDeleting}=useDeleteProduct()
  const {data, isLoading, isError, error}=useProducts({
        token,
        search: searchQuery || null,
        categoryId: selectedCategory || null,
        brandId: selectedBrand || null,
        sortBy,
        sortOrder,
        page,
        limit
      })
  

  

  useEffect(() => {
    if (data) {
      setProducts(data?.products);
      setTotalCount(data?.total_count);
    }
  }, [data]);

  // Transaction Modal State
  const [selectedTxProduct, setSelectedTxProduct] = useState(null);
  const [txHistory, setTxHistory] = useState([]);
  const [txLoading, setTxLoading] = useState(false);

  const openTxHistory = async (product) => {
    setSelectedTxProduct(product);
    setTxLoading(true);
    try {
      const history = await invoke("get_product_transactions_cmd", { token, productId: product.id });
      setTxHistory(history);
    } catch (e) {
      console.error(e);
      toast.error("Failed to load transaction history");
    } finally {
      setTxLoading(false);
    }
  };

  // Reset page to 1 when filters or search change
  useEffect(() => {
    setPage(1);
  }, [searchQuery, selectedCategory, selectedBrand]);

  const handleProductDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this product?")) return;
    mutate({ token, id },
      { onSuccess: () => {
        checkLowStockAlerts();
        toast.success("Product deleted successfully");
      }, onError: (e) => {
        toast.error("Error deleting product: " + e);
      }
    })
  };

  const toggleSort = (field) => {
    if (sortBy === field) {
      setSortOrder(sortOrder === "ASC" ? "DESC" : "ASC");
    } else {
      setSortBy(field);
      setSortOrder("ASC");
    }
  };

  return (
    <div className="">
      <div className="">
            <div className="p-4 border-b">
              <h1 className="text-2xl font-bold tracking-tight">Inventory</h1>
              <p className="text-muted-foreground">Manage products catalogue, categorize, adjust stocks, and more.</p>
            </div>
        <div className="flex flex-col md:flex-row gap-3 items-center justify-between p-4">
          <div className="flex flex-1 gap-2 w-full md:w-auto">
            <div className="relative flex-1 max-w-sm">
              <HugeiconsIcon icon={Search01Icon} size={18} strokeWidth={2} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
              <Input
                placeholder="Search by name, SKU or barcode..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9 bg-card border-border/40"
              />
            </div>
            <Select value={categories?.find((c) => c.id === selectedCategory)?.name || ""}  onValueChange={(value) =>setSelectedCategory(value)}>
                    <SelectTrigger className="w-full max-w-40">
                      <SelectValue placeholder="Select Category" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        <SelectItem value="">None</SelectItem>
                        {categories?.map((c) => (
                          <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
            <Select value={brands?.find((b) => b.id === selectedBrand)?.name || ""}  onValueChange={(value) =>setSelectedBrand(value)}>
                    <SelectTrigger className="w-full max-w-40">
                      <SelectValue placeholder="Select Brand" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        <SelectItem value="">None</SelectItem>
                        {brands?.map((b) => (
                          <SelectItem key={b.id} value={b.id}>{b.name}</SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
          </div>
          {!isReadOnly && (
            <div className="flex gap-2 w-full md:w-auto">
              <Button onClick={() => {
                setParsedProducts([]);
                setImportError(null);
                setIsImportModalOpen(true);
              }} variant="outline" className="flex items-center gap-1.5 border-border/40 hover:bg-muted/10">
                Bulk Upload
              </Button>
              <Button onClick={() => navigate("/inventory/products/new")} className="w-full md:w-auto flex items-center gap-1.5">
                <HugeiconsIcon icon={Add01Icon} size={16} strokeWidth={2} /> Add Product
              </Button>
            </div>
          )}
        </div>
            <div className="overflow-x-auto border-t">
              <Table>
                <TableHeader className={'bg-muted'}>
                  <TableRow className="text-muted-foreground">
                    <TableHead>
                      <Checkbox/>
                    </TableHead>
                    <TableHead onClick={() => toggleSort("name")}>
                      <div className="flex items-center gap-1">
                        Product Name
                        {sortBy === "name" && <SortArrow sortOrder={sortOrder} />}
                      </div>
                    </TableHead>
                    <TableHead onClick={() => toggleSort("sku")}>
                      <div className="flex items-center gap-1">
                        SKU / Barcode
                        {sortBy === "sku" && <SortArrow sortOrder={sortOrder} />}
                      </div>
                    </TableHead>
                    <TableHead>Category / Brand</TableHead>
                    <TableHead onClick={() => toggleSort("cost")}>
                      <div className="flex items-center justify-end gap-1">
                        Cost Price
                        {sortBy === "cost" && <SortArrow sortOrder={sortOrder} />}
                      </div>
                    </TableHead>
                    <TableHead onClick={() => toggleSort("price")}>
                      <div className="flex items-center justify-end gap-1">
                        Selling Price
                        {sortBy === "price" && <SortArrow sortOrder={sortOrder} />}
                      </div>
                    </TableHead>
                    <TableHead onClick={() => toggleSort("stock")}>
                      <div className="flex items-center justify-center gap-1">
                        In Stock
                        {sortBy === "stock" && <SortArrow sortOrder={sortOrder} />}
                      </div>
                    </TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {isLoading ? (
                    <TableRow>
                      <TableCell colSpan={8} className="py-8 text-center text-muted-foreground text-xs">
                        <div className="flex items-center justify-center gap-2">
                          <div className="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
                          Loading products...
                        </div>
                      </TableCell>
                    </TableRow>
                  ) : products.length > 0 ? (
                    products.map((p) => {
                      const isLowStock = p.stock <= p.reorder_level;
                      return (
                        <TableRow key={p.id}>
                          <TableCell>
                            <Checkbox/>
                          </TableCell>
                           <TableCell className="font-medium">
                            <div className="flex items-center gap-3">
                              {p.image_path ? (
                                <img src={p.image_path} alt={p.name} className="h-8 w-8 rounded object-cover border border-border/40 bg-card" />
                              ) : (
                                <div className="h-8 w-8 rounded border border-dashed border-border/40 bg-muted/40 flex items-center justify-center text-[10px] text-muted-foreground shrink-0">
                                  No Img
                                </div>
                              )}
                              <div>
                                <div className="line-clamp-1">{p.name}</div>
                                {p.description && <div className="text-[10px] text-muted-foreground font-normal line-clamp-1 max-w-[200px]">{p.description}</div>}
                              </div>
                            </div>
                          </TableCell>
                          <TableCell>
                            <div className="font-semibold font-mono text-xs">{p.sku}</div>
                            <div className="text-xs text-muted-foreground">{p.barcode || "-"}</div>
                          </TableCell>
                          <TableCell className="text-xs">
                            <div>C: <span className="font-semibold">{p.category_name || "Uncategorized"}</span></div>
                            <div>B: <span className="font-semibold">{p.brand_name || "-"}</span></div>
                          </TableCell>
                          <TableCell className="text-right font-mono">{currencySymbol}{p.cost.toFixed(2)}</TableCell>
                          <TableCell className="text-right font-mono">{currencySymbol}{p.price.toFixed(2)}</TableCell>
                          <TableCell className={`text-center font-bold font-mono ${isLowStock ? 'text-amber-600' : 'text-foreground'}`}>
                            {p.stock} <span className="text-[10px] font-normal text-muted-foreground">{p.unit || 'pcs'}</span>
                          </TableCell>
                          <TableCell>
                            {isLowStock ? (
                              <span className="px-2 py-0.5 text-[10px] font-semibold bg-amber-500/10 text-amber-600 rounded-full border border-amber-500/20">
                                Low Stock
                              </span>
                            ) : (
                              <span className="px-2 py-0.5 text-[10px] font-semibold bg-emerald-500/10 text-emerald-600 rounded-full border border-emerald-500/20">
                                Healthy
                              </span>
                            )}
                          </TableCell>
                          <TableCell className="text-right">
                            <div className="flex justify-end gap-1.5">
                              <Button
                                variant="ghost"
                                size="icon"
                                className="h-8 w-8 text-indigo-500 hover:text-indigo-600 hover:bg-indigo-500/10"
                                onClick={() => openTxHistory(p)}
                                title="View Transaction History Ledger"
                              >
                                <History className="h-4 w-4" />
                              </Button>
                              {!isReadOnly && (
                                <>
                                  <Button variant="ghost" size="icon" className="h-8 w-8 text-primary" onClick={() => navigate(`/inventory/products/edit/${p.id}`)}>
                                    <HugeiconsIcon icon={Edit02Icon} size={14} />
                                  </Button>
                                  {!cannotDelete && (
                                    <Button variant="ghost" size="icon" className="h-8 w-8 text-destructive" onClick={() => handleProductDelete(p.id)}>
                                      <HugeiconsIcon icon={Delete02Icon} size={14} />
                                    </Button>
                                  )}
                                </>
                              )}
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    })
                  ) : (
                    <TableRow>
                      <TableCell colSpan={8} className="py-8 text-center text-muted-foreground text-xs">
                        No products found. Add products to populate list.
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>

              {/* Pagination Controls */}
              <div className="flex flex-col border-b sm:flex-row items-center justify-between gap-4 py-4 px-6 border-t border-border/40 text-xs text-muted-foreground bg-muted/5">
                <div>
                  Showing <span className="font-semibold text-foreground">{totalCount === 0 ? 0 : (page - 1) * limit + 1}</span> to{" "}
                  <span className="font-semibold text-foreground">{Math.min(page * limit, totalCount)}</span> of{" "}
                  <span className="font-semibold text-foreground">{totalCount}</span> products
                </div>
                <div className="flex items-center gap-4">
                  <div className="flex items-center gap-1.5">
                    <span>Rows per page:</span>
                    <select
                      value={limit}
                      onChange={(e) => {
                        setLimit(parseInt(e.target.value));
                        setPage(1);
                      }}
                      className="p-1 border rounded bg-card border-border/40 text-foreground text-xs focus:outline-none"
                    >
                      <option value={5}>5</option>
                      <option value={10}>10</option>
                      <option value={20}>20</option>
                      <option value={50}>50</option>
                    </select>
                  </div>
                  <div className="flex items-center gap-1">
                    <Button
                      variant="outline"
                      size="icon"
                      onClick={() => setPage(Math.max(1, page - 1))}
                      disabled={page === 1}
                    >
                      <HugeiconsIcon icon={ArrowLeft01Icon} size={14} />
                    </Button>
                    <span className="px-2 font-medium">Page {page} of {Math.ceil(totalCount / limit) || 1}</span>
                    <Button
                      variant="outline"
                      size="icon"
                      onClick={() => setPage(Math.min(Math.ceil(totalCount / limit), page + 1))}
                      disabled={page >= Math.ceil(totalCount / limit)}
                    >
                      <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                    </Button>
                  </div>
                </div>
              </div>

            </div>
      </div>

      {/* Excel Bulk Import Dialog */}
      <Dialog open={isImportModalOpen} onOpenChange={setIsImportModalOpen}>
        <DialogContent className="sm:max-w-lg font-sans">
          <DialogHeader>
            <DialogTitle>Excel Product Bulk Upload</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-2">
            <p className="text-xs text-muted-foreground">
              Upload an Excel spreadsheet (`.xlsx` or `.xls`) containing your products. 
              The sheet must contain at least <strong>Name</strong> and <strong>SKU</strong> headers. 
              Optional columns: <em>Barcode, Category, Brand, Cost, Price, Stock, Reorder Level, Unit</em>.
            </p>
            <div className="border border-dashed border-border/60 p-6 rounded-lg text-center bg-muted/5 hover:bg-muted/10 transition-colors">
              <input
                type="file"
                accept=".xlsx, .xls"
                onChange={handleExcelUpload}
                className="hidden"
                id="excel-file-upload"
              />
              <label htmlFor="excel-file-upload" className="cursor-pointer space-y-2 block">
                <div className="text-xs font-semibold text-primary">Click to select files</div>
                <div className="text-[10px] text-muted-foreground">Limit to .xlsx, .xls files</div>
              </label>
            </div>

            {importError && (
              <div className="p-3 bg-destructive/10 text-destructive text-xs rounded-md border border-destructive/20">
                {importError}
              </div>
            )}

            {parsedProducts.length > 0 && (
              <div className="space-y-2">
                <div className="text-xs font-semibold text-emerald-600 flex items-center gap-1">
                  ✓ Validated {parsedProducts.length} products ready for import!
                </div>
                <div className="border rounded-lg max-h-48 overflow-y-auto divide-y divide-border/20 text-xs bg-muted/5">
                  {parsedProducts.map((p, idx) => (
                    <div key={idx} className="p-2 flex justify-between gap-4">
                      <div>
                        <div className="font-semibold">{p.name}</div>
                        <div className="text-[10px] text-muted-foreground">SKU: {p.sku} | Barcode: {p.barcode || "N/A"}</div>
                      </div>
                      <div className="text-right">
                        <div>Price: {currencySymbol}{p.price.toFixed(2)}</div>
                        <div className="text-[10px] text-muted-foreground">Stock: {p.stock} {p.unit}</div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
          <DialogFooter className="flex flex-col sm:flex-row gap-2 justify-between items-center w-full">
            <Button variant="outline" onClick={handleDownloadTemplate} className="mr-auto text-xs border-dashed text-primary hover:text-primary-foreground hover:bg-primary">
              Download Sample Excel Template
            </Button>
            <div className="flex gap-2 w-full sm:w-auto justify-end">
              <Button variant="outline" onClick={() => setIsImportModalOpen(false)}>Cancel</Button>
              <Button 
                onClick={handleBulkImportSubmit} 
                disabled={parsedProducts.length === 0 || isImporting}
                className="bg-primary hover:bg-primary/95 text-primary-foreground font-semibold"
              >
                {isImporting ? "Importing..." : "Start Import"}
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Product Transaction Ledger Modal */}
      <Dialog open={!!selectedTxProduct} onOpenChange={() => setSelectedTxProduct(null)}>
        <DialogContent className="sm:max-w-3xl font-sans">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <History className="h-5 w-5 text-primary" />
              Transaction Ledger: {selectedTxProduct?.name}
            </DialogTitle>
          </DialogHeader>
          <div className="py-4 space-y-4">
            <div className="grid grid-cols-2 gap-4 text-xs bg-muted/40 p-3 rounded-lg border border-border/20">
              <div>
                <span className="text-muted-foreground">SKU:</span> <span className="font-semibold font-mono text-foreground">{selectedTxProduct?.sku}</span>
              </div>
              <div>
                <span className="text-muted-foreground">Current Stock:</span> <span className="font-semibold text-foreground">{selectedTxProduct?.stock} {selectedTxProduct?.unit || 'pcs'}</span>
              </div>
            </div>

            <div className="max-h-[400px] overflow-y-auto border rounded-lg">
              <Table>
                <TableHeader className="bg-muted/80">
                  <TableRow>
                    <TableHead className="text-xs">Date</TableHead>
                    <TableHead className="text-xs">Type</TableHead>
                    <TableHead className="text-xs">Reference</TableHead>
                    <TableHead className="text-xs text-right">Quantity</TableHead>
                    <TableHead className="text-xs text-right">Price</TableHead>
                    <TableHead className="text-xs">User</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {txLoading ? (
                    <TableRow>
                      <TableCell colSpan={6} className="py-8 text-center text-muted-foreground text-xs">
                        <div className="flex items-center justify-center gap-2">
                          <div className="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
                          Loading ledger entries...
                        </div>
                      </TableCell>
                    </TableRow>
                  ) : txHistory.length > 0 ? (
                    txHistory.map((tx, idx) => {
                      const qtyColor = tx.quantity > 0 ? "text-emerald-600 font-bold" : tx.quantity < 0 ? "text-rose-600 font-bold" : "text-foreground";
                      const typeBadge = tx.tx_type === "SALE" ? "bg-blue-500/10 text-blue-600 border-blue-500/20" 
                                      : tx.tx_type === "PURCHASE" ? "bg-emerald-500/10 text-emerald-600 border-emerald-500/20"
                                      : "bg-amber-500/10 text-amber-600 border-amber-500/20";
                      return (
                        <TableRow key={idx}>
                          <TableCell className="text-xs font-mono">{new Date(tx.date).toLocaleString()}</TableCell>
                          <TableCell>
                            <span className={`px-2 py-0.5 text-[10px] font-semibold rounded-full border ${typeBadge}`}>
                              {tx.tx_type}
                            </span>
                          </TableCell>
                          <TableCell className="text-xs max-w-[150px] truncate" title={tx.reference}>{tx.reference}</TableCell>
                          <TableCell className={`text-right text-xs font-mono ${qtyColor}`}>
                            {tx.quantity > 0 ? `+${tx.quantity}` : tx.quantity}
                          </TableCell>
                          <TableCell className="text-right text-xs font-mono">
                            {tx.price > 0 ? `${currencySymbol}${tx.price.toFixed(2)}` : "-"}
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground">{tx.performed_by || "System"}</TableCell>
                        </TableRow>
                      );
                    })
                  ) : (
                    <TableRow>
                      <TableCell colSpan={6} className="py-8 text-center text-muted-foreground text-xs">
                        No transactions recorded for this product.
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </div>
          </div>
          <DialogFooter>
            <Button type="button" onClick={() => setSelectedTxProduct(null)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      </div>
    // </div>
  );
}

const SortArrow = ({sortOrder})=>{
    return(sortOrder === "ASC" ? <ChevronUp size={16} /> : <ChevronDown size={16} />)
}