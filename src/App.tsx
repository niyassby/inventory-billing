import type { ReactNode } from "react";
import { Suspense, lazy } from "react";
import { Navigate, Outlet, Route, Routes } from "react-router-dom";
import Auth from "./app/auth";
import Layout from "./app/layout";
import { FullScreenSpinner, RequireAuth } from "./app/require-auth";

// Route-level code splitting: each page loads on first visit instead of in one 1.7 MB bundle.
// lazy() must run once at module level; creating it during render suspends forever.
const Overview = lazy(() => import("./pages/dashboard/Overview"));
const Products = lazy(() => import("./pages/inventory/Products"));
const AddEditProduct = lazy(() => import("./pages/inventory/AddEditProduct"));
const Categories = lazy(() => import("./pages/inventory/Categories"));
const Adjustments = lazy(() => import("./pages/inventory/Adjustments"));
const Barcodes = lazy(() => import("./pages/inventory/Barcodes"));
const SalesLayout = lazy(() => import("./pages/sales/SalesLayout"));
const Invoices = lazy(() => import("./pages/sales/Invoices"));
const Quotations = lazy(() => import("./pages/sales/Quotations"));
const SalesReturns = lazy(() => import("./pages/sales/Returns"));
const POS = lazy(() => import("./pages/sales/POS"));
const Orders = lazy(() => import("./pages/purchases/Orders"));
const Bills = lazy(() => import("./pages/purchases/Bills"));
const PurchaseReturns = lazy(() => import("./pages/purchases/Returns"));
const Customers = lazy(() => import("./pages/contacts/Customers"));
const Leads = lazy(() => import("./pages/contacts/Leads"));
const Suppliers = lazy(() => import("./pages/contacts/Suppliers"));
const ChartOfAccounts = lazy(() => import("./pages/accounting/ChartOfAccounts"));
const Journals = lazy(() => import("./pages/accounting/Journals"));
const Expenses = lazy(() => import("./pages/accounting/Expenses"));
const PaymentsReceived = lazy(() => import("./pages/accounting/PaymentsReceived"));
const Taxes = lazy(() => import("./pages/accounting/Taxes"));
const ProfitAndLoss = lazy(() => import("./pages/reports/ProfitAndLoss"));
const SalesSummary = lazy(() => import("./pages/reports/SalesSummary"));
const StockValuation = lazy(() => import("./pages/reports/StockValuation"));
const TaxSummary = lazy(() => import("./pages/reports/TaxSummary"));
const Company = lazy(() => import("./pages/settings/Company"));
const Users = lazy(() => import("./pages/settings/Users"));
const Preferences = lazy(() => import("./pages/settings/Preferences"));
const Templates = lazy(() => import("./pages/settings/Templates"));
const Maintenance = lazy(() => import("./pages/settings/Maintenance"));

function PageSuspense({ children }: { children: ReactNode }) {
  return (
    <Suspense
      fallback={
        <div
          className="flex h-[60vh] items-center justify-center"
          role="status"
          aria-label="Loading"
        >
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
        </div>
      }
    >
      {children}
    </Suspense>
  );
}

export default function App() {
  return (
    <Routes>
      <Route path="/auth" element={<Auth />} />

      {/* POS is full-screen (outside the sidebar layout) but still requires a session. */}
      <Route
        path="/sales/pos"
        element={
          <RequireAuth>
            <Suspense fallback={<FullScreenSpinner label="Opening POS…" />}>
              <POS />
            </Suspense>
          </RequireAuth>
        }
      />

      <Route path="/" element={<Layout />}>
        <Route
          element={
            <PageSuspense>
              <Outlet />
            </PageSuspense>
          }
        >
          <Route index element={<Navigate to="/dashboard" replace />} />
          <Route path="dashboard" element={<Overview />} />
          <Route path="dashboard/reports" element={<Navigate to="/reports" replace />} />

          <Route path="inventory" element={<Outlet />}>
            <Route index element={<Products />} />
            <Route path="products" element={<Products />} />
            <Route path="products/new" element={<AddEditProduct />} />
            <Route path="products/edit/:id" element={<AddEditProduct />} />
            <Route path="categories" element={<Categories />} />
            <Route path="adjustments" element={<Adjustments />} />
            <Route path="barcodes" element={<Barcodes />} />
          </Route>

          <Route path="sales" element={<SalesLayout />}>
            <Route index element={<Invoices />} />
            <Route path="invoices" element={<Invoices />} />
            <Route path="quotations" element={<Quotations />} />
            <Route path="returns" element={<SalesReturns />} />
          </Route>

          <Route path="purchases" element={<Navigate to="/purchases/orders" replace />} />
          <Route path="purchases/orders" element={<Orders />} />
          <Route path="purchases/bills" element={<Bills />} />
          <Route path="purchases/returns" element={<PurchaseReturns />} />

          <Route path="contacts" element={<Navigate to="/contacts/customers" replace />} />
          <Route path="contacts/customers" element={<Customers />} />
          <Route path="contacts/leads" element={<Leads />} />
          <Route path="contacts/suppliers" element={<Suppliers />} />

          <Route
            path="accounting"
            element={<Navigate to="/accounting/chart-of-accounts" replace />}
          />
          <Route path="accounting/chart-of-accounts" element={<ChartOfAccounts />} />
          <Route path="accounting/journals" element={<Journals />} />
          <Route path="accounting/expenses" element={<Expenses />} />
          <Route path="accounting/payments-received" element={<PaymentsReceived />} />
          <Route path="accounting/taxes" element={<Taxes />} />

          <Route path="reports" element={<Navigate to="/reports/profit-and-loss" replace />} />
          <Route path="reports/profit-and-loss" element={<ProfitAndLoss />} />
          <Route path="reports/sales" element={<SalesSummary />} />
          <Route path="reports/stock-valuation" element={<StockValuation />} />
          <Route path="reports/tax" element={<TaxSummary />} />
          {/* The report pages' own tab buttons use these older paths (audit §8). */}
          <Route path="reports/sales-summary" element={<Navigate to="/reports/sales" replace />} />
          <Route path="reports/tax-summary" element={<Navigate to="/reports/tax" replace />} />

          <Route path="settings" element={<Navigate to="/settings/company" replace />} />
          <Route path="settings/company" element={<Company />} />
          <Route path="settings/users" element={<Users />} />
          <Route path="settings/preferences" element={<Preferences />} />
          <Route path="settings/templates" element={<Templates />} />
          <Route path="settings/maintenance" element={<Maintenance />} />
        </Route>
      </Route>

      <Route path="*" element={<Navigate to="/dashboard" replace />} />
    </Routes>
  );
}
