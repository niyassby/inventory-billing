/**
 * Hand-written mirror of the current (v1) Rust command contract in `src-tauri/src/models` and
 * `src-tauri/src/commands`. It exists so TypeScript code can call today's backend safely while the
 * backend is rebuilt. Phase 3 replaces this file with types generated from the Rust DTOs
 * (tauri-specta), and the legacy commands are retired module by module.
 *
 * Conventions of the legacy backend: money and quantities are floats (major units), field names are
 * snake_case, and command arguments are camelCase (Tauri converts them).
 */

export interface User {
  id: string;
  name: string;
  email: string;
  /** "admin" | "manager" | "accountant" | "staff" (free string in v1). */
  role: string;
  created_at: string;
  updated_at: string;
}

export interface AuthResponse {
  user: User;
  token: string;
}

export interface Category {
  id: string;
  name: string;
  created_at: string;
}

export type Brand = Category;

export interface Unit {
  id: string;
  name: string;
  short_name: string;
  created_at: string;
}

export interface Product {
  id: string;
  name: string;
  sku: string;
  barcode: string | null;
  qr_code: string | null;
  category_id: string | null;
  category_name: string | null;
  brand_id: string | null;
  brand_name: string | null;
  cost: number;
  price: number;
  stock: number;
  reorder_level: number;
  is_expiry_tracked: number;
  expiry_date: string | null;
  created_at: string;
  updated_at: string;
  unit: string;
  description: string | null;
  image_path: string | null;
  tax_id: string | null;
  tax_name: string | null;
  tax_rate: number | null;
  unit_id: string | null;
  unit_name: string | null;
  offer_price: number | null;
}

export interface Paginated {
  total_count: number;
  page: number;
  limit: number;
}

export interface StockAdjustment {
  id: string;
  product_id: string;
  product_name: string | null;
  adjustment_type: "ADD" | "SUBTRACT" | "SET";
  quantity: number;
  reason: string | null;
  adjusted_by: string;
  adjusted_by_name: string | null;
  created_at: string;
}

export interface Customer {
  id: string;
  name: string;
  email: string | null;
  phone: string | null;
  address: string | null;
  due_amount: number;
  created_at: string;
  updated_at: string;
}

export interface Supplier {
  id: string;
  name: string;
  email: string | null;
  phone: string | null;
  address: string | null;
  balance: number;
  created_at: string;
  updated_at: string;
}

export interface Lead {
  id: string;
  name: string;
  contact_name: string | null;
  phone: string | null;
  email: string | null;
  stage: string;
  deal_value: number;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

export interface DocumentItem {
  id: string;
  product_id: string;
  product_name: string | null;
  quantity: number;
  unit_price: number;
  total: number;
}

export interface QuotationItem extends DocumentItem {
  quotation_id: string;
}

export interface Quotation {
  id: string;
  quotation_number: string;
  customer_id: string;
  customer_name: string | null;
  total: number;
  status: string;
  date: string;
  created_at: string;
  items: QuotationItem[];
}

export interface InvoiceItem extends DocumentItem {
  invoice_id: string;
  tax_rate: number | null;
}

export interface Invoice {
  id: string;
  invoice_number: string;
  customer_id: string;
  customer_name: string | null;
  subtotal: number;
  discount: number;
  tax: number;
  total: number;
  amount_paid: number;
  due_amount: number;
  payment_status: string;
  payment_method: string | null;
  date: string;
  created_at: string;
  items: InvoiceItem[];
}

export interface PurchaseItem extends DocumentItem {
  purchase_id: string;
}

export interface Purchase {
  id: string;
  purchase_number: string;
  supplier_id: string;
  supplier_name: string | null;
  total: number;
  status: string;
  date: string;
  created_at: string;
  items: PurchaseItem[];
}

export interface Payment {
  id: string;
  payment_type: "RECEIVE" | "PAY";
  entity_id: string;
  entity_name: string | null;
  amount: number;
  payment_method: string;
  reference_no: string | null;
  date: string;
  created_at: string;
}

export interface Expense {
  id: string;
  title: string;
  category: string;
  amount: number;
  date: string;
  description: string | null;
  created_at: string;
}

export interface TaxRate {
  id: string;
  name: string;
  rate: number;
  is_active: number;
  created_at: string;
}

export interface ChartOfAccount {
  id: string;
  code: string;
  name: string;
  account_type: string;
  parent_id: string | null;
  is_active: number;
}

export interface JournalLine {
  id: string;
  journal_entry_id: string;
  account_id: string;
  account_name: string | null;
  account_code: string | null;
  debit: number;
  credit: number;
}

export interface JournalEntry {
  id: string;
  entry_date: string;
  description: string;
  reference: string | null;
  created_at: string;
  lines: JournalLine[];
}

export interface AuditLog {
  id: string;
  user_id: string | null;
  user_name: string | null;
  action: string;
  table_name: string;
  record_id: string | null;
  old_values: string | null;
  new_values: string | null;
  timestamp: string;
}

export interface DashboardStats {
  today_sales: number;
  monthly_sales: number;
  revenue: number;
  profit: number;
  expenses: number;
  stock_value: number;
  low_stock_count: number;
}

export interface ChartPoint {
  label: string;
  value: number;
}

export interface HeldOrder {
  id: string;
  customer_name: string;
  discount: number;
  tax_rate: number;
  /** JSON-encoded cart. */
  items: string;
  created_at: string;
}

export interface ImportProductInput {
  name: string;
  sku: string;
  barcode: string | null;
  category_name: string | null;
  brand_name: string | null;
  cost: number | null;
  price: number | null;
  stock: number | null;
  reorder_level: number | null;
  unit: string | null;
}

export interface ProductTransaction {
  date: string;
  tx_type: "SALE" | "PURCHASE" | "ADJUSTMENT";
  reference: string;
  quantity: number;
  price: number;
  performed_by: string | null;
}

interface Token {
  token: string;
}
interface ById extends Token {
  id: string;
}
interface ListQuery extends Token {
  search?: string | null;
  sortBy?: string | null;
  sortOrder?: "ASC" | "DESC" | null;
  page: number;
  limit: number;
}

interface ProductFields {
  name: string;
  sku: string;
  barcode: string | null;
  qrCode: string | null;
  categoryId: string | null;
  brandId: string | null;
  cost: number;
  price: number;
  reorderLevel: number;
  isExpiryTracked: number;
  expiryDate: string | null;
  unit: string;
  description: string | null;
  imagePath: string | null;
  taxId: string | null;
  unitId: string | null;
  offerPrice: number | null;
}

interface ContactFields {
  name: string;
  email: string | null;
  phone: string | null;
  address: string | null;
}

interface LeadFields {
  name: string;
  contactName: string | null;
  phone: string | null;
  email: string | null;
  stage: string;
  dealValue: number;
  notes: string | null;
}

interface InvoiceFields {
  customerId: string;
  subtotal: number;
  discount: number;
  tax: number;
  total: number;
  amountPaid: number;
  paymentMethod: string | null;
  items: Omit<InvoiceItem, "tax_rate">[];
}

type Cmd<Args, Result> = { args: Args; result: Result };

/** Every command registered in `src-tauri/src/lib.rs`, with its arguments and result. */
export interface LegacyCommands {
  login: Cmd<{ email: string; password: string }, AuthResponse>;
  logout: Cmd<Token, null>;
  get_profile: Cmd<Token, User>;
  get_audit_logs_cmd: Cmd<Token, AuditLog[]>;

  list_users_cmd: Cmd<Token, User[]>;
  create_user_cmd: Cmd<
    Token & { name: string; email: string; passwordHash: string; role: string },
    User
  >;
  update_user_cmd: Cmd<ById & { name: string; email: string; role: string }, User>;
  delete_user_cmd: Cmd<ById, null>;

  create_category_cmd: Cmd<Token & { name: string }, Category>;
  list_categories_cmd: Cmd<Token, Category[]>;
  delete_category_cmd: Cmd<ById, null>;
  create_brand_cmd: Cmd<Token & { name: string }, Brand>;
  list_brands_cmd: Cmd<Token, Brand[]>;
  delete_brand_cmd: Cmd<ById, null>;
  create_unit_cmd: Cmd<Token & { name: string; shortName: string }, Unit>;
  list_units_cmd: Cmd<Token, Unit[]>;
  delete_unit_cmd: Cmd<ById, null>;

  create_product_cmd: Cmd<Token & ProductFields & { stock: number }, Product>;
  update_product_cmd: Cmd<ById & ProductFields, Product>;
  delete_product_cmd: Cmd<ById, null>;
  list_products_cmd: Cmd<Token, Product[]>;
  get_product_by_id_cmd: Cmd<ById, Product>;
  list_products_filtered_cmd: Cmd<
    ListQuery & { categoryId?: string | null; brandId?: string | null },
    Paginated & { products: Product[] }
  >;
  adjust_stock_cmd: Cmd<
    Token & {
      productId: string;
      adjustmentType: StockAdjustment["adjustment_type"];
      quantity: number;
      reason: string | null;
    },
    Product
  >;
  list_stock_adjustments_cmd: Cmd<Token, StockAdjustment[]>;
  bulk_import_products_cmd: Cmd<Token & { products: ImportProductInput[] }, null>;
  get_product_transactions_cmd: Cmd<Token & { productId: string }, ProductTransaction[]>;

  create_customer_cmd: Cmd<Token & ContactFields, Customer>;
  update_customer_cmd: Cmd<ById & ContactFields & { dueAmount: number }, Customer>;
  list_customers_cmd: Cmd<Token, Customer[]>;
  list_customers_filtered_cmd: Cmd<ListQuery, Paginated & { customers: Customer[] }>;
  delete_customer_cmd: Cmd<ById, null>;
  create_supplier_cmd: Cmd<Token & ContactFields, Supplier>;
  update_supplier_cmd: Cmd<ById & ContactFields & { balance: number }, Supplier>;
  list_suppliers_cmd: Cmd<Token, Supplier[]>;
  list_suppliers_filtered_cmd: Cmd<ListQuery, Paginated & { suppliers: Supplier[] }>;
  delete_supplier_cmd: Cmd<ById, null>;
  create_lead_cmd: Cmd<Token & LeadFields, Lead>;
  update_lead_cmd: Cmd<ById & LeadFields, Lead>;
  list_leads_cmd: Cmd<Token, Lead[]>;
  list_leads_filtered_cmd: Cmd<
    ListQuery & { stage?: string | null },
    Paginated & { leads: Lead[] }
  >;
  delete_lead_cmd: Cmd<ById, null>;

  create_invoice_cmd: Cmd<Token & InvoiceFields, Invoice>;
  update_invoice_cmd: Cmd<ById & InvoiceFields, Invoice>;
  get_invoice_cmd: Cmd<ById, Invoice>;
  list_invoices_cmd: Cmd<Token, Invoice[]>;
  list_invoices_filtered_cmd: Cmd<
    ListQuery & { paymentStatus?: string | null },
    Paginated & { invoices: Invoice[] }
  >;
  update_invoice_status_cmd: Cmd<ById & { status: string }, null>;
  hold_order_cmd: Cmd<
    ById & { customerName: string; discount: number; taxRate: number; items: string },
    null
  >;
  list_held_orders_cmd: Cmd<Token, HeldOrder[]>;
  delete_held_order_cmd: Cmd<ById, null>;

  create_quotation_cmd: Cmd<
    Token & { customerId: string; total: number; status: string; items: QuotationItem[] },
    Quotation
  >;
  get_quotation_cmd: Cmd<ById, Quotation>;
  list_quotations_cmd: Cmd<Token, Quotation[]>;
  list_quotations_filtered_cmd: Cmd<
    ListQuery & { status?: string | null },
    Paginated & { quotations: Quotation[] }
  >;

  create_purchase_cmd: Cmd<
    Token & { supplierId: string; total: number; status: string; items: PurchaseItem[] },
    Purchase
  >;
  get_purchase_cmd: Cmd<ById, Purchase>;
  list_purchases_cmd: Cmd<Token, Purchase[]>;
  list_purchases_filtered_cmd: Cmd<
    ListQuery & { status?: string | null },
    Paginated & { purchases: Purchase[] }
  >;
  receive_purchase_cmd: Cmd<ById, Purchase>;

  create_payment_cmd: Cmd<
    Token & {
      paymentType: Payment["payment_type"];
      entityId: string;
      amount: number;
      paymentMethod: string;
      referenceNo: string | null;
    },
    Payment
  >;
  list_payments_cmd: Cmd<Token, Payment[]>;
  list_payments_filtered_cmd: Cmd<
    ListQuery & { paymentType?: string | null },
    Paginated & { payments: Payment[] }
  >;
  create_expense_cmd: Cmd<
    Token & {
      title: string;
      category: string;
      amount: number;
      date: string;
      description: string | null;
    },
    Expense
  >;
  list_expenses_cmd: Cmd<Token, Expense[]>;
  list_expenses_filtered_cmd: Cmd<
    ListQuery & { category?: string | null },
    Paginated & { expenses: Expense[] }
  >;
  delete_expense_cmd: Cmd<ById, null>;
  list_chart_of_accounts_cmd: Cmd<Token, ChartOfAccount[]>;
  create_chart_of_account_cmd: Cmd<
    Token & { code: string; name: string; accountType: string; parentId: string | null },
    ChartOfAccount
  >;
  create_journal_entry_cmd: Cmd<
    Token & {
      entryDate: string;
      description: string;
      reference: string | null;
      lines: JournalLine[];
    },
    JournalEntry
  >;
  list_journal_entries_cmd: Cmd<Token, JournalEntry[]>;

  get_dashboard_stats_cmd: Cmd<Token, DashboardStats>;
  get_sales_chart_points_cmd: Cmd<Token, ChartPoint[]>;
  get_tax_summary_cmd: Cmd<Token, ChartPoint[]>;
  list_tax_rates_cmd: Cmd<Token, TaxRate[]>;
  create_tax_rate_cmd: Cmd<Token & { name: string; rate: number }, TaxRate>;

  backup_database_cmd: Cmd<Token & { destPath: string }, null>;
  restore_database_cmd: Cmd<Token & { srcPath: string }, null>;
}

export type LegacyCommandName = keyof LegacyCommands;
export type CommandArgs<K extends LegacyCommandName> = LegacyCommands[K]["args"];
export type CommandResult<K extends LegacyCommandName> = LegacyCommands[K]["result"];
