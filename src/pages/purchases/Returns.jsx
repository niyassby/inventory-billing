import React from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01Icon, ShoppingBagIcon, InvoiceIcon } from "@hugeicons/core-free-icons";

export default function Returns() {
  const navigate = useNavigate();

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
            variant="outline"
            onClick={() => navigate("/purchases/bills")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={InvoiceIcon} size={16} strokeWidth={2} /> Supplier Bills
          </Button>
          <Button
            variant="default"
            onClick={() => navigate("/purchases/returns")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Add01Icon} size={16} strokeWidth={2} /> Purchase Returns
          </Button>
        </div>
      </div>

      <div className="space-y-4">
        <h3 className="text-lg font-semibold">Purchase Returns & Debit Notes</h3>

        <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
          <CardContent className="flex flex-col items-center justify-center p-12 text-center">
            <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-primary/10 text-primary">
              <HugeiconsIcon icon={ShoppingBagIcon} size={24} />
            </div>
            <h4 className="text-md mb-1 font-semibold text-foreground">
              No Purchase Returns Processed
            </h4>
            <p className="max-w-sm text-sm text-muted-foreground">
              All supplier shipments are received and correct. If you return goods due to defects or
              excess, debit notes will be listed here.
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
