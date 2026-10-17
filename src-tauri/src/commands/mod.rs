use crate::auth;
use crate::database::DbState;
use crate::models::*;
use crate::repositories as repo;
use rusqlite::Connection;
use tauri::State;

// Helper function to check role permissions
fn authorize(conn: &Connection, token: &str, allowed_roles: &[&str]) -> Result<User, String> {
    let token_hash = auth::hash_token(token);
    let user = repo::validate_session(conn, &token_hash)
        .map_err(|_| "UNAUTHORIZED: Invalid or expired session token".to_string())?;

    if !allowed_roles.contains(&user.role.as_str()) {
        return Err(format!(
            "FORBIDDEN: Role '{}' does not have permission for this action",
            user.role
        ));
    }

    Ok(user)
}

// ==========================================
// AUTH COMMANDS
// ==========================================
#[tauri::command]
pub fn login(
    state: State<'_, DbState>,
    email: String,
    password: String,
) -> Result<AuthResponse, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    let (user_id, hash) = repo::get_user_password_hash(&conn, &email)
        .map_err(|_| "Login failed: Invalid credentials".to_string())?;

    if !auth::verify_password(&hash, &password) {
        return Err("Login failed: Invalid credentials".to_string());
    }

    let user = repo::get_user_by_id(&conn, &user_id).map_err(|e| e.to_string())?;

    let token = auth::generate_session_token();
    let token_hash = auth::hash_token(&token);

    repo::create_session(&conn, &user.id, &token_hash).map_err(|e| e.to_string())?;

    repo::log_audit(
        &conn,
        Some(&user.id),
        "LOGIN",
        "users",
        Some(&user.id),
        None,
        None,
    )
    .map_err(|e| e.to_string())?;

    Ok(AuthResponse { user, token })
}

#[tauri::command]
pub fn logout(state: State<'_, DbState>, token: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let token_hash = auth::hash_token(&token);
    if let Ok(user) = repo::validate_session(&conn, &token_hash) {
        repo::log_audit(
            &conn,
            Some(&user.id),
            "LOGOUT",
            "users",
            Some(&user.id),
            None,
            None,
        )
        .ok();
    }
    repo::delete_session(&conn, &token_hash).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_profile(state: State<'_, DbState>, token: String) -> Result<User, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let token_hash = auth::hash_token(&token);
    repo::validate_session(&conn, &token_hash).map_err(|_| "Session invalid".to_string())
}

#[tauri::command]
pub fn get_audit_logs_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<AuditLog>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager"])?;
    repo::get_audit_logs(&conn).map_err(|e| e.to_string())
}

// ==========================================
// USER COMMANDS
// ==========================================
#[tauri::command]
pub fn list_users_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<User>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin"])?;
    repo::list_users(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_user_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    email: String,
    password_hash: String, // Plain from client or hashed, we hash it here
    role: String,
) -> Result<User, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin"])?;
    let hashed = auth::hash_password(&password_hash)?;
    repo::create_user(&conn, &name, &email, &hashed, &role, Some(&actor.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_user_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    name: String,
    email: String,
    role: String,
) -> Result<User, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin"])?;
    repo::update_user(&conn, &id, &name, &email, &role, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_user_cmd(state: State<'_, DbState>, token: String, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin"])?;
    repo::delete_user(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

// ==========================================
// INVENTORY COMMANDS
// ==========================================
#[tauri::command]
pub fn create_category_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
) -> Result<Category, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::create_category(&conn, &name, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_categories_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<Category>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_categories(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_category_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_category(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_brand_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
) -> Result<Brand, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::create_brand(&conn, &name, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_brands_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Brand>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_brands(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_brand_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_brand(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_product_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    sku: String,
    barcode: Option<String>,
    qr_code: Option<String>,
    category_id: Option<String>,
    brand_id: Option<String>,
    cost: f64,
    price: f64,
    stock: f64,
    reorder_level: f64,
    is_expiry_tracked: i32,
    expiry_date: Option<String>,
    unit: String,
    description: Option<String>,
    image_path: Option<String>,
    tax_id: Option<String>,
    unit_id: Option<String>,
    offer_price: Option<f64>,
) -> Result<Product, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::create_product(
        &conn,
        &name,
        &sku,
        barcode.as_deref(),
        qr_code.as_deref(),
        category_id.as_deref(),
        brand_id.as_deref(),
        cost,
        price,
        stock,
        reorder_level,
        is_expiry_tracked,
        expiry_date.as_deref(),
        &unit,
        description.as_deref(),
        image_path.as_deref(),
        tax_id.as_deref(),
        unit_id.as_deref(),
        offer_price,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_product_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    name: String,
    sku: String,
    barcode: Option<String>,
    qr_code: Option<String>,
    category_id: Option<String>,
    brand_id: Option<String>,
    cost: f64,
    price: f64,
    reorder_level: f64,
    is_expiry_tracked: i32,
    expiry_date: Option<String>,
    unit: String,
    description: Option<String>,
    image_path: Option<String>,
    tax_id: Option<String>,
    unit_id: Option<String>,
    offer_price: Option<f64>,
) -> Result<Product, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::update_product(
        &conn,
        &id,
        &name,
        &sku,
        barcode.as_deref(),
        qr_code.as_deref(),
        category_id.as_deref(),
        brand_id.as_deref(),
        cost,
        price,
        reorder_level,
        is_expiry_tracked,
        expiry_date.as_deref(),
        &unit,
        description.as_deref(),
        image_path.as_deref(),
        tax_id.as_deref(),
        unit_id.as_deref(),
        offer_price,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_product_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_product(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_products_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Product>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_products(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_product_by_id_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<Product, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::get_product_by_id(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_products_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    category_id: Option<String>,
    brand_id: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedProducts, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    let (products, total_count) = repo::list_products_filtered(
        &conn,
        search.as_deref(),
        category_id.as_deref(),
        brand_id.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;

    Ok(PaginatedProducts {
        products,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn adjust_stock_cmd(
    state: State<'_, DbState>,
    token: String,
    product_id: String,
    adjustment_type: String,
    quantity: f64,
    reason: Option<String>,
) -> Result<Product, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::adjust_stock(
        &conn,
        &product_id,
        &adjustment_type,
        quantity,
        reason.as_deref(),
        &actor.id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_stock_adjustments_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<StockAdjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::list_stock_adjustments(&conn).map_err(|e| e.to_string())
}

// ==========================================
// CRM COMMANDS
// ==========================================
#[tauri::command]
pub fn create_customer_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
) -> Result<Customer, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::create_customer(
        &conn,
        &name,
        email.as_deref(),
        phone.as_deref(),
        address.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_customer_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    due_amount: f64,
) -> Result<Customer, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::update_customer(
        &conn,
        &id,
        &name,
        email.as_deref(),
        phone.as_deref(),
        address.as_deref(),
        due_amount,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_customers_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<Customer>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_customers(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_customer_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_customer(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_supplier_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
) -> Result<Supplier, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::create_supplier(
        &conn,
        &name,
        email.as_deref(),
        phone.as_deref(),
        address.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_supplier_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    balance: f64,
) -> Result<Supplier, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::update_supplier(
        &conn,
        &id,
        &name,
        email.as_deref(),
        phone.as_deref(),
        address.as_deref(),
        balance,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_suppliers_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<Supplier>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_suppliers(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_supplier_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_supplier(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_lead_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    contact_name: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    stage: String,
    deal_value: f64,
    notes: Option<String>,
) -> Result<Lead, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::create_lead(
        &conn,
        &name,
        contact_name.as_deref(),
        phone.as_deref(),
        email.as_deref(),
        &stage,
        deal_value,
        notes.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_lead_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    name: String,
    contact_name: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    stage: String,
    deal_value: f64,
    notes: Option<String>,
) -> Result<Lead, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::update_lead(
        &conn,
        &id,
        &name,
        contact_name.as_deref(),
        phone.as_deref(),
        email.as_deref(),
        &stage,
        deal_value,
        notes.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_leads_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Lead>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_leads(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_lead_cmd(state: State<'_, DbState>, token: String, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_lead(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

// ==========================================
// BILLING / INVOICING COMMANDS
// ==========================================
#[tauri::command]
pub fn create_invoice_cmd(
    state: State<'_, DbState>,
    token: String,
    customer_id: String,
    subtotal: f64,
    discount: f64,
    tax: f64,
    total: f64,
    amount_paid: f64,
    payment_method: Option<String>,
    items: Vec<InvoiceItem>,
) -> Result<Invoice, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::create_invoice(
        &conn,
        &customer_id,
        subtotal,
        discount,
        tax,
        total,
        amount_paid,
        payment_method.as_deref(),
        items,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_invoice_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<Invoice, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::get_invoice(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_invoices_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Invoice>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_invoices(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_quotation_cmd(
    state: State<'_, DbState>,
    token: String,
    customer_id: String,
    total: f64,
    status: String,
    items: Vec<QuotationItem>,
) -> Result<Quotation, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::create_quotation(&conn, &customer_id, total, &status, items, Some(&actor.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_quotation_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<Quotation, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::get_quotation(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_quotations_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<Quotation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_quotations(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_purchase_cmd(
    state: State<'_, DbState>,
    token: String,
    supplier_id: String,
    total: f64,
    status: String,
    items: Vec<PurchaseItem>,
) -> Result<Purchase, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::create_purchase(&conn, &supplier_id, total, &status, items, Some(&actor.id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_purchase_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<Purchase, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::get_purchase(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_purchases_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<Purchase>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_purchases(&conn).map_err(|e| e.to_string())
}

// ==========================================
// ACCOUNTING COMMANDS
// ==========================================
#[tauri::command]
pub fn create_payment_cmd(
    state: State<'_, DbState>,
    token: String,
    payment_type: String,
    entity_id: String,
    amount: f64,
    payment_method: String,
    reference_no: Option<String>,
) -> Result<Payment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::create_payment(
        &conn,
        &payment_type,
        &entity_id,
        amount,
        &payment_method,
        reference_no.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_payments_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Payment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::list_payments(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_expense_cmd(
    state: State<'_, DbState>,
    token: String,
    title: String,
    category: String,
    amount: f64,
    date: String,
    description: Option<String>,
) -> Result<Expense, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::create_expense(
        &conn,
        &title,
        &category,
        amount,
        &date,
        description.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_expenses_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Expense>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::list_expenses(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_expense_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::delete_expense(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_chart_of_accounts_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<ChartOfAccount>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::list_chart_of_accounts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_chart_of_account_cmd(
    state: State<'_, DbState>,
    token: String,
    code: String,
    name: String,
    account_type: String,
    parent_id: Option<String>,
) -> Result<ChartOfAccount, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::create_chart_of_account(
        &conn,
        &code,
        &name,
        &account_type,
        parent_id.as_deref(),
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_journal_entry_cmd(
    state: State<'_, DbState>,
    token: String,
    entry_date: String,
    description: String,
    reference: Option<String>,
    lines: Vec<JournalLine>,
) -> Result<JournalEntry, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::create_journal_entry(
        &conn,
        &entry_date,
        &description,
        reference.as_deref(),
        lines,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_journal_entries_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<JournalEntry>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::list_journal_entries(&conn).map_err(|e| e.to_string())
}

// ==========================================
// METRICS & REPORT COMMANDS
// ==========================================
#[tauri::command]
pub fn get_dashboard_stats_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<DashboardStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::calculate_dashboard_stats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_sales_chart_points_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<ChartPoint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::get_sales_chart_points(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_tax_summary_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<ChartPoint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::get_tax_summary(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tax_rates_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<TaxRate>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    repo::list_tax_rates(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tax_rate_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    rate: f64,
) -> Result<TaxRate, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    repo::create_tax_rate(&conn, &name, rate, Some(&actor.id)).map_err(|e| e.to_string())
}

// ==========================================
// BACKUP AND RESTORE
// ==========================================
#[tauri::command]
pub fn backup_database_cmd(
    state: State<'_, DbState>,
    token: String,
    dest_path: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin"])?;

    // Perform SQLite online backup
    conn.execute("VACUUM INTO ?1", rusqlite::params![dest_path])
        .map_err(|e| e.to_string())?;

    repo::log_audit(
        &conn,
        Some(&actor.id),
        "DATABASE_BACKUP",
        "database",
        None,
        None,
        Some(&dest_path),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn restore_database_cmd(
    state: State<'_, DbState>,
    _app_handle: tauri::AppHandle,
    token: String,
    src_path: String,
) -> Result<(), String> {
    // We cannot easily replace the db file while SQLite connection is open,
    // so we will lock the connection, copy the source DB to a temp path,
    // run backup APIs, or restore by copying.
    // In rusqlite, we can copy the contents or use backup APIs:
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin"])?;

    // Open the backup file
    let src_conn = Connection::open(&src_path).map_err(|e| e.to_string())?;

    // We can restore by restoring the sqlite db contents:
    // We will use rusqlite backup API in a nested block to drop borrow early
    {
        let backup =
            rusqlite::backup::Backup::new(&src_conn, &mut conn).map_err(|e| e.to_string())?;

        backup
            .run_to_completion(100, std::time::Duration::from_millis(250), None)
            .map_err(|e| e.to_string())?;
    }

    repo::log_audit(
        &conn,
        Some(&actor.id),
        "DATABASE_RESTORE",
        "database",
        None,
        None,
        Some(&src_path),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn hold_order_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    customer_name: String,
    discount: f64,
    tax_rate: f64,
    items: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let _actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO held_orders (id, customer_name, discount, tax_rate, items, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            customer_name = excluded.customer_name,
            discount = excluded.discount,
            tax_rate = excluded.tax_rate,
            items = excluded.items",
        rusqlite::params![id, customer_name, discount, tax_rate, items, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_held_orders_cmd(
    state: State<'_, DbState>,
    token: String,
) -> Result<Vec<HeldOrder>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "staff"])?;
    let mut stmt = conn.prepare("SELECT id, customer_name, discount, tax_rate, items, created_at FROM held_orders ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(HeldOrder {
                id: row.get(0)?,
                customer_name: row.get(1)?,
                discount: row.get(2)?,
                tax_rate: row.get(3)?,
                items: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| e.to_string())?);
    }
    Ok(list)
}

#[tauri::command]
pub fn delete_held_order_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "staff"])?;
    conn.execute(
        "DELETE FROM held_orders WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn bulk_import_products_cmd(
    state: State<'_, DbState>,
    token: String,
    products: Vec<ImportProductInput>,
) -> Result<(), String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;

    // Begin transaction
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();

    {
        let mut stmt_cat = tx
            .prepare("SELECT id FROM categories WHERE name = ?1")
            .map_err(|e| e.to_string())?;
        let mut stmt_brand = tx
            .prepare("SELECT id FROM brands WHERE name = ?1")
            .map_err(|e| e.to_string())?;

        for p in products {
            // 1. Category resolve/create
            let mut category_id: Option<String> = None;
            if let Some(cat_name) = &p.category_name {
                let cat_name_trimmed = cat_name.trim();
                if !cat_name_trimmed.is_empty() {
                    let existing: Result<String, _> =
                        stmt_cat.query_row(rusqlite::params![cat_name_trimmed], |r| r.get(0));
                    match existing {
                        Ok(id) => category_id = Some(id),
                        Err(_) => {
                            let new_id = uuid::Uuid::new_v4().to_string();
                            tx.execute(
                                "INSERT INTO categories (id, name, created_at, is_deleted) VALUES (?1, ?2, ?3, 0)",
                                rusqlite::params![new_id, cat_name_trimmed, now],
                            ).map_err(|e| e.to_string())?;
                            category_id = Some(new_id);
                        }
                    }
                }
            }

            // 2. Brand resolve/create
            let mut brand_id: Option<String> = None;
            if let Some(br_name) = &p.brand_name {
                let br_name_trimmed = br_name.trim();
                if !br_name_trimmed.is_empty() {
                    let existing: Result<String, _> =
                        stmt_brand.query_row(rusqlite::params![br_name_trimmed], |r| r.get(0));
                    match existing {
                        Ok(id) => brand_id = Some(id),
                        Err(_) => {
                            let new_id = uuid::Uuid::new_v4().to_string();
                            tx.execute(
                                "INSERT INTO brands (id, name, created_at, is_deleted) VALUES (?1, ?2, ?3, 0)",
                                rusqlite::params![new_id, br_name_trimmed, now],
                            ).map_err(|e| e.to_string())?;
                            brand_id = Some(new_id);
                        }
                    }
                }
            }

            // 3. Insert product
            let prod_id = uuid::Uuid::new_v4().to_string();
            let cost_val = p.cost.unwrap_or(0.0);
            let price_val = p.price.unwrap_or(0.0);
            let stock_val = p.stock.unwrap_or(0.0);
            let reorder_val = p.reorder_level.unwrap_or(0.0);
            let unit_val = p.unit.unwrap_or_else(|| "pcs".to_string());

            tx.execute(
                "INSERT INTO products (id, name, sku, barcode, category_id, brand_id, cost, price, stock, reorder_level, created_at, updated_at, unit, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)
                 ON CONFLICT(sku) DO UPDATE SET
                    name = excluded.name,
                    barcode = excluded.barcode,
                    category_id = excluded.category_id,
                    brand_id = excluded.brand_id,
                    cost = excluded.cost,
                    price = excluded.price,
                    stock = excluded.stock,
                    reorder_level = excluded.reorder_level,
                    updated_at = excluded.updated_at,
                    unit = excluded.unit",
                rusqlite::params![
                    prod_id,
                    p.name,
                    p.sku,
                    p.barcode,
                    category_id,
                    brand_id,
                    cost_val,
                    price_val,
                    stock_val,
                    reorder_val,
                    now,
                    now,
                    unit_val
                ],
            ).map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| e.to_string())?;

    // Log audit for bulk import
    repo::log_audit(
        &conn,
        Some(&actor.id),
        "BULK_IMPORT",
        "products",
        None,
        None,
        None,
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn update_invoice_status_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;

    let inv = repo::get_invoice(&conn, &id).map_err(|e| e.to_string())?;
    let old_due = inv.due_amount;
    let old_serialized = serde_json::to_string(&inv).unwrap_or_default();

    if status == "Paid" {
        conn.execute(
            "UPDATE invoices SET payment_status = 'Paid', amount_paid = total, due_amount = 0.0 WHERE id = ?1",
            rusqlite::params![id],
        ).map_err(|e| e.to_string())?;

        conn.execute(
            "UPDATE customers SET due_amount = due_amount - ?1 WHERE id = ?2",
            rusqlite::params![old_due, inv.customer_id],
        )
        .map_err(|e| e.to_string())?;
    } else if status == "Cancel" {
        conn.execute(
            "UPDATE invoices SET payment_status = 'Cancel', due_amount = 0.0 WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;

        conn.execute(
            "UPDATE customers SET due_amount = due_amount - ?1 WHERE id = ?2",
            rusqlite::params![old_due, inv.customer_id],
        )
        .map_err(|e| e.to_string())?;

        // Revert product stock and log adjustments
        for item in &inv.items {
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE products SET stock = stock + ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![item.quantity, now, item.product_id],
            )
            .map_err(|e| e.to_string())?;

            let adj_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
                 VALUES (?1, ?2, 'ADD', ?3, ?4, ?5, ?6)",
                rusqlite::params![adj_id, item.product_id, item.quantity, format!("Invoice {} Cancelled", inv.invoice_number), actor.id, now],
            ).map_err(|e| e.to_string())?;
        }
    } else {
        conn.execute(
            "UPDATE invoices SET payment_status = ?1 WHERE id = ?2",
            rusqlite::params![status, id],
        )
        .map_err(|e| e.to_string())?;
    }

    let updated = repo::get_invoice(&conn, &id).map_err(|e| e.to_string())?;
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    repo::log_audit(
        &conn,
        Some(&actor.id),
        "UPDATE_STATUS",
        "invoices",
        Some(&id),
        Some(&old_serialized),
        Some(&new_serialized),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn receive_purchase_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
) -> Result<Purchase, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::receive_purchase(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_invoice_cmd(
    state: State<'_, DbState>,
    token: String,
    id: String,
    customer_id: String,
    subtotal: f64,
    discount: f64,
    tax: f64,
    total: f64,
    amount_paid: f64,
    payment_method: Option<String>,
    items: Vec<InvoiceItem>,
) -> Result<Invoice, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager", "staff"])?;
    repo::update_invoice(
        &conn,
        &id,
        &customer_id,
        subtotal,
        discount,
        tax,
        total,
        amount_paid,
        payment_method.as_deref(),
        items,
        Some(&actor.id),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_customers_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedCustomers, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    let (customers, total_count) = repo::list_customers_filtered(
        &conn,
        search.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedCustomers {
        customers,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_suppliers_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedSuppliers, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    let (suppliers, total_count) = repo::list_suppliers_filtered(
        &conn,
        search.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedSuppliers {
        suppliers,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_leads_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    stage: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedLeads, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "staff"])?;
    let (leads, total_count) = repo::list_leads_filtered(
        &conn,
        search.as_deref(),
        stage.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedLeads {
        leads,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_invoices_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    payment_status: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedInvoices, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    let (invoices, total_count) = repo::list_invoices_filtered(
        &conn,
        search.as_deref(),
        payment_status.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedInvoices {
        invoices,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_quotations_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    status: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedQuotations, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    let (quotations, total_count) = repo::list_quotations_filtered(
        &conn,
        search.as_deref(),
        status.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedQuotations {
        quotations,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_purchases_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    status: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedPurchases, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant", "staff"])?;
    let (purchases, total_count) = repo::list_purchases_filtered(
        &conn,
        search.as_deref(),
        status.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedPurchases {
        purchases,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_expenses_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    category: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedExpenses, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    let (expenses, total_count) = repo::list_expenses_filtered(
        &conn,
        search.as_deref(),
        category.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedExpenses {
        expenses,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn list_payments_filtered_cmd(
    state: State<'_, DbState>,
    token: String,
    search: Option<String>,
    payment_type: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    page: i32,
    limit: i32,
) -> Result<PaginatedPayments, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(&conn, &token, &["admin", "manager", "accountant"])?;
    let (payments, total_count) = repo::list_payments_filtered(
        &conn,
        search.as_deref(),
        payment_type.as_deref(),
        sort_by.as_deref(),
        sort_order.as_deref(),
        page,
        limit,
    )
    .map_err(|e| e.to_string())?;
    Ok(PaginatedPayments {
        payments,
        total_count,
        page,
        limit,
    })
}

#[tauri::command]
pub fn create_unit_cmd(
    state: State<'_, DbState>,
    token: String,
    name: String,
    short_name: String,
) -> Result<Unit, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::create_unit(&conn, &name, &short_name, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_units_cmd(state: State<'_, DbState>, token: String) -> Result<Vec<Unit>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(
        &conn,
        &token,
        &["admin", "manager", "cashier", "accountant"],
    )?;
    repo::list_units(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_unit_cmd(state: State<'_, DbState>, token: String, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let actor = authorize(&conn, &token, &["admin", "manager"])?;
    repo::delete_unit(&conn, &id, Some(&actor.id)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_product_transactions_cmd(
    state: State<'_, DbState>,
    token: String,
    product_id: String,
) -> Result<Vec<ProductTransaction>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    authorize(
        &conn,
        &token,
        &["admin", "manager", "cashier", "accountant"],
    )?;
    repo::get_product_transactions(&conn, &product_id).map_err(|e| e.to_string())
}
