import { useState } from "react";
import type { Api } from "@inventory/shared";
import { PackageX } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SelectField } from "@/components/ui/select-field";
import type { ColumnDef } from "@/components/app/data-table";
import { DataTable } from "@/components/app/data-table";
import { confirm } from "@/components/app/dialogs";
import { Page, QueryView } from "@/components/app/page";
import { requestId } from "@/api/v2";
import { useMoney, usePublicSettings } from "@/lib/business";
import { showDate, showQty } from "@/lib/format";
import { useAction, useCommand } from "@/lib/queries";
import { useCan } from "@/stores/session";

/** Expired and near-expiry batches, soonest first, with a write-off for each. */
export default function ExpiryPage() {
  const money = useMoney();
  const settings = usePublicSettings();
  const canWriteOff = useCan("stock.adjust");
  const [days, setDays] = useState("");
  const rows = useCommand("inventory_expiry_list", {
    days: days ? Number(days) : null,
    warehouseId: null,
  });
  const writeOff = useAction("stock_write_off", {
    invalidates: [
      "inventory_expiry_list",
      "inventory_expiry_summary",
      "products_list",
      "notifications_list",
    ],
    success: (doc) => `Written off as ${doc.docNo}`,
  });
  const policy = settings.data?.inventory.expiredStockPolicy ?? "block";
  const columns: ColumnDef<Api.ExpiryRow>[] = [
    {
      id: "name",
      header: "Item",
      cell: ({ row }) => (
        <span className="flex flex-col">
          <span className="font-medium">{row.original.name}</span>
          <span className="text-xs text-muted-foreground">{row.original.sku}</span>
        </span>
      ),
    },
    { id: "batch", header: "Batch", cell: ({ row }) => row.original.batchNo },
    {
      id: "expiry",
      header: "Expires",
      cell: ({ row }) => (
        <span className="flex flex-col">
          {showDate(row.original.expiryDate)}
          <span className="text-xs">
            {row.original.expired ? (
              <Badge variant="destructive">
                Expired {Math.abs(row.original.daysLeft)} days ago
              </Badge>
            ) : (
              <Badge variant="warning">
                {row.original.daysLeft === 0 ? "Today" : `In ${row.original.daysLeft} days`}
              </Badge>
            )}
          </span>
        </span>
      ),
    },
    { id: "store", header: "Store", cell: ({ row }) => row.original.warehouseName },
    {
      id: "qty",
      header: "In stock",
      meta: { align: "right" },
      cell: ({ row }) => showQty(row.original.qty),
    },
    {
      id: "value",
      header: "Value",
      meta: { align: "right" },
      cell: ({ row }) => money(row.original.value),
    },
    {
      id: "actions",
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) =>
        canWriteOff ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              void confirm({
                title: `Write off batch ${row.original.batchNo}?`,
                description: `${showQty(row.original.qty)} of ${row.original.name} (${money(row.original.value)}) leaves the stock as a stock adjustment.`,
                confirmLabel: "Write off",
                destructive: true,
              }).then((ok) => {
                if (ok === false) return;
                writeOff.mutate({
                  input: {
                    idempotencyKey: requestId(),
                    variantId: row.original.variantId,
                    batchId: row.original.batchId,
                    warehouseId: row.original.warehouseId,
                    notes: null,
                  },
                });
              })
            }
          >
            <PackageX /> Write off
          </Button>
        ) : null,
    },
  ];
  return (
    <Page
      title="Expiry"
      description={
        policy === "block"
          ? "Expired batches are never sold. Sell near-expiry stock first, return it to the supplier or write it off."
          : policy === "warn"
            ? "Expired batches sell only when nothing else is left, with a warning."
            : "Expired batches sell like any other batch (Settings → Inventory)."
      }
      actions={
        <SelectField
          aria-label="Look ahead"
          className="w-48"
          value={days}
          onValueChange={setDays}
          options={[
            { value: "", label: "Warning window (settings)" },
            { value: "0", label: "Expired only" },
            { value: "7", label: "Next 7 days" },
            { value: "30", label: "Next 30 days" },
            { value: "90", label: "Next 90 days" },
            { value: "180", label: "Next 6 months" },
          ]}
        />
      }
    >
      <QueryView query={rows}>
        {(data) => (
          <DataTable
            columns={columns}
            data={data}
            rowKey={(r) => `${r.batchId}-${r.warehouseId}`}
            empty={{
              title: "Nothing expired or expiring soon",
              description: "Batches with an expiry date show here as their date comes near.",
            }}
          />
        )}
      </QueryView>
    </Page>
  );
}
