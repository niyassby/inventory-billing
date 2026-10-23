import { useMemo } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { Api } from "@inventory/shared";
import { Plus } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { SelectField } from "@/components/ui/select-field";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { StockBadge } from "@/components/app/badges";
import { ProductImage } from "@/components/app/product-image";
import { Page } from "@/components/app/page";
import { SearchInput } from "@/components/app/search-input";
import { useMoney } from "@/lib/business";
import { showQty, showRate } from "@/lib/format";
import { useCommand, useList } from "@/lib/queries";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useCan } from "@/stores/session";

export default function ProductsPage() {
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("products.manage");
  // The list starts in name order; saying so lets the Item header show it and the first click reverse it.
  const table = useTableState("products", {
    sort: "name",
    filters: { q: "", category: "", low: "", inactive: "" },
  });
  const [search, setSearch] = useSearchFilter(table);
  const term = table.filter("q");
  const categories = useCommand("categories_list", {});
  const query = useList("products_list", {
    search: term || null,
    categoryId: table.filter("category") || null,
    lowStockOnly: table.filter("low") === "1",
    includeInactive: table.filter("inactive") === "1",
    ...table.input,
  });

  const columns = useMemo<ColumnDef<Api.ProductListItem>[]>(
    () => [
      {
        header: "Item",
        accessorKey: "name",
        meta: { sortKey: "name" },
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-3">
            <ProductImage
              productId={row.original.productId}
              imageKey={row.original.imageKey}
              name={row.original.name}
              size="md"
            />
            <div className="min-w-0">
              <p className="truncate font-medium">
                {row.original.name}
                {!row.original.isActive && (
                  <Badge variant="outline" className="ml-2">
                    Inactive
                  </Badge>
                )}
              </p>
              <p className="text-caption">
                {row.original.sku}
                {row.original.barcode ? ` · ${row.original.barcode}` : ""}
              </p>
            </div>
          </div>
        ),
      },
      {
        header: "Category",
        accessorFn: (r) => r.categoryName ?? "",
        meta: { sortKey: "category" },
        cell: ({ row }) => row.original.categoryName ?? "",
      },
      {
        header: "Stock",
        accessorKey: "stock",
        meta: { align: "right", sortKey: "stock" },
        cell: ({ row }) => (
          <span className="inline-flex items-center justify-end gap-2">
            <StockBadge stock={row.original.stock} reorderLevel={row.original.reorderLevel} />
            <span className="num">
              {showQty(row.original.stock)} {row.original.unitCode}
            </span>
          </span>
        ),
      },
      {
        header: "Price",
        accessorKey: "salePrice",
        meta: { align: "right", sortKey: "price" },
        cell: ({ row }) => money(row.original.salePrice),
      },
      {
        header: "Cost",
        accessorKey: "costPrice",
        meta: { align: "right", sortKey: "cost" },
        cell: ({ row }) => money(row.original.costPrice),
      },
      {
        header: "Tax",
        accessorKey: "taxRate",
        meta: { align: "right", sortKey: "tax" },
        cell: ({ row }) => showRate(row.original.taxRate),
      },
    ],
    [money],
  );

  return (
    <Page
      title="Products"
      description="Everything you sell or buy, with stock across all stores."
      actions={
        canManage && (
          <Button render={<Link to="/inventory/products/new" />} nativeButton={false}>
            <Plus /> New product
          </Button>
        )
      }
      toolbar={
        <>
          <SearchInput
            aria-label="Search products"
            placeholder="Search name, SKU or barcode"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <SelectField
            aria-label="Category"
            className="w-48"
            value={table.filter("category")}
            onValueChange={(v) => table.setFilter("category", v)}
            options={[
              { value: "", label: "All categories" },
              ...(categories.data ?? []).map((c) => ({ value: c.id, label: c.name })),
            ]}
          />
          <label className="flex items-center gap-2 text-sm">
            <Checkbox
              checked={table.filter("low") === "1"}
              onCheckedChange={(v) => table.setFilter("low", v ? "1" : "")}
            />
            Low stock only
          </label>
          <label className="flex items-center gap-2 text-sm">
            <Checkbox
              checked={table.filter("inactive") === "1"}
              onCheckedChange={(v) => table.setFilter("inactive", v ? "1" : "")}
            />
            Show inactive
          </label>
        </>
      }
    >
      <DataTable
        viewKey="products"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        rowKey={(r) => r.variantId}
        onRowClick={(r) => void navigate(`/inventory/products/${r.productId}`)}
        paging={
          query.data
            ? {
                page: table.state.page,
                pageSize: table.state.size,
                total: query.data.total,
                onPage: table.setPage,
                onPageSize: table.setSize,
              }
            : undefined
        }
        empty={{
          title: term ? "No products match" : "No products yet",
          description: term
            ? "Try another name, SKU or barcode."
            : "Add your first product to start billing.",
          action:
            canManage && !term ? (
              <Button render={<Link to="/inventory/products/new" />} nativeButton={false}>
                <Plus /> New product
              </Button>
            ) : undefined,
        }}
      />
    </Page>
  );
}
