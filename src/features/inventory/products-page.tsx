import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { Api } from "@inventory/shared";
import { Plus } from "lucide-react";
import { Avatar } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { StockBadge } from "@/components/app/badges";
import { Page } from "@/components/app/page";
import { SearchInput } from "@/components/app/search-input";
import { useDebounced } from "@/components/app/pickers";
import { useMoney } from "@/lib/business";
import { showQty, showRate } from "@/lib/format";
import { useCommand, useList } from "@/lib/queries";
import { useCan } from "@/stores/session";

const PAGE_SIZE = 100;

export default function ProductsPage() {
  const navigate = useNavigate();
  const money = useMoney();
  const canManage = useCan("products.manage");
  const [search, setSearch] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [lowStockOnly, setLowStockOnly] = useState(false);
  const [includeInactive, setIncludeInactive] = useState(false);
  const [page, setPage] = useState(1);
  const term = useDebounced(search.trim(), 200);
  const categories = useCommand("categories_list", {});
  const query = useList("products_list", {
    search: term || null,
    categoryId: categoryId || null,
    lowStockOnly,
    includeInactive,
    page,
    pageSize: PAGE_SIZE,
  });

  const columns = useMemo<ColumnDef<Api.ProductListItem>[]>(
    () => [
      {
        header: "Item",
        accessorKey: "name",
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-3">
            <Avatar name={row.original.name} size="sm" />
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
        cell: ({ row }) => row.original.categoryName ?? "",
      },
      {
        header: "Stock",
        accessorKey: "stock",
        meta: { align: "right" },
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
        meta: { align: "right" },
        cell: ({ row }) => money(row.original.salePrice),
      },
      {
        header: "Cost",
        accessorKey: "costPrice",
        meta: { align: "right" },
        cell: ({ row }) => money(row.original.costPrice),
      },
      {
        header: "Tax",
        accessorKey: "taxRate",
        meta: { align: "right" },
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
            onChange={(e) => {
              setSearch(e.target.value);
              setPage(1);
            }}
          />
          <NativeSelect
            aria-label="Category"
            value={categoryId}
            onChange={(e) => {
              setCategoryId(e.target.value);
              setPage(1);
            }}
          >
            <NativeSelectOption value="">All categories</NativeSelectOption>
            {categories.data?.map((c) => (
              <NativeSelectOption key={c.id} value={c.id}>
                {c.name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <label className="flex items-center gap-2 text-sm">
            <Checkbox
              checked={lowStockOnly}
              onCheckedChange={(v) => {
                setLowStockOnly(v);
                setPage(1);
              }}
            />
            Low stock only
          </label>
          <label className="flex items-center gap-2 text-sm">
            <Checkbox
              checked={includeInactive}
              onCheckedChange={(v) => {
                setIncludeInactive(v);
                setPage(1);
              }}
            />
            Show inactive
          </label>
        </>
      }
    >
      <DataTable
        viewKey="products"
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        rowKey={(r) => r.variantId}
        onRowClick={(r) => void navigate(`/inventory/products/${r.productId}`)}
        paging={
          query.data
            ? { page, pageSize: PAGE_SIZE, total: query.data.total, onPage: setPage }
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
