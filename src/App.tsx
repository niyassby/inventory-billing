import type { ReactNode } from "react";
import { Suspense, lazy, useEffect } from "react";
import { Navigate, Outlet, Route, Routes } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { call } from "@/api/v2";
import { ApprovalDialog, ConfirmDialog } from "@/components/app/dialogs";
import { ErrorState } from "@/components/app/page";
import { ChangePassword, SignIn } from "@/features/auth/sign-in";
import { Setup } from "@/features/auth/setup";
import { Shell } from "@/app/shell";
import { TitleStrip } from "@/components/TitleBar";
import { isTauri } from "@tauri-apps/api/core";
import { useSession } from "@/stores/session";
import "@/stores/approval";

// Route-level code splitting: each screen loads on first visit.
const Dashboard = lazy(() => import("@/features/dashboard/dashboard"));
const PosPage = lazy(() => import("@/features/pos/pos-page"));
const Reports = {
  Index: lazy(() =>
    import("@/features/reports/reports").then((m) => ({ default: m.ReportsIndex })),
  ),
  View: lazy(() => import("@/features/reports/reports").then((m) => ({ default: m.ReportPage }))),
};
const Accounting = {
  Expenses: lazy(() =>
    import("@/features/accounting/accounting-pages").then((m) => ({ default: m.ExpensesPage })),
  ),
  Accounts: lazy(() =>
    import("@/features/accounting/accounting-pages").then((m) => ({ default: m.AccountsPage })),
  ),
  DayClose: lazy(() =>
    import("@/features/accounting/accounting-pages").then((m) => ({ default: m.DayClosePage })),
  ),
};
// Development only: every primitive in the current theme (not part of the packaged app).
const KitchenSink = import.meta.env.DEV ? lazy(() => import("@/features/dev/kitchen-sink")) : null;
const SettingsPage = lazy(() => import("@/features/settings/settings-page"));
const ProductsPage = lazy(() => import("@/features/inventory/products-page"));
const ProductEditor = lazy(() => import("@/features/inventory/product-editor"));
const MastersPage = lazy(() => import("@/features/inventory/masters-page"));
const LabelsPage = lazy(() => import("@/features/printing/labels-page"));
const PriceListsPage = lazy(() => import("@/features/inventory/price-lists-page"));
const ExpiryPage = lazy(() => import("@/features/inventory/expiry-page"));
const Stock = {
  Moves: lazy(() =>
    import("@/features/inventory/stock-pages").then((m) => ({ default: m.StockMovesPage })),
  ),
  Adjust: lazy(() =>
    import("@/features/inventory/stock-pages").then((m) => ({ default: m.AdjustmentPage })),
  ),
  Transfer: lazy(() =>
    import("@/features/inventory/stock-pages").then((m) => ({ default: m.TransferPage })),
  ),
  Count: lazy(() =>
    import("@/features/inventory/stock-pages").then((m) => ({ default: m.CountPage })),
  ),
  Doc: lazy(() =>
    import("@/features/inventory/stock-pages").then((m) => ({ default: m.StockDocumentPage })),
  ),
  Card: lazy(() =>
    import("@/features/inventory/stock-pages").then((m) => ({ default: m.StockCardPage })),
  ),
};
const Parties = {
  List: lazy(() =>
    import("@/features/parties/party-pages").then((m) => ({ default: m.PartiesPage })),
  ),
  Detail: lazy(() =>
    import("@/features/parties/party-pages").then((m) => ({ default: m.PartyPage })),
  ),
  Payments: lazy(() => import("@/features/parties/payments-page")),
};
const Sales = {
  Invoices: lazy(() =>
    import("@/features/sales/invoice-pages").then((m) => ({ default: m.InvoicesPage })),
  ),
  Invoice: lazy(() =>
    import("@/features/sales/invoice-pages").then((m) => ({ default: m.InvoicePage })),
  ),
  Returns: lazy(() =>
    import("@/features/sales/invoice-pages").then((m) => ({ default: m.ReturnsPage })),
  ),
  Return: lazy(() =>
    import("@/features/sales/invoice-pages").then((m) => ({ default: m.ReturnPage })),
  ),
  NewReturn: lazy(() =>
    import("@/features/sales/invoice-pages").then((m) => ({ default: m.NewReturnPage })),
  ),
  Docs: lazy(() =>
    import("@/features/sales/document-pages").then((m) => ({ default: m.SalesDocsPage })),
  ),
  Doc: lazy(() =>
    import("@/features/sales/document-pages").then((m) => ({ default: m.SalesDocPage })),
  ),
  DocEditor: lazy(() =>
    import("@/features/sales/document-pages").then((m) => ({ default: m.SalesDocEditor })),
  ),
};
const Purchases = {
  List: lazy(() =>
    import("@/features/purchases/purchase-pages").then((m) => ({ default: m.PurchasesPage })),
  ),
  Editor: lazy(() =>
    import("@/features/purchases/purchase-pages").then((m) => ({ default: m.PurchaseEditor })),
  ),
  Doc: lazy(() =>
    import("@/features/purchases/purchase-pages").then((m) => ({ default: m.PurchaseDocPage })),
  ),
  Receipt: lazy(() =>
    import("@/features/purchases/purchase-pages").then((m) => ({ default: m.ReceiptPage })),
  ),
  NewReturn: lazy(() =>
    import("@/features/purchases/purchase-pages").then((m) => ({
      default: m.NewPurchaseReturnPage,
    })),
  ),
};

/** Screens outside the app shell get the window's title strip (the shell's header has its own controls). */
function Framed({ children }: { children: ReactNode }) {
  return (
    <div className={isTauri() ? "with-titlebar" : undefined}>
      <TitleStrip />
      {children}
    </div>
  );
}

export function FullScreenSpinner({ label }: { label: string }) {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-background font-sans text-foreground">
      <div className="flex flex-col items-center gap-4" role="status" aria-live="polite">
        <div className="h-10 w-10 animate-spin rounded-full border-4 border-primary border-t-transparent" />
        <p className="text-sm font-medium text-muted-foreground">{label}</p>
      </div>
    </div>
  );
}

function FramedSpinner({ label }: { label: string }) {
  return (
    <Framed>
      <FullScreenSpinner label={label} />
    </Framed>
  );
}

function Lazy({ children }: { children: ReactNode }) {
  return <Suspense fallback={<FullScreenSpinner label="Loading…" />}>{children}</Suspense>;
}

/** First run → setup; then sign-in; then (if needed) a new password; then the app. */
function Gate() {
  const setup = useQuery({
    queryKey: ["setup_status"],
    queryFn: () => call("setup_status", {}),
    staleTime: Infinity,
  });
  const status = useSession((s) => s.status);
  const mustChange = useSession((s) => s.mustChangePassword);
  const restore = useSession((s) => s.restore);
  useEffect(() => {
    if (status === "checking") void restore();
  }, [status, restore]);

  if (setup.isPending) return <FramedSpinner label="Starting…" />;
  if (setup.error) {
    return (
      <Framed>
        <div className="flex h-screen items-center justify-center p-6">
          <ErrorState error={setup.error} onRetry={() => void setup.refetch()} />
        </div>
      </Framed>
    );
  }
  if (setup.data.needsSetup)
    return (
      <Framed>
        <Setup status={setup.data} />
      </Framed>
    );
  if (status === "checking") return <FramedSpinner label="Signing you in…" />;
  if (status === "signedOut") {
    return (
      <Framed>
        <SignIn />
      </Framed>
    );
  }
  if (mustChange) {
    return (
      <Framed>
        <ChangePassword />
      </Framed>
    );
  }
  return <AppRoutes />;
}

function AppRoutes() {
  return (
    <Routes>
      <Route
        path="/pos"
        element={
          <Framed>
            <Lazy>
              <PosPage />
            </Lazy>
          </Framed>
        }
      />
      <Route element={<Shell />}>
        <Route
          element={
            <Lazy>
              <Outlet />
            </Lazy>
          }
        >
          <Route index element={<Dashboard />} />
          <Route path="inventory/products" element={<ProductsPage />} />
          <Route path="inventory/products/:id" element={<ProductEditor />} />
          <Route path="inventory/masters" element={<MastersPage />} />
          <Route path="inventory/labels" element={<LabelsPage />} />
          <Route path="inventory/price-lists" element={<PriceListsPage />} />
          <Route path="inventory/expiry" element={<ExpiryPage />} />
          <Route path="inventory/stock" element={<Stock.Moves />} />
          <Route path="inventory/adjustments/new" element={<Stock.Adjust />} />
          <Route path="inventory/transfers/new" element={<Stock.Transfer />} />
          <Route path="inventory/counts/:id" element={<Stock.Count />} />
          <Route path="inventory/documents/:kind/:id" element={<Stock.Doc />} />
          <Route path="inventory/stock-card/:variantId" element={<Stock.Card />} />

          <Route path="sales/invoices" element={<Sales.Invoices />} />
          <Route path="sales/invoices/:id" element={<Sales.Invoice />} />
          <Route path="sales/returns" element={<Sales.Returns />} />
          <Route path="sales/returns/new" element={<Sales.NewReturn />} />
          <Route path="sales/returns/:id" element={<Sales.Return />} />
          <Route path="sales/documents" element={<Sales.Docs />} />
          <Route path="sales/documents/new" element={<Sales.DocEditor />} />
          <Route path="sales/documents/:id" element={<Sales.Doc />} />
          <Route path="sales/documents/:id/edit" element={<Sales.DocEditor />} />

          <Route path="purchases" element={<Purchases.List />} />
          <Route
            path="purchases/orders/new"
            element={<Purchases.Editor key="order" mode="order" />}
          />
          <Route
            path="purchases/orders/:id"
            element={<Purchases.Doc key="purchase_order" kind="purchase_order" />}
          />
          <Route
            path="purchases/receipts/new"
            element={<Purchases.Editor key="receipt" mode="receipt" />}
          />
          <Route path="purchases/receipts/:id" element={<Purchases.Receipt />} />
          <Route path="purchases/bills/new" element={<Purchases.Editor key="bill" mode="bill" />} />
          <Route
            path="purchases/bills/:id"
            element={<Purchases.Doc key="purchase_bill" kind="purchase_bill" />}
          />
          <Route path="purchases/returns/new" element={<Purchases.NewReturn />} />
          <Route
            path="purchases/returns/:id"
            element={<Purchases.Doc key="purchase_return" kind="purchase_return" />}
          />

          <Route
            path="parties/customers"
            element={<Parties.List key="customer" partyType="customer" />}
          />
          <Route
            path="parties/suppliers"
            element={<Parties.List key="supplier" partyType="supplier" />}
          />
          <Route path="parties/:id" element={<Parties.Detail />} />
          <Route path="payments" element={<Parties.Payments />} />
          <Route path="reports" element={<Reports.Index />} />
          <Route path="reports/:name" element={<Reports.View />} />
          <Route path="accounting/expenses" element={<Accounting.Expenses />} />
          <Route path="accounting/accounts" element={<Accounting.Accounts />} />
          <Route path="accounting/day-close" element={<Accounting.DayClose />} />
          {KitchenSink && <Route path="dev/ui" element={<KitchenSink />} />}
          <Route path="settings" element={<SettingsPage />} />
          <Route path="settings/:section" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Route>
    </Routes>
  );
}

export default function App() {
  return (
    <>
      <Gate />
      <ApprovalDialog />
      <ConfirmDialog />
    </>
  );
}
