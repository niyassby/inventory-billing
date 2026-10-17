import { useState, useEffect } from "react";
import { useNavigate, useParams, Link } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { toast } from "sonner";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowLeft02Icon,
  SaveIcon,
  ArchiveIcon,
  Tag01Icon,
  ShuffleIcon,
  BarcodeIcon,
} from "@hugeicons/core-free-icons";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useCreateOrUpdateProduct } from "@/hooks/tanstack/useProducts";
import { useBrands, useCategories } from "@/hooks/tanstack/useCategorie&Brand";
import { useUnits } from "@/hooks/tanstack/useUnits";

export default function AddEditProduct() {
  const navigate = useNavigate();
  const { id } = useParams();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const checkLowStockAlerts = useStore((state) => state.checkLowStockAlerts);
  const { mutate: createOrUpdateProduct, isPending: isSaving } = useCreateOrUpdateProduct();

  const isEditMode = !!id;

  // Form State
  const [form, setForm] = useState({
    name: "",
    sku: "",
    barcode: "",
    category_id: "",
    brand_id: "",
    cost: 0,
    price: 0,
    reorder_level: 5,
    is_expiry_tracked: 0,
    expiry_date: "",
    unit: "pcs",
    unit_id: "",
    tax_id: "",
    offer_price: 0,
    description: "",
    image_path: "",
    initial_stock: 0,
  });

  const { data: categories } = useCategories({ token });
  const { data: brands } = useBrands({ token });
  const { data: dbUnits } = useUnits({ token });
  const [taxRates, setTaxRates] = useState([]);
  const [loading, setLoading] = useState(false);

  // Fetch Reference Data
  const fetchMetadata = async () => {
    if (!isEditMode) {
      setForm((f) => ({
        ...f,
        sku: "SKU-" + Math.floor(100000 + Math.random() * 900000),
        barcode:
          String(Math.floor(100000 + Math.random() * 900000)) +
          String(Math.floor(100000 + Math.random() * 900000)),
      }));
    }
  };

  // Fetch Product Details (if editing)
  const fetchProductDetails = async () => {
    setLoading(true);
    try {
      const prod = await invoke("get_product_by_id_cmd", { token, id });
      setForm({
        name: prod.name,
        sku: prod.sku,
        barcode: prod.barcode || "",
        category_id: prod.category_id || "",
        brand_id: prod.brand_id || "",
        cost: prod.cost,
        price: prod.price,
        reorder_level: prod.reorder_level,
        is_expiry_tracked: prod.is_expiry_tracked,
        expiry_date: prod.expiry_date || "",
        unit: prod.unit || "pcs",
        unit_id: prod.unit_id || "",
        tax_id: prod.tax_id || "",
        offer_price: prod.offer_price || 0,
        description: prod.description || "",
        image_path: prod.image_path || "",
        initial_stock: prod.stock, // read-only reference
      });
    } catch (e) {
      console.error("Failed to load product details:", e);
      toast.error("Error loading product details");
      navigate("/inventory/products");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (token) {
      fetchMetadata();
      if (isEditMode) {
        fetchProductDetails();
      }
      invoke("list_tax_rates_cmd", { token }).then(setTaxRates).catch(console.error);
    }
  }, [token, id]);

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (isEditMode) {
      createOrUpdateProduct(
        {
          cmd: "update_product_cmd",
          data: {
            token,
            id,
            name: form.name,
            sku: form.sku,
            barcode: form.barcode || null,
            qrCode: form.barcode || null,
            categoryId: form.category_id || null,
            brandId: form.brand_id || null,
            cost: parseFloat(form.cost),
            price: parseFloat(form.price),
            reorderLevel: parseFloat(form.reorder_level),
            isExpiryTracked: parseInt(form.is_expiry_tracked),
            expiryDate: form.expiry_date || null,
            unit: form.unit,
            unitId: form.unit_id || null,
            taxId: form.tax_id || null,
            offerPrice: form.offer_price ? parseFloat(form.offer_price) : 0.0,
            description: form.description || null,
            imagePath: form.image_path || null,
          },
        },
        {
          onSuccess: () => {
            toast.success("Product updated successfully");
            navigate("/inventory/products");
          },
          onError: (e) => {
            toast.error("Error updating product: " + e);
          },
        },
      );
    } else {
      createOrUpdateProduct(
        {
          cmd: "create_product_cmd",
          data: {
            token,
            name: form.name,
            sku: form.sku,
            barcode: form.barcode || null,
            qrCode: form.barcode || null,
            categoryId: form.category_id || null,
            brandId: form.brand_id || null,
            cost: parseFloat(form.cost),
            price: parseFloat(form.price),
            stock: parseFloat(form.initial_stock || 0),
            reorderLevel: parseFloat(form.reorder_level),
            isExpiryTracked: parseInt(form.is_expiry_tracked),
            expiryDate: form.expiry_date || null,
            unit: form.unit,
            unitId: form.unit_id || null,
            taxId: form.tax_id || null,
            offerPrice: form.offer_price ? parseFloat(form.offer_price) : 0.0,
            description: form.description || null,
            imagePath: form.image_path || null,
          },
        },
        {
          onSuccess: () => {
            toast.success("Product created successfully");
            checkLowStockAlerts();
            navigate("/inventory/products");
          },
          onError: (e) => {
            toast.error("Error creating product: " + e);
          },
        },
      );
    }
  };
  {
    loading && (
      <div className="flex h-[40vh] items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
      </div>
    );
  }

  return (
    <div className="p-6 pb-10">
      <form onSubmit={handleSubmit} className="max-w-4xl bg-accent/20">
        <div>
          <div className="space-y-6 p-6">
            <div className="text-lg font-semibold">
              {isEditMode ? "Edit Product" : "Add New Product"}
            </div>
            <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
              {/* Product Image Uploader */}
              <div className="space-y-2 md:col-span-2">
                <Label className="text-sm font-semibold">Product Image</Label>
                <div className="flex flex-col items-center gap-4 rounded-lg border border-dashed border-border/40 bg-card/20 p-4 sm:flex-row">
                  {form.image_path ? (
                    <div className="relative h-24 w-24 shrink-0 overflow-hidden rounded border border-border/40 bg-card">
                      <img
                        src={form.image_path}
                        alt="Preview"
                        className="h-full w-full object-cover"
                      />
                      <button
                        type="button"
                        onClick={() => setForm({ ...form, image_path: "" })}
                        className="text-destructive-foreground absolute top-1 right-1 flex h-5 w-5 items-center justify-center rounded-full bg-destructive text-xs shadow hover:bg-destructive/90"
                      >
                        &times;
                      </button>
                    </div>
                  ) : (
                    <div className="flex h-24 w-24 shrink-0 items-center justify-center rounded border border-dashed border-border/40 bg-card/10 text-xs text-muted-foreground">
                      No Image
                    </div>
                  )}
                  <div className="w-full space-y-1.5">
                    <Input
                      type="file"
                      accept="image/*"
                      onChange={(e) => {
                        const file = e.target.files?.[0];
                        if (!file) return;

                        const reader = new FileReader();
                        reader.onload = (event) => {
                          const img = new Image();
                          img.onload = () => {
                            const canvas = document.createElement("canvas");
                            const MAX_WIDTH = 300;
                            const MAX_HEIGHT = 300;
                            let width = img.width;
                            let height = img.height;

                            if (width > height) {
                              if (width > MAX_WIDTH) {
                                height *= MAX_WIDTH / width;
                                width = MAX_WIDTH;
                              }
                            } else {
                              if (height > MAX_HEIGHT) {
                                width *= MAX_HEIGHT / height;
                                height = MAX_HEIGHT;
                              }
                            }

                            canvas.width = width;
                            canvas.height = height;
                            const ctx = canvas.getContext("2d");
                            ctx.drawImage(img, 0, 0, width, height);

                            const compressedDataUrl = canvas.toDataURL("image/jpeg", 0.7);
                            setForm({ ...form, image_path: compressedDataUrl });
                          };
                          img.src = event.target.result;
                        };
                        reader.readAsDataURL(file);
                      }}
                      className="max-w-xs cursor-pointer text-xs file:mr-2 file:rounded-md file:border-0 file:bg-primary file:px-2 file:py-1 file:text-xs file:font-semibold file:text-primary-foreground hover:file:bg-primary/95"
                    />
                    <p className="text-xs text-muted-foreground">
                      Supports JPG, PNG. Auto-compressed & scaled to 300x300 for database storage
                      efficiency.
                    </p>
                  </div>
                </div>
              </div>

              {/* General Info */}
              <div className="space-y-2 md:col-span-2">
                <Label htmlFor="name" className="text-sm font-semibold">
                  Product Name *
                </Label>
                <Input
                  id="name"
                  value={form.name}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                  placeholder="e.g. Wireless Mouse, Leather Notebook"
                  required
                />
              </div>

              <div className="space-y-2 md:col-span-2">
                <Label htmlFor="description" className="text-sm font-semibold">
                  Product Description
                </Label>
                <textarea
                  id="description"
                  value={form.description}
                  onChange={(e) => setForm({ ...form, description: e.target.value })}
                  placeholder="Enter detailed product specifications, details, or metadata..."
                  className="min-h-[80px] w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground focus:ring-1 focus:ring-primary focus:outline-none"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="sku" className="text-sm font-semibold">
                  SKU *
                </Label>
                <Input
                  id="sku"
                  value={form.sku}
                  onChange={(e) => setForm({ ...form, sku: e.target.value })}
                  placeholder="e.g. SKU-10023"
                  required
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="barcode" className="text-sm font-semibold">
                  Barcode (EAN13/CODE128)
                </Label>
                <Input
                  id="barcode"
                  value={form.barcode}
                  onChange={(e) => setForm({ ...form, barcode: e.target.value })}
                  placeholder="Scan or enter barcode"
                />
              </div>

              {/* Categories & Brands */}
              <div className="space-y-2">
                <Label htmlFor="category" className="text-sm font-semibold">
                  Category
                </Label>
                <Select
                  value={form.category_id || ""}
                  onValueChange={(value) => setForm({ ...form, category_id: value })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select Category" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="none">None</SelectItem>
                      {categories?.map((c) => (
                        <SelectItem key={c.id} value={c.id}>
                          {c.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="brand" className="text-sm font-semibold">
                  Brand
                </Label>
                <Select
                  value={form.brand_id || ""}
                  onValueChange={(value) => setForm({ ...form, brand_id: value })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select Brand" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="none">None</SelectItem>
                      {brands?.map((b) => (
                        <SelectItem key={b.id} value={b.id}>
                          {b.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>

              {/* Pricing & Stock */}
              <div className="space-y-2">
                <Label htmlFor="cost" className="text-sm font-semibold">
                  Cost Price ({currencySymbol}) *
                </Label>
                <Input
                  id="cost"
                  type="number"
                  step="0.01"
                  value={form.cost}
                  onChange={(e) => setForm({ ...form, cost: e.target.value })}
                  required
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="price" className="text-sm font-semibold">
                  Selling Price ({currencySymbol}) *
                </Label>
                <Input
                  id="price"
                  type="number"
                  step="0.01"
                  value={form.price}
                  onChange={(e) => setForm({ ...form, price: e.target.value })}
                  required
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="offer_price" className="text-sm font-semibold">
                  Offer Price ({currencySymbol})
                </Label>
                <Input
                  id="offer_price"
                  type="number"
                  step="0.01"
                  value={form.offer_price}
                  onChange={(e) => setForm({ ...form, offer_price: e.target.value })}
                  placeholder="Promotional price (optional)"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="tax_rate" className="text-sm font-semibold">
                  Tax Rate
                </Label>
                <Select
                  value={form.tax_id || ""}
                  onValueChange={(val) => setForm({ ...form, tax_id: val })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select Tax Rate" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="none">None (0%)</SelectItem>
                      {taxRates?.map((tr) => (
                        <SelectItem key={tr.id} value={tr.id}>
                          {tr.name} ({tr.rate}%)
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="unit" className="text-sm font-semibold">
                  Unit of Measure *
                </Label>
                <Select
                  value={form.unit_id || ""}
                  onValueChange={(val) => {
                    const selectedUnit = dbUnits?.find((u) => u.id === val);
                    setForm({
                      ...form,
                      unit_id: val,
                      unit: selectedUnit ? selectedUnit.short_name : "pcs",
                    });
                  }}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select Unit" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {dbUnits?.map((u) => (
                        <SelectItem key={u.id} value={u.id}>
                          {u.name} ({u.short_name})
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>

              {/* Initial Stock (Only for Create Mode) */}
              <div className="space-y-2">
                <Label htmlFor="initial_stock" className="text-sm font-semibold">
                  {isEditMode ? "Current Stock (Read-only)" : "Initial Stock Level"}
                </Label>
                <Input
                  id="initial_stock"
                  type="number"
                  value={form.initial_stock}
                  onChange={(e) => setForm({ ...form, initial_stock: e.target.value })}
                  disabled={isEditMode}
                  className={isEditMode ? "cursor-not-allowed bg-muted opacity-75" : ""}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="reorder_level" className="text-sm font-semibold">
                  Reorder Alert Level
                </Label>
                <Input
                  id="reorder_level"
                  type="number"
                  value={form.reorder_level}
                  onChange={(e) => setForm({ ...form, reorder_level: e.target.value })}
                />
              </div>

              {/* Expiry Tracking */}
              <div className="space-y-2">
                <Label htmlFor="expiry_tracked" className="text-sm font-semibold">
                  Expiry Date Tracked
                </Label>
                <Select
                  value={String(form.is_expiry_tracked)}
                  onValueChange={(value) =>
                    setForm({ ...form, is_expiry_tracked: parseInt(value) })
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Select Expiry Tracking" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="0">No Expiry Tracking</SelectItem>
                      <SelectItem value="1">Track Expiry Date</SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>

              {form.is_expiry_tracked === 1 && (
                <div className="space-y-2 md:col-span-2">
                  <Label htmlFor="expiry_date" className="text-sm font-semibold">
                    Expiry Date
                  </Label>
                  <Input
                    id="expiry_date"
                    type="date"
                    value={form.expiry_date}
                    onChange={(e) => setForm({ ...form, expiry_date: e.target.value })}
                  />
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Form Actions */}
        <div className="flex items-center justify-end gap-3">
          <Button
            type="button"
            variant="outline"
            onClick={() => navigate("/inventory/products")}
            disabled={isSaving}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={isSaving} className="flex min-w-32 items-center gap-1.5">
            {isSaving ? (
              <HugeiconsIcon icon={SaveIcon} className="animate-pulse" size={16} />
            ) : (
              <HugeiconsIcon icon={SaveIcon} size={16} />
            )}
            {isEditMode ? "Save Changes" : "Create Product"}
          </Button>
        </div>
      </form>
    </div>
  );
}
