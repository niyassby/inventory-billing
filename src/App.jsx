import React from 'react'
import { Navigate, Outlet, Route, Routes } from 'react-router-dom'
import Layout from './app/layout'
import Auth from './app/auth'

// Pages
import Overview from './pages/dashboard/Overview'

import Products from './pages/inventory/Products'
import AddEditProduct from './pages/inventory/AddEditProduct'
import Categories from './pages/inventory/Categories'
import Adjustments from './pages/inventory/Adjustments'
import Barcodes from './pages/inventory/Barcodes'

import Invoices from './pages/sales/Invoices'
import Quotations from './pages/sales/Quotations'
import SalesReturns from './pages/sales/Returns'
import POS from './pages/sales/POS'

import Orders from './pages/purchases/Orders'
import Bills from './pages/purchases/Bills'
import PurchaseReturns from './pages/purchases/Returns'

import Customers from './pages/contacts/Customers'
import Leads from './pages/contacts/Leads'
import Suppliers from './pages/contacts/Suppliers'

import ChartOfAccounts from './pages/accounting/ChartOfAccounts'
import Journals from './pages/accounting/Journals'
import Expenses from './pages/accounting/Expenses'
import PaymentsReceived from './pages/accounting/PaymentsReceived'
import Taxes from './pages/accounting/Taxes'

import ProfitAndLoss from './pages/reports/ProfitAndLoss'
import SalesSummary from './pages/reports/SalesSummary'
import StockValuation from './pages/reports/StockValuation'
import TaxSummary from './pages/reports/TaxSummary'

import Company from './pages/settings/Company'
import Users from './pages/settings/Users'
import Preferences from './pages/settings/Preferences'
import Templates from './pages/settings/Templates'
import Maintenance from './pages/settings/Maintenance'
import InventoryLayout from './pages/inventory/InventoryLayout'
import SalesLayout from './pages/sales/SalesLayout'

function App() {
  return (
    <Routes>
      {/* Auth Route */}
      <Route path="/auth" element={<Auth />} />

      {/* POS is outside the sidebar layout to utilize full-screen grid for cashier workspace */}
      <Route path="/sales/pos" element={<POS />} />

      {/* Main Layout App Routes */}
      <Route path="/" element={<Layout/>}>
        <Route index element={<Navigate to="/dashboard" replace />} />
        <Route path="dashboard" element={<Overview />} />
        
        {/* Inventory */}
        <Route path="inventory" element={<Outlet />} >
          <Route index element={<Products />} />
          <Route path="products" element={<Products />} />
          <Route path="products/new" element={<AddEditProduct />} />
          <Route path="products/edit/:id" element={<AddEditProduct />} />
          <Route path="categories" element={<Categories />} />
          <Route path="adjustments" element={<Adjustments />} />
          <Route path="barcodes" element={<Barcodes />} />
        </Route>

        {/* Sales */}
        <Route path="sales" element={<SalesLayout />} >
          <Route index element={<Invoices />} />
          <Route path="invoices" element={<Invoices />} />
          <Route path="quotations" element={<Quotations />} />
          <Route path="returns" element={<SalesReturns />} />
        </Route>

        {/* Purchases */}
        <Route path="purchases" element={<Navigate to="/purchases/orders" replace />} />
        <Route path="purchases/orders" element={<Orders />} />
        <Route path="purchases/bills" element={<Bills />} />
        <Route path="purchases/returns" element={<PurchaseReturns />} />

        {/* Contacts */}
        <Route path="contacts" element={<Navigate to="/contacts/customers" replace />} />
        <Route path="contacts/customers" element={<Customers />} />
        <Route path="contacts/leads" element={<Leads />} />
        <Route path="contacts/suppliers" element={<Suppliers />} />

        {/* Accounting */}
        <Route path="accounting" element={<Navigate to="/accounting/chart-of-accounts" replace />} />
        <Route path="accounting/chart-of-accounts" element={<ChartOfAccounts />} />
        <Route path="accounting/journals" element={<Journals />} />
        <Route path="accounting/expenses" element={<Expenses />} />
        <Route path="accounting/payments-received" element={<PaymentsReceived />} />
        <Route path="accounting/taxes" element={<Taxes />} />

        {/* Reports */}
        <Route path="reports" element={<Navigate to="/reports/profit-and-loss" replace />} />
        <Route path="reports/profit-and-loss" element={<ProfitAndLoss />} />
        <Route path="reports/sales" element={<SalesSummary />} />
        <Route path="reports/stock-valuation" element={<StockValuation />} />
        <Route path="reports/tax" element={<TaxSummary />} />

        {/* Settings */}
        <Route path="settings" element={<Navigate to="/settings/company" replace />} />
        <Route path="settings/company" element={<Company />} />
        <Route path="settings/users" element={<Users />} />
        <Route path="settings/preferences" element={<Preferences />} />
        <Route path="settings/templates" element={<Templates />} />
        <Route path="settings/maintenance" element={<Maintenance />} />
      </Route>

      {/* Fallback */}
      <Route path="*" element={<Navigate to="/dashboard" replace />} />
    </Routes>
  )
}

export default App;