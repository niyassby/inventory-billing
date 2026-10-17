use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user: User,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brand {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub qr_code: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub brand_id: Option<String>,
    pub brand_name: Option<String>,
    pub cost: f64,
    pub price: f64,
    pub stock: f64,
    pub reorder_level: f64,
    pub is_expiry_tracked: i32,
    pub expiry_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub unit: String,
    pub description: Option<String>,
    pub image_path: Option<String>,
    pub tax_id: Option<String>,
    pub tax_name: Option<String>,
    pub tax_rate: Option<f64>,
    pub unit_id: Option<String>,
    pub unit_name: Option<String>,
    pub offer_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedProducts {
    pub products: Vec<Product>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockAdjustment {
    pub id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub adjustment_type: String, // 'ADD', 'SUBTRACT', 'SET'
    pub quantity: f64,
    pub reason: Option<String>,
    pub adjusted_by: String,
    pub adjusted_by_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub due_amount: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supplier {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub balance: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lead {
    pub id: String,
    pub name: String,
    pub contact_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub stage: String, // 'New', 'Contacted', 'Proposal', 'Negotiation', 'Won', 'Lost'
    pub deal_value: f64,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotationItem {
    pub id: String,
    pub quotation_id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub quantity: f64,
    pub unit_price: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quotation {
    pub id: String,
    pub quotation_number: String,
    pub customer_id: String,
    pub customer_name: Option<String>,
    pub total: f64,
    pub status: String,
    pub date: String,
    pub created_at: String,
    pub items: Vec<QuotationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceItem {
    pub id: String,
    pub invoice_id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub quantity: f64,
    pub unit_price: f64,
    pub total: f64,
    pub tax_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub invoice_number: String,
    pub customer_id: String,
    pub customer_name: Option<String>,
    pub subtotal: f64,
    pub discount: f64,
    pub tax: f64,
    pub total: f64,
    pub amount_paid: f64,
    pub due_amount: f64,
    pub payment_status: String,
    pub payment_method: Option<String>,
    pub date: String,
    pub created_at: String,
    pub items: Vec<InvoiceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseItem {
    pub id: String,
    pub purchase_id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub quantity: f64,
    pub unit_price: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Purchase {
    pub id: String,
    pub purchase_number: String,
    pub supplier_id: String,
    pub supplier_name: Option<String>,
    pub total: f64,
    pub status: String,
    pub date: String,
    pub created_at: String,
    pub items: Vec<PurchaseItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub id: String,
    pub payment_type: String, // 'RECEIVE', 'PAY'
    pub entity_id: String,
    pub entity_name: Option<String>,
    pub amount: f64,
    pub payment_method: String,
    pub reference_no: Option<String>,
    pub date: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    pub id: String,
    pub title: String,
    pub category: String,
    pub amount: f64,
    pub date: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRate {
    pub id: String,
    pub name: String,
    pub rate: f64,
    pub is_active: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartOfAccount {
    pub id: String,
    pub code: String,
    pub name: String,
    pub account_type: String, // 'Asset', 'Liability', 'Equity', 'Revenue', 'Expense'
    pub parent_id: Option<String>,
    pub is_active: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLine {
    pub id: String,
    pub journal_entry_id: String,
    pub account_id: String,
    pub account_name: Option<String>,
    pub account_code: Option<String>,
    pub debit: f64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub entry_date: String,
    pub description: String,
    pub reference: Option<String>,
    pub created_at: String,
    pub lines: Vec<JournalLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub action: String,
    pub table_name: String,
    pub record_id: Option<String>,
    pub old_values: Option<String>,
    pub new_values: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub today_sales: f64,
    pub monthly_sales: f64,
    pub revenue: f64,
    pub profit: f64,
    pub expenses: f64,
    pub stock_value: f64,
    pub low_stock_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartPoint {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldOrder {
    pub id: String,
    pub customer_name: String,
    pub discount: f64,
    pub tax_rate: f64,
    pub items: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProductInput {
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub category_name: Option<String>,
    pub brand_name: Option<String>,
    pub cost: Option<f64>,
    pub price: Option<f64>,
    pub stock: Option<f64>,
    pub reorder_level: Option<f64>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedCustomers {
    pub customers: Vec<Customer>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedSuppliers {
    pub suppliers: Vec<Supplier>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedLeads {
    pub leads: Vec<Lead>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedInvoices {
    pub invoices: Vec<Invoice>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedQuotations {
    pub quotations: Vec<Quotation>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedPurchases {
    pub purchases: Vec<Purchase>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedExpenses {
    pub expenses: Vec<Expense>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedPayments {
    pub payments: Vec<Payment>,
    pub total_count: i64,
    pub page: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductTransaction {
    pub date: String,
    pub tx_type: String, // 'SALE', 'PURCHASE', 'ADJUSTMENT'
    pub reference: String,
    pub quantity: f64,
    pub price: f64,
    pub performed_by: Option<String>,
}
