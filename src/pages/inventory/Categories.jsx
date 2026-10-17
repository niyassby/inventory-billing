import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Delete02Icon,
  BarcodeIcon,
  Tag01Icon,
  ArchiveIcon,
  ShuffleIcon
} from "@hugeicons/core-free-icons";
import { useBrands, useCategories, useCreateBrand, useCreateCategorie, useDeleteBrand, useDeleteCategory } from "@/hooks/tanstack/useCategorie&Brand";
import { useUnits, useCreateUnit, useDeleteUnit } from "@/hooks/tanstack/useUnits";
import { toast } from "sonner";

export default function Categories() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "accountant";
  const cannotDelete = userRole === "staff" || userRole === "accountant";

  // Data State
  const {data: categories, isLoading: catLoading}= useCategories({token});
  const {data: brands, isLoading: brandLoading}= useBrands({token});
  const {data: units, isLoading: unitLoading}= useUnits({token});
  
  const {mutate: createCategory, isPending: isCreatingCategory}=useCreateCategorie()
  const {mutate: createBrand, isPending: isCreatingBrand}=useCreateBrand()
  const {mutate: createUnit, isPending: isCreatingUnit}=useCreateUnit()
  
  const {mutate: deleteCategory}=useDeleteCategory()
  const {mutate: deleteBrand}=useDeleteBrand()
  const {mutate: deleteUnit}=useDeleteUnit()

  // Modals State
  const [isCategoryModalOpen, setIsCategoryModalOpen] = useState(false);
  const [categoryName, setCategoryName] = useState("");

  const [isBrandModalOpen, setIsBrandModalOpen] = useState(false);
  const [brandName, setBrandName] = useState("");

  const [isUnitModalOpen, setIsUnitModalOpen] = useState(false);
  const [unitName, setUnitName] = useState("");
  const [unitShortName, setUnitShortName] = useState("");


  // Category and Brand submits
  const handleCategorySubmit = async (e) => {
    e.preventDefault();
    createCategory({token, name: categoryName}, {
      onSuccess: () => {
        setCategoryName("");
        setIsCategoryModalOpen(false);
        toast.success("Category created successfully");
      },
      onError: (error) => {
        toast.error("Error creating category: " + error);
      }
    })
    
  };

  const handleBrandSubmit = async (e) => {
    e.preventDefault();
    createBrand({token, name: brandName}, {
      onSuccess: () => {
        setBrandName("");
        setIsBrandModalOpen(false);
        toast.success("Brand created successfully");
      },
      onError: (error) => {
        toast.error("Error creating brand: " + error);
      }
    })
  };

  const handleCategoryDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this category?")) return;
    deleteCategory({token, id}, {
      onSuccess: () => {
        toast.success("Category deleted successfully");
      },
      onError: (error) => {
        toast.error("Error deleting category: " + error);
      }
    })
  };

  const handleBrandDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this brand?")) return;
    deleteBrand({token, id}, {
      onSuccess: () => {
        toast.success("Brand deleted successfully");
      },
      onError: (error) => {
        toast.error("Error deleting brand: " + error);
      }
    })
  };

  const handleUnitSubmit = async (e) => {
    e.preventDefault();
    createUnit({token, name: unitName, shortName: unitShortName}, {
      onSuccess: () => {
        setUnitName("");
        setUnitShortName("");
        setIsUnitModalOpen(false);
        toast.success("Unit created successfully");
      },
      onError: (error) => {
        toast.error("Error creating unit: " + error);
      }
    })
  };

  const handleUnitDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this unit?")) return;
    deleteUnit({token, id}, {
      onSuccess: () => {
        toast.success("Unit deleted successfully");
      },
      onError: (error) => {
        toast.error("Error deleting unit: " + error);
      }
    })
  };

  return (
    <div className="space-y-6 p-4 font-sans">

      {catLoading || brandLoading || unitLoading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="grid gap-6 grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {/* Category Card */}
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
              <div>
                <CardTitle>Categories</CardTitle>
                <CardDescription>Classify products for advanced accounting reports.</CardDescription>
              </div>
              {!isReadOnly && (
                <Button size="sm" onClick={() => setIsCategoryModalOpen(true)} className="flex items-center gap-1">
                  <HugeiconsIcon icon={Add01Icon} size={14} /> New Category
                </Button>
              )}
            </CardHeader>
            <CardContent>
              <div className="divide-y divide-border/20">
                {categories?.map((c) => (
                  <div key={c.id} className="flex justify-between items-center py-2.5">
                    <span className="text-sm font-medium">{c.name}</span>
                    {!cannotDelete && (
                      <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive" onClick={() => handleCategoryDelete(c.id)}>
                        <HugeiconsIcon icon={Delete02Icon} size={14} />
                      </Button>
                    )}
                  </div>
                ))}
                {categories?.length === 0 && (
                  <p className="text-xs text-muted-foreground py-4 text-center">No categories defined yet.</p>
                )}
              </div>
            </CardContent>
          </Card>

          {/* Brand Card */}
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
              <div>
                <CardTitle>Brands</CardTitle>
                <CardDescription>Associate brands with products for tracking statistics.</CardDescription>
              </div>
              {!isReadOnly && (
                <Button size="sm" onClick={() => setIsBrandModalOpen(true)} className="flex items-center gap-1">
                  <HugeiconsIcon icon={Add01Icon} size={14} /> New Brand
                </Button>
              )}
            </CardHeader>
            <CardContent>
              <div className="divide-y divide-border/20">
                {brands?.map((b) => (
                  <div key={b.id} className="flex justify-between items-center py-2.5">
                    <span className="text-sm font-medium">{b.name}</span>
                    {!cannotDelete && (
                      <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive" onClick={() => handleBrandDelete(b.id)}>
                        <HugeiconsIcon icon={Delete02Icon} size={14} />
                      </Button>
                    )}
                  </div>
                ))}
                {brands?.length === 0 && (
                  <p className="text-xs text-muted-foreground py-4 text-center">No brands defined yet.</p>
                )}
              </div>
            </CardContent>
          </Card>

          {/* Units Card */}
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
              <div>
                <CardTitle>Units of Measure</CardTitle>
                <CardDescription>Manage unit lookups for products.</CardDescription>
              </div>
              {!isReadOnly && (
                <Button size="sm" onClick={() => setIsUnitModalOpen(true)} className="flex items-center gap-1">
                  <HugeiconsIcon icon={Add01Icon} size={14} /> New Unit
                </Button>
              )}
            </CardHeader>
            <CardContent>
              <div className="divide-y divide-border/20">
                {units?.map((u) => (
                  <div key={u.id} className="flex justify-between items-center py-2.5">
                    <span className="text-sm font-medium">{u.name} <span className="text-xs text-muted-foreground">({u.short_name})</span></span>
                    {!cannotDelete && (
                      <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive" onClick={() => handleUnitDelete(u.id)}>
                        <HugeiconsIcon icon={Delete02Icon} size={14} />
                      </Button>
                    )}
                  </div>
                ))}
                {units?.length === 0 && (
                  <p className="text-xs text-muted-foreground py-4 text-center">No units defined yet.</p>
                )}
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Category Modal */}
      <Dialog open={isCategoryModalOpen} onOpenChange={setIsCategoryModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>New Category</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCategorySubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="cat_name">Category Name *</Label>
              <Input
                id="cat_name"
                value={categoryName}
                onChange={(e) => setCategoryName(e.target.value)}
                required
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsCategoryModalOpen(false)}>Cancel</Button>
              <Button type="submit" disabled={isCreatingCategory}>{isCreatingCategory ? "Adding..." : "Add Category"}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Brand Modal */}
      <Dialog open={isBrandModalOpen} onOpenChange={setIsBrandModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>New Brand</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleBrandSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="brand_name">Brand Name *</Label>
              <Input
                id="brand_name"
                value={brandName}
                onChange={(e) => setBrandName(e.target.value)}
                required
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsBrandModalOpen(false)}>Cancel</Button>
              <Button type="submit" disabled={isCreatingBrand}>{isCreatingBrand ? "Adding..." : "Add Brand"}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Unit Modal */}
      <Dialog open={isUnitModalOpen} onOpenChange={setIsUnitModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>New Unit</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleUnitSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="unit_name">Unit Name *</Label>
              <Input
                id="unit_name"
                value={unitName}
                onChange={(e) => setUnitName(e.target.value)}
                placeholder="e.g. Kilograms"
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="unit_short">Short Name *</Label>
              <Input
                id="unit_short"
                value={unitShortName}
                onChange={(e) => setUnitShortName(e.target.value)}
                placeholder="e.g. kg"
                required
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsUnitModalOpen(false)}>Cancel</Button>
              <Button type="submit" disabled={isCreatingUnit}>{isCreatingUnit ? "Adding..." : "Add Unit"}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
