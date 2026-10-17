-- 0006 system seed data: permissions, built-in roles, system accounts and expense categories.
-- Generated from packages/shared/src/constants.ts; tests check the two stay in sync.
-- Fixed ids so every installation (and the legacy migration) refers to the same rows.

INSERT INTO permissions (code, description) VALUES
  ('dashboard.view', 'View (dashboard)'),
  ('products.view', 'View (products)'),
  ('products.manage', 'Manage (products)'),
  ('products.price_change', 'Price change (products)'),
  ('stock.view', 'View (stock)'),
  ('stock.adjust', 'Adjust (stock)'),
  ('stock.transfer', 'Transfer (stock)'),
  ('stock.count', 'Count (stock)'),
  ('sales.create', 'Create (sales)'),
  ('sales.view', 'View (sales)'),
  ('sales.discount', 'Discount (sales)'),
  ('sales.discount_override', 'Discount override (sales)'),
  ('sales.price_override', 'Price override (sales)'),
  ('sales.void', 'Void (sales)'),
  ('sales.return', 'Return (sales)'),
  ('sales.backdate', 'Backdate (sales)'),
  ('quotations.manage', 'Manage (quotations)'),
  ('purchases.view', 'View (purchases)'),
  ('purchases.manage', 'Manage (purchases)'),
  ('purchases.receive', 'Receive (purchases)'),
  ('parties.view', 'View (parties)'),
  ('parties.manage', 'Manage (parties)'),
  ('payments.receive', 'Receive (payments)'),
  ('payments.pay', 'Pay (payments)'),
  ('expenses.manage', 'Manage (expenses)'),
  ('accounting.view', 'View (accounting)'),
  ('accounting.manage', 'Manage (accounting)'),
  ('day_close.perform', 'Perform (day close)'),
  ('reports.view', 'View (reports)'),
  ('reports.export', 'Export (reports)'),
  ('settings.view', 'View (settings)'),
  ('settings.manage', 'Manage (settings)'),
  ('users.manage', 'Manage (users)'),
  ('backup.manage', 'Manage (backup)'),
  ('audit.view', 'View (audit)'),
  ('approvals.grant', 'Grant (approvals)');

INSERT INTO roles (id, code, name, is_system, max_discount_rate, created_at, updated_at) VALUES
  ('01920000-0000-7000-8000-000000000001', 'owner', 'Owner', 1, 1000000, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8000-000000000002', 'manager', 'Manager', 1, 200000, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8000-000000000003', 'cashier', 'Cashier', 1, 50000, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8000-000000000004', 'stock_keeper', 'Stock Keeper', 1, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8000-000000000005', 'accountant', 'Accountant', 1, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

-- Owner: everything. Manager: everything except user and backup management.
INSERT INTO role_permissions (role_id, permission_code) SELECT '01920000-0000-7000-8000-000000000001', code FROM permissions;
INSERT INTO role_permissions (role_id, permission_code) SELECT '01920000-0000-7000-8000-000000000002', code FROM permissions
  WHERE code NOT IN ('users.manage', 'backup.manage');
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000003', 'dashboard.view'),
  ('01920000-0000-7000-8000-000000000003', 'products.view'),
  ('01920000-0000-7000-8000-000000000003', 'stock.view'),
  ('01920000-0000-7000-8000-000000000003', 'sales.create'),
  ('01920000-0000-7000-8000-000000000003', 'sales.view'),
  ('01920000-0000-7000-8000-000000000003', 'sales.discount'),
  ('01920000-0000-7000-8000-000000000003', 'quotations.manage'),
  ('01920000-0000-7000-8000-000000000003', 'parties.view'),
  ('01920000-0000-7000-8000-000000000003', 'parties.manage'),
  ('01920000-0000-7000-8000-000000000003', 'payments.receive');
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000004', 'dashboard.view'),
  ('01920000-0000-7000-8000-000000000004', 'products.view'),
  ('01920000-0000-7000-8000-000000000004', 'products.manage'),
  ('01920000-0000-7000-8000-000000000004', 'stock.view'),
  ('01920000-0000-7000-8000-000000000004', 'stock.adjust'),
  ('01920000-0000-7000-8000-000000000004', 'stock.transfer'),
  ('01920000-0000-7000-8000-000000000004', 'stock.count'),
  ('01920000-0000-7000-8000-000000000004', 'purchases.view'),
  ('01920000-0000-7000-8000-000000000004', 'purchases.receive'),
  ('01920000-0000-7000-8000-000000000004', 'parties.view');
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000005', 'dashboard.view'),
  ('01920000-0000-7000-8000-000000000005', 'products.view'),
  ('01920000-0000-7000-8000-000000000005', 'stock.view'),
  ('01920000-0000-7000-8000-000000000005', 'sales.view'),
  ('01920000-0000-7000-8000-000000000005', 'purchases.view'),
  ('01920000-0000-7000-8000-000000000005', 'parties.view'),
  ('01920000-0000-7000-8000-000000000005', 'payments.receive'),
  ('01920000-0000-7000-8000-000000000005', 'payments.pay'),
  ('01920000-0000-7000-8000-000000000005', 'expenses.manage'),
  ('01920000-0000-7000-8000-000000000005', 'accounting.view'),
  ('01920000-0000-7000-8000-000000000005', 'accounting.manage'),
  ('01920000-0000-7000-8000-000000000005', 'day_close.perform'),
  ('01920000-0000-7000-8000-000000000005', 'reports.view'),
  ('01920000-0000-7000-8000-000000000005', 'reports.export'),
  ('01920000-0000-7000-8000-000000000005', 'audit.view');

INSERT INTO accounts (id, code, name, account_type, subtype, is_system, created_at, updated_at) VALUES
  ('01920000-0000-7000-8001-000000001000', '1000', 'Cash in Hand', 'asset', 'cash', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000001010', '1010', 'Bank Account', 'asset', 'bank', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000001200', '1200', 'Accounts Receivable', 'asset', 'receivable', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000001400', '1400', 'Inventory', 'asset', 'inventory', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000001500', '1500', 'Input Tax Credit (GST/VAT)', 'asset', 'tax_input', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000002000', '2000', 'Accounts Payable', 'liability', 'payable', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000002100', '2100', 'Output Tax Payable (GST/VAT)', 'liability', 'tax_output', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000003000', '3000', 'Owner''s Equity', 'equity', 'equity', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000003900', '3900', 'Opening Balance Adjustments', 'equity', 'equity', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000004000', '4000', 'Sales Revenue', 'income', 'sales', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000004100', '4100', 'Sales Returns', 'income', 'sales_returns', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000004900', '4900', 'Rounding Off', 'income', 'round_off', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000005000', '5000', 'Cost of Goods Sold', 'expense', 'cogs', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000005100', '5100', 'Rent Expense', 'expense', 'expense', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000005200', '5200', 'Utility Expense', 'expense', 'expense', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000005300', '5300', 'Discounts Allowed', 'expense', 'discount', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000005400', '5400', 'Stock Adjustments and Shrinkage', 'expense', 'stock_adjustment', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8001-000000005900', '5900', 'General Expenses', 'expense', 'expense', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

INSERT INTO expense_categories (id, name, account_id, created_at, updated_at) VALUES
  ('01920000-0000-7000-8002-000000000001', 'Rent', '01920000-0000-7000-8001-000000005100', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8002-000000000002', 'Utilities', '01920000-0000-7000-8001-000000005200', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('01920000-0000-7000-8002-000000000003', 'General', '01920000-0000-7000-8001-000000005900', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
