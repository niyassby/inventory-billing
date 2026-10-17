use rusqlite::{params, Connection, Result, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::models::*;

// ==========================================
// AUDIT LOGS
// ==========================================
pub fn log_audit(
    conn: &Connection,
    user_id: Option<&str>,
    action: &str,
    table_name: &str,
    record_id: Option<&str>,
    old_values: Option<&str>,
    new_values: Option<&str>,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_logs (id, user_id, action, table_name, record_id, old_values, new_values, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, user_id, action, table_name, record_id, old_values, new_values, now],
    )?;
    Ok(())
}

pub fn get_audit_logs(conn: &Connection) -> Result<Vec<AuditLog>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.user_id, u.name, a.action, a.table_name, a.record_id, a.old_values, a.new_values, a.timestamp
         FROM audit_logs a
         LEFT JOIN users u ON a.user_id = u.id
         ORDER BY a.timestamp DESC LIMIT 100",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(AuditLog {
            id: row.get(0)?,
            user_id: row.get(1)?,
            user_name: row.get(2)?,
            action: row.get(3)?,
            table_name: row.get(4)?,
            record_id: row.get(5)?,
            old_values: row.get(6)?,
            new_values: row.get(7)?,
            timestamp: row.get(8)?,
        })
    })?;
    
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

// ==========================================
// USER REPO
// ==========================================
// pub fn get_user_by_email(conn: &Connection, email: &str) -> Result<User> {
//     conn.query_row(
//         "SELECT id, name, email, role, created_at, updated_at FROM users WHERE email = ?1 AND is_deleted = 0",
//         params![email],
//         |row| {
//             Ok(User {
//                 id: row.get(0)?,
//                 name: row.get(1)?,
//                 email: row.get(2)?,
//                 role: row.get(3)?,
//                 created_at: row.get(4)?,
//                 updated_at: row.get(5)?,
//             })
//         },
//     )
// }

pub fn get_user_by_id(conn: &Connection, id: &str) -> Result<User> {
    conn.query_row(
        "SELECT id, name, email, role, created_at, updated_at FROM users WHERE id = ?1 AND is_deleted = 0",
        params![id],
        |row| {
            Ok(User {
                id: row.get(0)?,
                name: row.get(1)?,
                email: row.get(2)?,
                role: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
}

pub fn get_user_password_hash(conn: &Connection, email: &str) -> Result<(String, String)> {
    conn.query_row(
        "SELECT id, password_hash FROM users WHERE email = ?1 AND is_deleted = 0",
        params![email],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
}

pub fn create_user(
    conn: &Connection,
    name: &str,
    email: &str,
    password_hash: &str,
    role: &str,
    actor_id: Option<&str>,
) -> Result<User> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO users (id, name, email, password_hash, role, created_at, updated_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![id, name, email, password_hash, role, now, now],
    )?;

    let new_user = User {
        id: id.clone(),
        name: name.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    let serialized = serde_json::to_string(&new_user).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "users", Some(&id), None, Some(&serialized))?;

    Ok(new_user)
}

pub fn update_user(
    conn: &Connection,
    id: &str,
    name: &str,
    email: &str,
    role: &str,
    actor_id: Option<&str>,
) -> Result<User> {
    let old_user = get_user_by_id(conn, id)?;
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE users SET name = ?1, email = ?2, role = ?3, updated_at = ?4 WHERE id = ?5",
        params![name, email, role, now, id],
    )?;

    let updated = get_user_by_id(conn, id)?;
    let old_serialized = serde_json::to_string(&old_user).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(
        conn,
        actor_id,
        "UPDATE",
        "users",
        Some(id),
        Some(&old_serialized),
        Some(&new_serialized),
    )?;

    Ok(updated)
}

pub fn delete_user(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    let old_user = get_user_by_id(conn, id)?;
    conn.execute(
        "UPDATE users SET is_deleted = 1 WHERE id = ?1",
        params![id],
    )?;

    let old_serialized = serde_json::to_string(&old_user).unwrap_or_default();
    log_audit(conn, actor_id, "DELETE", "users", Some(id), Some(&old_serialized), None)?;
    Ok(())
}

pub fn list_users(conn: &Connection) -> Result<Vec<User>> {
    let mut stmt = conn.prepare("SELECT id, name, email, role, created_at, updated_at FROM users WHERE is_deleted = 0")?;
    let rows = stmt.query_map([], |row| {
        Ok(User {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            role: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

// ==========================================
// SESSIONS
// ==========================================
pub fn create_session(conn: &Connection, user_id: &str, token_hash: &str) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires = now + chrono::Duration::hours(8);
    
    conn.execute(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, user_id, token_hash, expires.to_rfc3339(), now.to_rfc3339()],
    )?;
    Ok(())
}

pub fn validate_session(conn: &Connection, token_hash: &str) -> Result<User> {
    let now = Utc::now().to_rfc3339();
    let user_id: String = conn.query_row(
        "SELECT user_id FROM sessions WHERE token_hash = ?1 AND expires_at > ?2",
        params![token_hash, now],
        |row| row.get(0),
    )?;

    get_user_by_id(conn, &user_id)
}

pub fn delete_session(conn: &Connection, token_hash: &str) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE token_hash = ?1", params![token_hash])?;
    Ok(())
}

// ==========================================
// CATEGORY & BRAND REPOS
// ==========================================
pub fn create_category(conn: &Connection, name: &str, actor_id: Option<&str>) -> Result<Category> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO categories (id, name, created_at, is_deleted) VALUES (?1, ?2, ?3, 0)",
        params![id, name, now],
    )?;

    let cat = Category { id: id.clone(), name: name.to_string(), created_at: now };
    let serialized = serde_json::to_string(&cat).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "categories", Some(&id), None, Some(&serialized))?;
    Ok(cat)
}

pub fn list_categories(conn: &Connection) -> Result<Vec<Category>> {
    let mut stmt = conn.prepare("SELECT id, name, created_at FROM categories WHERE is_deleted = 0 ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Category {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_category(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("UPDATE categories SET is_deleted = 1 WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "categories", Some(id), None, None)?;
    Ok(())
}

pub fn create_brand(conn: &Connection, name: &str, actor_id: Option<&str>) -> Result<Brand> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO brands (id, name, created_at, is_deleted) VALUES (?1, ?2, ?3, 0)",
        params![id, name, now],
    )?;

    let br = Brand { id: id.clone(), name: name.to_string(), created_at: now };
    let serialized = serde_json::to_string(&br).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "brands", Some(&id), None, Some(&serialized))?;
    Ok(br)
}

pub fn list_brands(conn: &Connection) -> Result<Vec<Brand>> {
    let mut stmt = conn.prepare("SELECT id, name, created_at FROM brands WHERE is_deleted = 0 ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Brand {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_brand(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("UPDATE brands SET is_deleted = 1 WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "brands", Some(id), None, None)?;
    Ok(())
}

// ==========================================
// PRODUCT REPO
// ==========================================
fn map_product_row(row: &Row) -> Result<Product> {
    Ok(Product {
        id: row.get(0)?,
        name: row.get(1)?,
        sku: row.get(2)?,
        barcode: row.get(3)?,
        qr_code: row.get(4)?,
        category_id: row.get(5)?,
        category_name: row.get(6)?,
        brand_id: row.get(7)?,
        brand_name: row.get(8)?,
        cost: row.get(9)?,
        price: row.get(10)?,
        stock: row.get(11)?,
        reorder_level: row.get(12)?,
        is_expiry_tracked: row.get(13)?,
        expiry_date: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        unit: row.get(17)?,
        description: row.get(18)?,
        image_path: row.get(19)?,
        tax_id: row.get(20)?,
        tax_name: row.get(21)?,
        tax_rate: row.get(22)?,
        unit_id: row.get(23)?,
        unit_name: row.get(24)?,
        offer_price: row.get(25)?,
    })
}

pub fn get_product_by_id(conn: &Connection, id: &str) -> Result<Product> {
    conn.query_row(
        "SELECT p.id, p.name, p.sku, p.barcode, p.qr_code, p.category_id, c.name, p.brand_id, b.name,
                p.cost, p.price, p.stock, p.reorder_level, p.is_expiry_tracked, p.expiry_date, p.created_at, p.updated_at, p.unit,
                p.description, p.image_path, p.tax_id, t.name, t.rate, p.unit_id, u.name, p.offer_price
         FROM products p
         LEFT JOIN categories c ON p.category_id = c.id
         LEFT JOIN brands b ON p.brand_id = b.id
         LEFT JOIN tax_rates t ON p.tax_id = t.id
         LEFT JOIN units u ON p.unit_id = u.id
         WHERE p.id = ?1 AND p.is_deleted = 0",
        params![id],
        map_product_row,
    )
}

pub fn create_product(
    conn: &Connection,
    name: &str,
    sku: &str,
    barcode: Option<&str>,
    qr_code: Option<&str>,
    category_id: Option<&str>,
    brand_id: Option<&str>,
    cost: f64,
    price: f64,
    stock: f64,
    reorder_level: f64,
    is_expiry_tracked: i32,
    expiry_date: Option<&str>,
    unit: &str,
    description: Option<&str>,
    image_path: Option<&str>,
    tax_id: Option<&str>,
    unit_id: Option<&str>,
    offer_price: Option<f64>,
    actor_id: Option<&str>,
) -> Result<Product> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO products (id, name, sku, barcode, qr_code, category_id, brand_id, cost, price, stock,
                               reorder_level, is_expiry_tracked, expiry_date, created_at, updated_at, is_deleted, unit,
                               description, image_path, tax_id, unit_id, offer_price)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 0, ?16, ?17, ?18, ?19, ?20, ?21)",
        params![
            id,
            name,
            sku,
            barcode,
            qr_code,
            category_id,
            brand_id,
            cost,
            price,
            stock,
            reorder_level,
            is_expiry_tracked,
            expiry_date,
            now,
            now,
            unit,
            description,
            image_path,
            tax_id,
            unit_id,
            offer_price.unwrap_or(0.0)
        ],
    )?;

    let prod = get_product_by_id(conn, &id)?;
    let serialized = serde_json::to_string(&prod).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "products", Some(&id), None, Some(&serialized))?;
    Ok(prod)
}

pub fn update_product(
    conn: &Connection,
    id: &str,
    name: &str,
    sku: &str,
    barcode: Option<&str>,
    qr_code: Option<&str>,
    category_id: Option<&str>,
    brand_id: Option<&str>,
    cost: f64,
    price: f64,
    reorder_level: f64,
    is_expiry_tracked: i32,
    expiry_date: Option<&str>,
    unit: &str,
    description: Option<&str>,
    image_path: Option<&str>,
    tax_id: Option<&str>,
    unit_id: Option<&str>,
    offer_price: Option<f64>,
    actor_id: Option<&str>,
) -> Result<Product> {
    let old_prod = get_product_by_id(conn, id)?;
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE products SET name = ?1, sku = ?2, barcode = ?3, qr_code = ?4, category_id = ?5, brand_id = ?6,
                             cost = ?7, price = ?8, reorder_level = ?9, is_expiry_tracked = ?10, expiry_date = ?11,
                             updated_at = ?12, unit = ?13, description = ?14, image_path = ?15, tax_id = ?16,
                             unit_id = ?17, offer_price = ?18 WHERE id = ?19",
        params![
            name,
            sku,
            barcode,
            qr_code,
            category_id,
            brand_id,
            cost,
            price,
            reorder_level,
            is_expiry_tracked,
            expiry_date,
            now,
            unit,
            description,
            image_path,
            tax_id,
            unit_id,
            offer_price.unwrap_or(0.0),
            id
        ],
    )?;

    let updated = get_product_by_id(conn, id)?;
    let old_serialized = serde_json::to_string(&old_prod).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(
        conn,
        actor_id,
        "UPDATE",
        "products",
        Some(id),
        Some(&old_serialized),
        Some(&new_serialized),
    )?;

    Ok(updated)
}

pub fn delete_product(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    let old_prod = get_product_by_id(conn, id)?;
    conn.execute("UPDATE products SET is_deleted = 1 WHERE id = ?1", params![id])?;

    let old_serialized = serde_json::to_string(&old_prod).unwrap_or_default();
    log_audit(conn, actor_id, "DELETE", "products", Some(id), Some(&old_serialized), None)?;
    Ok(())
}

pub fn list_products(conn: &Connection) -> Result<Vec<Product>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.sku, p.barcode, p.qr_code, p.category_id, c.name, p.brand_id, b.name,
                p.cost, p.price, p.stock, p.reorder_level, p.is_expiry_tracked, p.expiry_date, p.created_at, p.updated_at, p.unit,
                p.description, p.image_path, p.tax_id, t.name, t.rate, p.unit_id, u.name, p.offer_price
         FROM products p
         LEFT JOIN categories c ON p.category_id = c.id
         LEFT JOIN brands b ON p.brand_id = b.id
         LEFT JOIN tax_rates t ON p.tax_id = t.id
         LEFT JOIN units u ON p.unit_id = u.id
         WHERE p.is_deleted = 0 ORDER BY p.name ASC",
    )?;
    let rows = stmt.query_map([], map_product_row)?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn list_products_filtered(
    conn: &Connection,
    search: Option<&str>,
    category_id: Option<&str>,
    brand_id: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Product>, i64)> {
    let mut query = String::from(
        "FROM products p
         LEFT JOIN categories c ON p.category_id = c.id
         LEFT JOIN brands b ON p.brand_id = b.id
         LEFT JOIN tax_rates t ON p.tax_id = t.id
         LEFT JOIN units u ON p.unit_id = u.id
         WHERE p.is_deleted = 0"
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (p.name LIKE ?{} OR p.sku LIKE ?{} OR p.barcode LIKE ?{})", param_index, param_index + 1, param_index + 2));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 3;
        }
    }

    if let Some(cat) = category_id {
        if !cat.is_empty() {
            query.push_str(&format!(" AND p.category_id = ?{}", param_index));
            params_vec.push(Box::new(cat.to_string()));
            param_index += 1;
        }
    }

    if let Some(br) = brand_id {
        if !br.is_empty() {
            query.push_str(&format!(" AND p.brand_id = ?{}", param_index));
            params_vec.push(Box::new(br.to_string()));
            param_index += 1;
        }
    }

    // Count query
    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    
    // We need to convert params_vec to references
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    // Sort order & By
    let sort_col = match sort_by.unwrap_or("name") {
        "name" => "p.name",
        "price" => "p.price",
        "stock" => "p.stock",
        "sku" => "p.sku",
        "cost" => "p.cost",
        _ => "p.name",
    };
    
    let order = match sort_order.unwrap_or("ASC") {
        "DESC" => "DESC",
        _ => "ASC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));

    // Pagination
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!(
        "SELECT p.id, p.name, p.sku, p.barcode, p.qr_code, p.category_id, c.name, p.brand_id, b.name,
                p.cost, p.price, p.stock, p.reorder_level, p.is_expiry_tracked, p.expiry_date, p.created_at, p.updated_at, p.unit,
                p.description, p.image_path, p.tax_id, t.name, t.rate, p.unit_id, u.name, p.offer_price {}",
        query
    );

    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    
    let rows = stmt_select.query_map(&select_params[..], map_product_row)?;
    let mut products = Vec::new();
    for r in rows {
        products.push(r?);
    }

    Ok((products, total_count))
}

pub fn adjust_stock(
    conn: &Connection,
    product_id: &str,
    adjustment_type: &str, // 'ADD', 'SUBTRACT', 'SET'
    quantity: f64,
    reason: Option<&str>,
    actor_id: &str,
) -> Result<Product> {
    let old_prod = get_product_by_id(conn, product_id)?;
    let current_stock = old_prod.stock;

    let new_stock = match adjustment_type {
        "ADD" => current_stock + quantity,
        "SUBTRACT" => current_stock - quantity,
        "SET" => quantity,
        _ => current_stock,
    };

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE products SET stock = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_stock, now, product_id],
    )?;

    let adj_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![adj_id, product_id, adjustment_type, quantity, reason, actor_id, now],
    )?;

    let updated = get_product_by_id(conn, product_id)?;
    let old_serialized = serde_json::to_string(&old_prod).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(
        conn,
        Some(actor_id),
        "STOCK_ADJUSTMENT",
        "products",
        Some(product_id),
        Some(&old_serialized),
        Some(&new_serialized),
    )?;

    Ok(updated)
}

pub fn list_stock_adjustments(conn: &Connection) -> Result<Vec<StockAdjustment>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.product_id, p.name, s.adjustment_type, s.quantity, s.reason, s.adjusted_by, u.name, s.created_at
         FROM stock_adjustments s
         JOIN products p ON s.product_id = p.id
         JOIN users u ON s.adjusted_by = u.id
         ORDER BY s.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(StockAdjustment {
            id: row.get(0)?,
            product_id: row.get(1)?,
            product_name: row.get(2)?,
            adjustment_type: row.get(3)?,
            quantity: row.get(4)?,
            reason: row.get(5)?,
            adjusted_by: row.get(6)?,
            adjusted_by_name: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

// ==========================================
// CRM: CUSTOMER, SUPPLIER & LEAD REPOS
// ==========================================
pub fn create_customer(
    conn: &Connection,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Customer> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO customers (id, name, email, phone, address, due_amount, created_at, updated_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, 0.0, ?6, ?7, 0)",
        params![id, name, email, phone, address, now, now],
    )?;

    let cust = Customer {
        id: id.clone(),
        name: name.to_string(),
        email: email.map(String::from),
        phone: phone.map(String::from),
        address: address.map(String::from),
        due_amount: 0.0,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let serialized = serde_json::to_string(&cust).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "customers", Some(&id), None, Some(&serialized))?;
    Ok(cust)
}

pub fn update_customer(
    conn: &Connection,
    id: &str,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
    due_amount: f64,
    actor_id: Option<&str>,
) -> Result<Customer> {
    let old_cust = conn.query_row(
        "SELECT id, name, email, phone, address, due_amount, created_at, updated_at FROM customers WHERE id = ?1",
        params![id],
        |row| Ok(Customer {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            phone: row.get(3)?,
            address: row.get(4)?,
            due_amount: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    )?;
    
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE customers SET name = ?1, email = ?2, phone = ?3, address = ?4, due_amount = ?5, updated_at = ?6 WHERE id = ?7",
        params![name, email, phone, address, due_amount, now, id],
    )?;

    let updated = Customer {
        id: id.to_string(),
        name: name.to_string(),
        email: email.map(String::from),
        phone: phone.map(String::from),
        address: address.map(String::from),
        due_amount,
        created_at: old_cust.created_at.clone(),
        updated_at: now,
    };

    let old_serialized = serde_json::to_string(&old_cust).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(conn, actor_id, "UPDATE", "customers", Some(id), Some(&old_serialized), Some(&new_serialized))?;
    Ok(updated)
}

pub fn list_customers(conn: &Connection) -> Result<Vec<Customer>> {
    let mut stmt = conn.prepare("SELECT id, name, email, phone, address, due_amount, created_at, updated_at FROM customers WHERE is_deleted = 0 ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Customer {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            phone: row.get(3)?,
            address: row.get(4)?,
            due_amount: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_customer(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("UPDATE customers SET is_deleted = 1 WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "customers", Some(id), None, None)?;
    Ok(())
}

pub fn create_supplier(
    conn: &Connection,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Supplier> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO suppliers (id, name, email, phone, address, balance, created_at, updated_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, 0.0, ?6, ?7, 0)",
        params![id, name, email, phone, address, now, now],
    )?;

    let supp = Supplier {
        id: id.clone(),
        name: name.to_string(),
        email: email.map(String::from),
        phone: phone.map(String::from),
        address: address.map(String::from),
        balance: 0.0,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let serialized = serde_json::to_string(&supp).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "suppliers", Some(&id), None, Some(&serialized))?;
    Ok(supp)
}

pub fn update_supplier(
    conn: &Connection,
    id: &str,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
    balance: f64,
    actor_id: Option<&str>,
) -> Result<Supplier> {
    let old_supp = conn.query_row(
        "SELECT id, name, email, phone, address, balance, created_at, updated_at FROM suppliers WHERE id = ?1",
        params![id],
        |row| Ok(Supplier {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            phone: row.get(3)?,
            address: row.get(4)?,
            balance: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    )?;
    
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE suppliers SET name = ?1, email = ?2, phone = ?3, address = ?4, balance = ?5, updated_at = ?6 WHERE id = ?7",
        params![name, email, phone, address, balance, now, id],
    )?;

    let updated = Supplier {
        id: id.to_string(),
        name: name.to_string(),
        email: email.map(String::from),
        phone: phone.map(String::from),
        address: address.map(String::from),
        balance,
        created_at: old_supp.created_at.clone(),
        updated_at: now,
    };

    let old_serialized = serde_json::to_string(&old_supp).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(conn, actor_id, "UPDATE", "suppliers", Some(id), Some(&old_serialized), Some(&new_serialized))?;
    Ok(updated)
}

pub fn list_suppliers(conn: &Connection) -> Result<Vec<Supplier>> {
    let mut stmt = conn.prepare("SELECT id, name, email, phone, address, balance, created_at, updated_at FROM suppliers WHERE is_deleted = 0 ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Supplier {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            phone: row.get(3)?,
            address: row.get(4)?,
            balance: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_supplier(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("UPDATE suppliers SET is_deleted = 1 WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "suppliers", Some(id), None, None)?;
    Ok(())
}

pub fn create_lead(
    conn: &Connection,
    name: &str,
    contact_name: Option<&str>,
    phone: Option<&str>,
    email: Option<&str>,
    stage: &str,
    deal_value: f64,
    notes: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Lead> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO leads (id, name, contact_name, phone, email, stage, deal_value, notes, created_at, updated_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
        params![id, name, contact_name, phone, email, stage, deal_value, notes, now, now],
    )?;

    let ld = Lead {
        id: id.clone(),
        name: name.to_string(),
        contact_name: contact_name.map(String::from),
        phone: phone.map(String::from),
        email: email.map(String::from),
        stage: stage.to_string(),
        deal_value,
        notes: notes.map(String::from),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let serialized = serde_json::to_string(&ld).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "leads", Some(&id), None, Some(&serialized))?;
    Ok(ld)
}

pub fn update_lead(
    conn: &Connection,
    id: &str,
    name: &str,
    contact_name: Option<&str>,
    phone: Option<&str>,
    email: Option<&str>,
    stage: &str,
    deal_value: f64,
    notes: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Lead> {
    let old_lead = conn.query_row(
        "SELECT id, name, contact_name, phone, email, stage, deal_value, notes, created_at, updated_at FROM leads WHERE id = ?1",
        params![id],
        |row| Ok(Lead {
            id: row.get(0)?,
            name: row.get(1)?,
            contact_name: row.get(2)?,
            phone: row.get(3)?,
            email: row.get(4)?,
            stage: row.get(5)?,
            deal_value: row.get(6)?,
            notes: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    )?;
    
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE leads SET name = ?1, contact_name = ?2, phone = ?3, email = ?4, stage = ?5, deal_value = ?6, notes = ?7, updated_at = ?8 WHERE id = ?9",
        params![name, contact_name, phone, email, stage, deal_value, notes, now, id],
    )?;

    let updated = Lead {
        id: id.to_string(),
        name: name.to_string(),
        contact_name: contact_name.map(String::from),
        phone: phone.map(String::from),
        email: email.map(String::from),
        stage: stage.to_string(),
        deal_value,
        notes: notes.map(String::from),
        created_at: old_lead.created_at.clone(),
        updated_at: now,
    };

    let old_serialized = serde_json::to_string(&old_lead).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(conn, actor_id, "UPDATE", "leads", Some(id), Some(&old_serialized), Some(&new_serialized))?;
    Ok(updated)
}

pub fn list_leads(conn: &Connection) -> Result<Vec<Lead>> {
    let mut stmt = conn.prepare("SELECT id, name, contact_name, phone, email, stage, deal_value, notes, created_at, updated_at FROM leads WHERE is_deleted = 0 ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Lead {
            id: row.get(0)?,
            name: row.get(1)?,
            contact_name: row.get(2)?,
            phone: row.get(3)?,
            email: row.get(4)?,
            stage: row.get(5)?,
            deal_value: row.get(6)?,
            notes: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_lead(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("UPDATE leads SET is_deleted = 1 WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "leads", Some(id), None, None)?;
    Ok(())
}

// ==========================================
// SALES: INVOICES & QUOTATIONS
// ==========================================
pub fn create_invoice(
    conn: &Connection,
    customer_id: &str,
    subtotal: f64,
    discount: f64,
    tax: f64,
    total: f64,
    amount_paid: f64,
    payment_method: Option<&str>,
    items: Vec<InvoiceItem>,
    actor_id: Option<&str>,
) -> Result<Invoice> {
    let id = Uuid::new_v4().to_string();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get(0))?;
    let invoice_number = format!("INV-{:06}", count + 1);
    let now_date = Utc::now().format("%Y-%m-%d").to_string();
    let now = Utc::now().to_rfc3339();
    
    let due_amount = total - amount_paid;
    let payment_status = if due_amount <= 0.0 {
        "Paid"
    } else if amount_paid > 0.0 {
        "Partial"
    } else {
        "Unpaid"
    };

    // Insert Invoice
    conn.execute(
        "INSERT INTO invoices (id, invoice_number, customer_id, subtotal, discount, tax, total, amount_paid,
                               due_amount, payment_status, payment_method, date, created_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)",
        params![id, invoice_number, customer_id, subtotal, discount, tax, total, amount_paid, due_amount, payment_status, payment_method, now_date, now],
    )?;

    // Update customer due amount
    conn.execute(
        "UPDATE customers SET due_amount = due_amount + ?1 WHERE id = ?2",
        params![due_amount, customer_id],
    )?;

    // Insert invoice items and update inventory stock!
    for item in &items {
        let item_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO invoice_items (id, invoice_id, product_id, quantity, unit_price, total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, id, item.product_id, item.quantity, item.unit_price, item.total],
        )?;

        // Subtract stock
        conn.execute(
            "UPDATE products SET stock = stock - ?1, updated_at = ?2 WHERE id = ?3",
            params![item.quantity, now, item.product_id],
        )?;
        
        // Stock adjustment log
        let adj_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
             VALUES (?1, ?2, 'SUBTRACT', ?3, ?4, ?5, ?6)",
            params![adj_id, item.product_id, item.quantity, format!("Invoice {}", invoice_number), actor_id.unwrap_or(""), now],
        )?;
    }

    // Double-entry accounting: Journal Entry for sale
    // Debit Accounts Receivable (1200) - Total
    // Debit Cash/Bank (1000/1010) - Amount Paid
    // Credit Sales Revenue (4000) - Subtotal
    let journal_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![journal_id, now_date, format!("Sale invoice {}", invoice_number), invoice_number, now],
    )?;

    // Find accounts IDs
    let ar_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1200'", [], |r| r.get(0))?;
    let sales_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '4000'", [], |r| r.get(0))?;
    let cash_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1000'", [], |r| r.get(0))?;

    // Debit Receivable
    if due_amount > 0.0 {
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &ar_acct_id, due_amount),
        )?;
    }
    // Debit Cash
    if amount_paid > 0.0 {
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &cash_acct_id, amount_paid),
        )?;
    }
    // Credit Revenue
    conn.execute(
        "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
         VALUES (?1, ?2, ?3, 0.0, ?4)",
        (Uuid::new_v4().to_string(), &journal_id, &sales_acct_id, total),
    )?;

    let inv = get_invoice(conn, &id)?;
    let serialized = serde_json::to_string(&inv).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "invoices", Some(&id), None, Some(&serialized))?;
    Ok(inv)
}

pub fn get_invoice(conn: &Connection, id: &str) -> Result<Invoice> {
    let mut invoice = conn.query_row(
        "SELECT i.id, i.invoice_number, i.customer_id, c.name, i.subtotal, i.discount, i.tax, i.total,
                i.amount_paid, i.due_amount, i.payment_status, i.payment_method, i.date, i.created_at
         FROM invoices i
         JOIN customers c ON i.customer_id = c.id
         WHERE i.id = ?1",
        params![id],
        |row| {
            Ok(Invoice {
                id: row.get(0)?,
                invoice_number: row.get(1)?,
                customer_id: row.get(2)?,
                customer_name: row.get(3)?,
                subtotal: row.get(4)?,
                discount: row.get(5)?,
                tax: row.get(6)?,
                total: row.get(7)?,
                amount_paid: row.get(8)?,
                due_amount: row.get(9)?,
                payment_status: row.get(10)?,
                payment_method: row.get(11)?,
                date: row.get(12)?,
                created_at: row.get(13)?,
                items: Vec::new(),
            })
        },
    )?;

    // Fetch items
    let mut stmt = conn.prepare(
        "SELECT ii.id, ii.invoice_id, ii.product_id, p.name, ii.quantity, ii.unit_price, ii.total, COALESCE(t.rate, 0.0) as tax_rate
         FROM invoice_items ii
         JOIN products p ON ii.product_id = p.id
         LEFT JOIN tax_rates t ON p.tax_id = t.id
         WHERE ii.invoice_id = ?1",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok(InvoiceItem {
            id: row.get(0)?,
            invoice_id: row.get(1)?,
            product_id: row.get(2)?,
            product_name: row.get(3)?,
            quantity: row.get(4)?,
            unit_price: row.get(5)?,
            total: row.get(6)?,
            tax_rate: Some(row.get(7)?),
        })
    })?;

    for r in rows {
        invoice.items.push(r?);
    }

    Ok(invoice)
}

pub fn list_invoices(conn: &Connection) -> Result<Vec<Invoice>> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.invoice_number, i.customer_id, c.name, i.subtotal, i.discount, i.tax, i.total,
                i.amount_paid, i.due_amount, i.payment_status, i.payment_method, i.date, i.created_at
         FROM invoices i
         JOIN customers c ON i.customer_id = c.id
         WHERE i.is_deleted = 0 ORDER BY i.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Invoice {
            id: row.get(0)?,
            invoice_number: row.get(1)?,
            customer_id: row.get(2)?,
            customer_name: row.get(3)?,
            subtotal: row.get(4)?,
            discount: row.get(5)?,
            tax: row.get(6)?,
            total: row.get(7)?,
            amount_paid: row.get(8)?,
            due_amount: row.get(9)?,
            payment_status: row.get(10)?,
            payment_method: row.get(11)?,
            date: row.get(12)?,
            created_at: row.get(13)?,
            items: Vec::new(),
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn create_quotation(
    conn: &Connection,
    customer_id: &str,
    total: f64,
    status: &str,
    items: Vec<QuotationItem>,
    actor_id: Option<&str>,
) -> Result<Quotation> {
    let id = Uuid::new_v4().to_string();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM quotations", [], |r| r.get(0))?;
    let quotation_number = format!("QT-{:06}", count + 1);
    let now_date = Utc::now().format("%Y-%m-%d").to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO quotations (id, quotation_number, customer_id, total, status, date, created_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![id, quotation_number, customer_id, total, status, now_date, now],
    )?;

    for item in &items {
        let item_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO quotation_items (id, quotation_id, product_id, quantity, unit_price, total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, id, item.product_id, item.quantity, item.unit_price, item.total],
        )?;
    }

    let qt = get_quotation(conn, &id)?;
    let serialized = serde_json::to_string(&qt).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "quotations", Some(&id), None, Some(&serialized))?;
    Ok(qt)
}

pub fn get_quotation(conn: &Connection, id: &str) -> Result<Quotation> {
    let mut quotation = conn.query_row(
        "SELECT q.id, q.quotation_number, q.customer_id, c.name, q.total, q.status, q.date, q.created_at
         FROM quotations q
         JOIN customers c ON q.customer_id = c.id
         WHERE q.id = ?1",
        params![id],
        |row| {
            Ok(Quotation {
                id: row.get(0)?,
                quotation_number: row.get(1)?,
                customer_id: row.get(2)?,
                customer_name: row.get(3)?,
                total: row.get(4)?,
                status: row.get(5)?,
                date: row.get(6)?,
                created_at: row.get(7)?,
                items: Vec::new(),
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT qi.id, qi.quotation_id, qi.product_id, p.name, qi.quantity, qi.unit_price, qi.total
         FROM quotation_items qi
         JOIN products p ON qi.product_id = p.id
         WHERE qi.quotation_id = ?1",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok(QuotationItem {
            id: row.get(0)?,
            quotation_id: row.get(1)?,
            product_id: row.get(2)?,
            product_name: row.get(3)?,
            quantity: row.get(4)?,
            unit_price: row.get(5)?,
            total: row.get(6)?,
        })
    })?;

    for r in rows {
        quotation.items.push(r?);
    }
    Ok(quotation)
}

pub fn list_quotations(conn: &Connection) -> Result<Vec<Quotation>> {
    let mut stmt = conn.prepare(
        "SELECT q.id, q.quotation_number, q.customer_id, c.name, q.total, q.status, q.date, q.created_at
         FROM quotations q
         JOIN customers c ON q.customer_id = c.id
         WHERE q.is_deleted = 0 ORDER BY q.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Quotation {
            id: row.get(0)?,
            quotation_number: row.get(1)?,
            customer_id: row.get(2)?,
            customer_name: row.get(3)?,
            total: row.get(4)?,
            status: row.get(5)?,
            date: row.get(6)?,
            created_at: row.get(7)?,
            items: Vec::new(),
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn create_purchase(
    conn: &Connection,
    supplier_id: &str,
    total: f64,
    status: &str,
    items: Vec<PurchaseItem>,
    actor_id: Option<&str>,
) -> Result<Purchase> {
    let id = Uuid::new_v4().to_string();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM purchases", [], |r| r.get(0))?;
    let purchase_number = format!("PO-{:06}", count + 1);
    let now_date = Utc::now().format("%Y-%m-%d").to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO purchases (id, purchase_number, supplier_id, total, status, date, created_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![id, purchase_number, supplier_id, total, status, now_date, now],
    )?;

    let is_received = status == "RECEIVED";

    // Update supplier balance
    if is_received {
        conn.execute(
            "UPDATE suppliers SET balance = balance + ?1 WHERE id = ?2",
            params![total, supplier_id],
        )?;
    }

    for item in &items {
        let item_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO purchase_items (id, purchase_id, product_id, quantity, unit_price, total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, id, item.product_id, item.quantity, item.unit_price, item.total],
        )?;

        if is_received {
            // Add inventory stock!
            conn.execute(
                "UPDATE products SET stock = stock + ?1, updated_at = ?2 WHERE id = ?3",
                params![item.quantity, now, item.product_id],
            )?;

            // Stock adjustment log
            let adj_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
                 VALUES (?1, ?2, 'ADD', ?3, ?4, ?5, ?6)",
                params![adj_id, item.product_id, item.quantity, format!("Purchase Order {}", purchase_number), actor_id.unwrap_or(""), now],
            )?;
        }
    }

    if is_received {
        // Double entry accounting:
        // Debit Inventory Asset (1400) - Total
        // Credit Accounts Payable (2000) - Total
        let journal_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![journal_id, now_date, format!("Purchase PO {}", purchase_number), purchase_number, now],
        )?;

        let inv_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1400'", [], |r| r.get(0))?;
        let ap_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '2000'", [], |r| r.get(0))?;

        // Debit Inventory
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &inv_acct_id, total),
        )?;
        // Credit AP
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, 0.0, ?4)",
            (Uuid::new_v4().to_string(), &journal_id, &ap_acct_id, total),
        )?;
    }

    let po = get_purchase(conn, &id)?;
    let serialized = serde_json::to_string(&po).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "purchases", Some(&id), None, Some(&serialized))?;
    Ok(po)
}

pub fn get_purchase(conn: &Connection, id: &str) -> Result<Purchase> {
    let mut po = conn.query_row(
        "SELECT p.id, p.purchase_number, p.supplier_id, s.name, p.total, p.status, p.date, p.created_at
         FROM purchases p
         JOIN suppliers s ON p.supplier_id = s.id
         WHERE p.id = ?1",
        params![id],
        |row| {
            Ok(Purchase {
                id: row.get(0)?,
                purchase_number: row.get(1)?,
                supplier_id: row.get(2)?,
                supplier_name: row.get(3)?,
                total: row.get(4)?,
                status: row.get(5)?,
                date: row.get(6)?,
                created_at: row.get(7)?,
                items: Vec::new(),
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT pi.id, pi.purchase_id, pi.product_id, pr.name, pi.quantity, pi.unit_price, pi.total
         FROM purchase_items pi
         JOIN products pr ON pi.product_id = pr.id
         WHERE pi.purchase_id = ?1",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok(PurchaseItem {
            id: row.get(0)?,
            purchase_id: row.get(1)?,
            product_id: row.get(2)?,
            product_name: row.get(3)?,
            quantity: row.get(4)?,
            unit_price: row.get(5)?,
            total: row.get(6)?,
        })
    })?;

    for r in rows {
        po.items.push(r?);
    }
    Ok(po)
}

pub fn list_purchases(conn: &Connection) -> Result<Vec<Purchase>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.purchase_number, p.supplier_id, s.name, p.total, p.status, p.date, p.created_at
         FROM purchases p
         JOIN suppliers s ON p.supplier_id = s.id
         WHERE p.is_deleted = 0 ORDER BY p.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Purchase {
            id: row.get(0)?,
            purchase_number: row.get(1)?,
            supplier_id: row.get(2)?,
            supplier_name: row.get(3)?,
            total: row.get(4)?,
            status: row.get(5)?,
            date: row.get(6)?,
            created_at: row.get(7)?,
            items: Vec::new(),
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

// ==========================================
// ACCOUNTING: EXPENSES, PAYMENTS, JOURNAL ENTRIES
// ==========================================
pub fn create_expense(
    conn: &Connection,
    title: &str,
    category: &str,
    amount: f64,
    date: &str,
    description: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Expense> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO expenses (id, title, category, amount, date, description, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, title, category, amount, date, description, now],
    )?;

    // Double entry accounting:
    // Debit Rent Expense or Utility Expense etc. (5100/5200)
    // Credit Cash on Hand (1000)
    let journal_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
         VALUES (?1, ?2, ?3, 'EXPENSE', ?4)",
        params![journal_id, date, format!("Expense: {}", title), now],
    )?;

    let exp_code = if category.to_lowercase().contains("rent") { "5100" } else { "5200" };
    let exp_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = ?1", params![exp_code], |r| r.get(0))?;
    let cash_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1000'", [], |r| r.get(0))?;

    // Debit Expense
    conn.execute(
        "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
         VALUES (?1, ?2, ?3, ?4, 0.0)",
        (Uuid::new_v4().to_string(), &journal_id, &exp_acct_id, amount),
    )?;
    // Credit Cash
    conn.execute(
        "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
         VALUES (?1, ?2, ?3, 0.0, ?4)",
        (Uuid::new_v4().to_string(), &journal_id, &cash_acct_id, amount),
    )?;

    let exp = Expense {
        id: id.clone(),
        title: title.to_string(),
        category: category.to_string(),
        amount,
        date: date.to_string(),
        description: description.map(String::from),
        created_at: now,
    };
    let serialized = serde_json::to_string(&exp).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "expenses", Some(&id), None, Some(&serialized))?;
    Ok(exp)
}

pub fn list_expenses(conn: &Connection) -> Result<Vec<Expense>> {
    let mut stmt = conn.prepare("SELECT id, title, category, amount, date, description, created_at FROM expenses ORDER BY date DESC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Expense {
            id: row.get(0)?,
            title: row.get(1)?,
            category: row.get(2)?,
            amount: row.get(3)?,
            date: row.get(4)?,
            description: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_expense(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("DELETE FROM expenses WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "expenses", Some(id), None, None)?;
    Ok(())
}

pub fn create_payment(
    conn: &Connection,
    payment_type: &str, // 'RECEIVE' (Customer due payment) or 'PAY' (Supplier due payment)
    entity_id: &str,
    amount: f64,
    payment_method: &str,
    reference_no: Option<&str>,
    actor_id: Option<&str>,
) -> Result<Payment> {
    let id = Uuid::new_v4().to_string();
    let now_date = Utc::now().format("%Y-%m-%d").to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO payments (id, type, entity_id, amount, payment_method, reference_no, date, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, payment_type, entity_id, amount, payment_method, reference_no, now_date, now],
    )?;

    // Update customer due amount or supplier balance
    if payment_type == "RECEIVE" {
        conn.execute(
            "UPDATE customers SET due_amount = due_amount - ?1 WHERE id = ?2",
            params![amount, entity_id],
        )?;

        // Double entry accounting:
        // Debit Cash/Bank (1000)
        // Credit Accounts Receivable (1200)
        let journal_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
             VALUES (?1, ?2, 'Customer payment received', 'PAYMENT', ?3)",
            params![journal_id, now_date, now],
        )?;
        
        let cash_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1000'", [], |r| r.get(0))?;
        let ar_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1200'", [], |r| r.get(0))?;

        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &cash_acct_id, amount),
        )?;
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, 0.0, ?4)",
            (Uuid::new_v4().to_string(), &journal_id, &ar_acct_id, amount),
        )?;
    } else {
        conn.execute(
            "UPDATE suppliers SET balance = balance - ?1 WHERE id = ?2",
            params![amount, entity_id],
        )?;

        // Double entry accounting:
        // Debit Accounts Payable (2000)
        // Credit Cash/Bank (1000)
        let journal_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
             VALUES (?1, ?2, 'Supplier payment made', 'PAYMENT', ?3)",
            params![journal_id, now_date, now],
        )?;
        
        let cash_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1000'", [], |r| r.get(0))?;
        let ap_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '2000'", [], |r| r.get(0))?;

        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &ap_acct_id, amount),
        )?;
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, 0.0, ?4)",
            (Uuid::new_v4().to_string(), &journal_id, &cash_acct_id, amount),
        )?;
    }

    let entity_name: Option<String> = if payment_type == "RECEIVE" {
        conn.query_row("SELECT name FROM customers WHERE id = ?1", params![entity_id], |r| r.get(0)).ok()
    } else {
        conn.query_row("SELECT name FROM suppliers WHERE id = ?1", params![entity_id], |r| r.get(0)).ok()
    };

    let p = Payment {
        id: id.clone(),
        payment_type: payment_type.to_string(),
        entity_id: entity_id.to_string(),
        entity_name,
        amount,
        payment_method: payment_method.to_string(),
        reference_no: reference_no.map(String::from),
        date: now_date,
        created_at: now,
    };
    let serialized = serde_json::to_string(&p).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "payments", Some(&id), None, Some(&serialized))?;
    Ok(p)
}

pub fn list_payments(conn: &Connection) -> Result<Vec<Payment>> {
    let mut stmt = conn.prepare("SELECT id, type, entity_id, amount, payment_method, reference_no, date, created_at FROM payments ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], |row| {
        let p_type: String = row.get(1)?;
        let entity_id: String = row.get(2)?;
        
        let entity_name: Option<String> = if p_type == "RECEIVE" {
            conn.query_row("SELECT name FROM customers WHERE id = ?1", params![entity_id], |r| r.get(0)).ok()
        } else {
            conn.query_row("SELECT name FROM suppliers WHERE id = ?1", params![entity_id], |r| r.get(0)).ok()
        };

        Ok(Payment {
            id: row.get(0)?,
            payment_type: p_type,
            entity_id,
            entity_name,
            amount: row.get(3)?,
            payment_method: row.get(4)?,
            reference_no: row.get(5)?,
            date: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn list_chart_of_accounts(conn: &Connection) -> Result<Vec<ChartOfAccount>> {
    let mut stmt = conn.prepare("SELECT id, code, name, type, parent_id, is_active FROM chart_of_accounts WHERE is_active = 1 ORDER BY code ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(ChartOfAccount {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
            account_type: row.get(3)?,
            parent_id: row.get(4)?,
            is_active: row.get(5)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn create_chart_of_account(
    conn: &Connection,
    code: &str,
    name: &str,
    account_type: &str,
    parent_id: Option<&str>,
    actor_id: Option<&str>,
) -> Result<ChartOfAccount> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO chart_of_accounts (id, code, name, type, parent_id, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        params![id, code, name, account_type, parent_id],
    )?;

    let acct = ChartOfAccount {
        id: id.clone(),
        code: code.to_string(),
        name: name.to_string(),
        account_type: account_type.to_string(),
        parent_id: parent_id.map(String::from),
        is_active: 1,
    };
    let serialized = serde_json::to_string(&acct).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "chart_of_accounts", Some(&id), None, Some(&serialized))?;
    Ok(acct)
}

pub fn create_journal_entry(
    conn: &Connection,
    entry_date: &str,
    description: &str,
    reference: Option<&str>,
    lines: Vec<JournalLine>,
    actor_id: Option<&str>,
) -> Result<JournalEntry> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Verify debits match credits
    let mut total_debit = 0.0;
    let mut total_credit = 0.0;
    for l in &lines {
        total_debit += l.debit;
        total_credit += l.credit;
    }

    if (total_debit - total_credit).abs() > 0.001 {
        return Err(rusqlite::Error::InvalidQuery); // Debits must match credits!
    }

    conn.execute(
        "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, entry_date, description, reference, now],
    )?;

    for line in &lines {
        let line_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![line_id, id, line.account_id, line.debit, line.credit],
        )?;
    }

    let je = get_journal_entry(conn, &id)?;
    let serialized = serde_json::to_string(&je).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "journal_entries", Some(&id), None, Some(&serialized))?;
    Ok(je)
}

pub fn get_journal_entry(conn: &Connection, id: &str) -> Result<JournalEntry> {
    let mut je = conn.query_row(
        "SELECT id, entry_date, description, reference, created_at FROM journal_entries WHERE id = ?1",
        params![id],
        |row| {
            Ok(JournalEntry {
                id: row.get(0)?,
                entry_date: row.get(1)?,
                description: row.get(2)?,
                reference: row.get(3)?,
                created_at: row.get(4)?,
                lines: Vec::new(),
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT jl.id, jl.journal_entry_id, jl.account_id, coa.name, coa.code, jl.debit, jl.credit
         FROM journal_lines jl
         JOIN chart_of_accounts coa ON jl.account_id = coa.id
         WHERE jl.journal_entry_id = ?1",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok(JournalLine {
            id: row.get(0)?,
            journal_entry_id: row.get(1)?,
            account_id: row.get(2)?,
            account_name: row.get(3)?,
            account_code: row.get(4)?,
            debit: row.get(5)?,
            credit: row.get(6)?,
        })
    })?;

    for r in rows {
        je.lines.push(r?);
    }
    Ok(je)
}

pub fn list_journal_entries(conn: &Connection) -> Result<Vec<JournalEntry>> {
    let mut stmt = conn.prepare("SELECT id, entry_date, description, reference, created_at FROM journal_entries ORDER BY entry_date DESC")?;
    let rows = stmt.query_map([], |row| {
        Ok(JournalEntry {
            id: row.get(0)?,
            entry_date: row.get(1)?,
            description: row.get(2)?,
            reference: row.get(3)?,
            created_at: row.get(4)?,
            lines: Vec::new(),
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        let mut je = r?;
        let mut stmt2 = conn.prepare(
            "SELECT jl.id, jl.journal_entry_id, jl.account_id, coa.name, coa.code, jl.debit, jl.credit
             FROM journal_lines jl
             JOIN chart_of_accounts coa ON jl.account_id = coa.id
             WHERE jl.journal_entry_id = ?1",
        )?;
        let lines_rows = stmt2.query_map(params![je.id], |row| {
            Ok(JournalLine {
                id: row.get(0)?,
                journal_entry_id: row.get(1)?,
                account_id: row.get(2)?,
                account_name: row.get(3)?,
                account_code: row.get(4)?,
                debit: row.get(5)?,
                credit: row.get(6)?,
            })
        })?;
        for lr in lines_rows {
            je.lines.push(lr?);
        }
        list.push(je);
    }
    Ok(list)
}

// ==========================================
// REPORTS CALCULATORS
// ==========================================
pub fn calculate_dashboard_stats(conn: &Connection) -> Result<DashboardStats> {
    let today_str = Utc::now().format("%Y-%m-%d").to_string();
    let month_str = Utc::now().format("%Y-%m").to_string() + "%";

    // Today Sales
    let today_sales: f64 = conn.query_row(
        "SELECT COALESCE(SUM(total), 0.0) FROM invoices WHERE date = ?1 AND is_deleted = 0",
        params![today_str],
        |r| r.get(0),
    ).unwrap_or(0.0);

    // Monthly Sales
    let monthly_sales: f64 = conn.query_row(
        "SELECT COALESCE(SUM(total), 0.0) FROM invoices WHERE date LIKE ?1 AND is_deleted = 0",
        params![month_str],
        |r| r.get(0),
    ).unwrap_or(0.0);

    // Revenue (Total sales)
    let revenue: f64 = conn.query_row(
        "SELECT COALESCE(SUM(total), 0.0) FROM invoices WHERE is_deleted = 0",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    // Expenses (Total expenses)
    let expenses: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0.0) FROM expenses",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    // Stock value (Sum of cost * stock)
    let stock_value: f64 = conn.query_row(
        "SELECT COALESCE(SUM(cost * stock), 0.0) FROM products WHERE is_deleted = 0",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    // Low stock count (Products under reorder_level)
    let low_stock_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM products WHERE stock <= reorder_level AND is_deleted = 0",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    // Profit calculation: Revenue - COGS - Expenses.
    // COGS = sum of invoice_item.quantity * product.cost
    let cogs: f64 = conn.query_row(
        "SELECT COALESCE(SUM(ii.quantity * p.cost), 0.0)
         FROM invoice_items ii
         JOIN products p ON ii.product_id = p.id
         JOIN invoices i ON ii.invoice_id = i.id
         WHERE i.is_deleted = 0",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    let profit = revenue - cogs - expenses;

    Ok(DashboardStats {
        today_sales,
        monthly_sales,
        revenue,
        profit,
        expenses,
        stock_value,
        low_stock_count,
    })
}

pub fn get_sales_chart_points(conn: &Connection) -> Result<Vec<ChartPoint>> {
    // Group invoices by date for the last 15 days
    let mut stmt = conn.prepare(
        "SELECT date, SUM(total)
         FROM invoices
         WHERE is_deleted = 0
         GROUP BY date
         ORDER BY date DESC
         LIMIT 15",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ChartPoint {
            label: row.get(0)?,
            value: row.get(1)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    // Reverse to display chronologically
    list.reverse();
    Ok(list)
}

// TAX SUMMARY REPORT
pub fn get_tax_summary(conn: &Connection) -> Result<Vec<ChartPoint>> {
    // Calculates total tax collected grouped by category/rate
    // For simplicity, let's group by invoice date
    let mut stmt = conn.prepare(
        "SELECT date, SUM(tax)
         FROM invoices
         WHERE is_deleted = 0
         GROUP BY date
         ORDER BY date DESC
         LIMIT 15",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ChartPoint {
            label: row.get(0)?,
            value: row.get(1)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    list.reverse();
    Ok(list)
}

// TAX RATES RULES
pub fn list_tax_rates(conn: &Connection) -> Result<Vec<TaxRate>> {
    let mut stmt = conn.prepare("SELECT id, name, rate, is_active, created_at FROM tax_rates WHERE is_active = 1")?;
    let rows = stmt.query_map([], |row| {
        Ok(TaxRate {
            id: row.get(0)?,
            name: row.get(1)?,
            rate: row.get(2)?,
            is_active: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn create_tax_rate(conn: &Connection, name: &str, rate: f64, actor_id: Option<&str>) -> Result<TaxRate> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tax_rates (id, name, rate, is_active, created_at) VALUES (?1, ?2, ?3, 1, ?4)",
        params![id, name, rate, now],
    )?;

    let tr = TaxRate { id: id.clone(), name: name.to_string(), rate, is_active: 1, created_at: now };
    let serialized = serde_json::to_string(&tr).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "tax_rates", Some(&id), None, Some(&serialized))?;
    Ok(tr)
}

pub fn receive_purchase(
    conn: &Connection,
    purchase_id: &str,
    actor_id: Option<&str>,
) -> Result<Purchase> {
    let po = get_purchase(conn, purchase_id)?;
    if po.status == "RECEIVED" {
        return Err(rusqlite::Error::InvalidQuery);
    }

    let now_date = Utc::now().format("%Y-%m-%d").to_string();
    let now = Utc::now().to_rfc3339();

    // Update status
    conn.execute(
        "UPDATE purchases SET status = 'RECEIVED' WHERE id = ?1",
        params![purchase_id],
    )?;

    // Update supplier balance
    conn.execute(
        "UPDATE suppliers SET balance = balance + ?1 WHERE id = ?2",
        params![po.total, po.supplier_id],
    )?;

    // Update product stock and log adjustments
    for item in &po.items {
        let prod = get_product_by_id(conn, &item.product_id)?;
        let current_stock = if prod.stock < 0.0 { 0.0 } else { prod.stock };
        let current_cost = prod.cost;
        let received_qty = item.quantity;
        let purchase_price = item.unit_price;
        let total_qty = current_stock + received_qty;
        let new_cost = if total_qty > 0.0 {
            ((current_stock * current_cost) + (received_qty * purchase_price)) / total_qty
        } else {
            purchase_price
        };

        conn.execute(
            "UPDATE products SET stock = stock + ?1, cost = ?2, updated_at = ?3 WHERE id = ?4",
            params![received_qty, new_cost, now, item.product_id],
        )?;

        let adj_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
             VALUES (?1, ?2, 'ADD', ?3, ?4, ?5, ?6)",
            params![adj_id, item.product_id, item.quantity, format!("Purchase Order {}", po.purchase_number), actor_id.unwrap_or(""), now],
        )?;
    }

    // Double entry accounting
    let journal_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![journal_id, now_date, format!("Purchase PO {}", po.purchase_number), po.purchase_number, now],
    )?;

    let inv_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1400'", [], |r| r.get(0))?;
    let ap_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '2000'", [], |r| r.get(0))?;

    // Debit Inventory
    conn.execute(
        "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
         VALUES (?1, ?2, ?3, ?4, 0.0)",
        (Uuid::new_v4().to_string(), &journal_id, &inv_acct_id, po.total),
    )?;
    // Credit AP
    conn.execute(
        "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
         VALUES (?1, ?2, ?3, 0.0, ?4)",
        (Uuid::new_v4().to_string(), &journal_id, &ap_acct_id, po.total),
    )?;

    let updated_po = get_purchase(conn, purchase_id)?;
    let old_serialized = serde_json::to_string(&po).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated_po).unwrap_or_default();
    log_audit(conn, actor_id, "UPDATE_STATUS", "purchases", Some(purchase_id), Some(&old_serialized), Some(&new_serialized))?;

    Ok(updated_po)
}

pub fn update_invoice(
    conn: &Connection,
    id: &str,
    customer_id: &str,
    subtotal: f64,
    discount: f64,
    tax: f64,
    total: f64,
    amount_paid: f64,
    payment_method: Option<&str>,
    items: Vec<InvoiceItem>,
    actor_id: Option<&str>,
) -> Result<Invoice> {
    let old_invoice = get_invoice(conn, id)?;
    if old_invoice.payment_status == "Cancel" {
        return Err(rusqlite::Error::InvalidQuery);
    }
    
    let now = Utc::now().to_rfc3339();
    let now_date = Utc::now().format("%Y-%m-%d").to_string();

    // 1. Revert stock adjustments & stock counts from old items
    for old_item in &old_invoice.items {
        conn.execute(
            "UPDATE products SET stock = stock + ?1, updated_at = ?2 WHERE id = ?3",
            params![old_item.quantity, now, old_item.product_id],
        )?;

        let adj_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
             VALUES (?1, ?2, 'ADD', ?3, ?4, ?5, ?6)",
            params![adj_id, old_item.product_id, old_item.quantity, format!("Revert Invoice {} Edit", old_invoice.invoice_number), actor_id.unwrap_or(""), now],
        )?;
    }

    // 2. Revert customer due amount
    conn.execute(
        "UPDATE customers SET due_amount = due_amount - ?1 WHERE id = ?2",
        params![old_invoice.due_amount, old_invoice.customer_id],
    )?;

    // 3. Delete old journal entries
    let journal_id_opt: Option<String> = conn.query_row(
        "SELECT id FROM journal_entries WHERE reference = ?1",
        params![old_invoice.invoice_number],
        |r| r.get(0),
    ).ok();

    if let Some(j_id) = journal_id_opt {
        conn.execute("DELETE FROM journal_lines WHERE journal_entry_id = ?1", params![j_id])?;
        conn.execute("DELETE FROM journal_entries WHERE id = ?1", params![j_id])?;
    }

    // 4. Update invoice header
    let due_amount = total - amount_paid;
    let payment_status = if due_amount <= 0.0 {
        "Paid"
    } else if amount_paid > 0.0 {
        "Partial"
    } else {
        "Unpaid"
    };

    conn.execute(
        "UPDATE invoices SET customer_id = ?1, subtotal = ?2, discount = ?3, tax = ?4, total = ?5,
                             amount_paid = ?6, due_amount = ?7, payment_status = ?8, payment_method = ?9
                             WHERE id = ?10",
        params![customer_id, subtotal, discount, tax, total, amount_paid, due_amount, payment_status, payment_method, id],
    )?;

    // 5. Delete old items
    conn.execute("DELETE FROM invoice_items WHERE invoice_id = ?1", params![id])?;

    // 6. Insert new items, subtract stock, log adjustments
    for item in &items {
        let item_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO invoice_items (id, invoice_id, product_id, quantity, unit_price, total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, id, item.product_id, item.quantity, item.unit_price, item.total],
        )?;

        conn.execute(
            "UPDATE products SET stock = stock - ?1, updated_at = ?2 WHERE id = ?3",
            params![item.quantity, now, item.product_id],
        )?;

        let adj_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO stock_adjustments (id, product_id, adjustment_type, quantity, reason, adjusted_by, created_at)
             VALUES (?1, ?2, 'SUBTRACT', ?3, ?4, ?5, ?6)",
            params![adj_id, item.product_id, item.quantity, format!("Invoice {}", old_invoice.invoice_number), actor_id.unwrap_or(""), now],
        )?;
    }

    // 7. Update new customer due amount
    conn.execute(
        "UPDATE customers SET due_amount = due_amount + ?1 WHERE id = ?2",
        params![due_amount, customer_id],
    )?;

    // 8. Create new journal entries
    let journal_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO journal_entries (id, entry_date, description, reference, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![journal_id, now_date, format!("Sale invoice {}", old_invoice.invoice_number), old_invoice.invoice_number, now],
    )?;

    let ar_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1200'", [], |r| r.get(0))?;
    let sales_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '4000'", [], |r| r.get(0))?;
    let cash_acct_id: String = conn.query_row("SELECT id FROM chart_of_accounts WHERE code = '1000'", [], |r| r.get(0))?;

    if due_amount > 0.0 {
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &ar_acct_id, due_amount),
        )?;
    }
    if amount_paid > 0.0 {
        conn.execute(
            "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
             VALUES (?1, ?2, ?3, ?4, 0.0)",
            (Uuid::new_v4().to_string(), &journal_id, &cash_acct_id, amount_paid),
        )?;
    }
    conn.execute(
        "INSERT INTO journal_lines (id, journal_entry_id, account_id, debit, credit)
         VALUES (?1, ?2, ?3, 0.0, ?4)",
        (Uuid::new_v4().to_string(), &journal_id, &sales_acct_id, total),
    )?;

    let updated = get_invoice(conn, id)?;
    let old_serialized = serde_json::to_string(&old_invoice).unwrap_or_default();
    let new_serialized = serde_json::to_string(&updated).unwrap_or_default();
    log_audit(conn, actor_id, "UPDATE", "invoices", Some(id), Some(&old_serialized), Some(&new_serialized))?;

    Ok(updated)
}

pub fn list_customers_filtered(
    conn: &Connection,
    search: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Customer>, i64)> {
    let mut query = String::from("FROM customers WHERE is_deleted = 0");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (name LIKE ?{} OR email LIKE ?{} OR phone LIKE ?{} OR address LIKE ?{})", param_index, param_index + 1, param_index + 2, param_index + 3));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 4;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("name") {
        "name" => "name",
        "due_amount" => "due_amount",
        "created_at" => "created_at",
        _ => "name",
    };

    let order = match sort_order.unwrap_or("ASC") {
        "DESC" => "DESC",
        _ => "ASC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!("SELECT id, name, email, phone, address, due_amount, created_at, updated_at {}", query);
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Customer {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            phone: row.get(3)?,
            address: row.get(4)?,
            due_amount: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut customers = Vec::new();
    for r in rows {
        customers.push(r?);
    }
    Ok((customers, total_count))
}

pub fn list_suppliers_filtered(
    conn: &Connection,
    search: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Supplier>, i64)> {
    let mut query = String::from("FROM suppliers WHERE is_deleted = 0");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (name LIKE ?{} OR email LIKE ?{} OR phone LIKE ?{} OR address LIKE ?{})", param_index, param_index + 1, param_index + 2, param_index + 3));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 4;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("name") {
        "name" => "name",
        "balance" => "balance",
        "created_at" => "created_at",
        _ => "name",
    };

    let order = match sort_order.unwrap_or("ASC") {
        "DESC" => "DESC",
        _ => "ASC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!("SELECT id, name, email, phone, address, balance, created_at, updated_at {}", query);
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Supplier {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
            phone: row.get(3)?,
            address: row.get(4)?,
            balance: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut suppliers = Vec::new();
    for r in rows {
        suppliers.push(r?);
    }
    Ok((suppliers, total_count))
}

pub fn list_leads_filtered(
    conn: &Connection,
    search: Option<&str>,
    stage: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Lead>, i64)> {
    let mut query = String::from("FROM leads WHERE is_deleted = 0");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (name LIKE ?{} OR contact_name LIKE ?{} OR phone LIKE ?{} OR email LIKE ?{} OR notes LIKE ?{})", param_index, param_index + 1, param_index + 2, param_index + 3, param_index + 4));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 5;
        }
    }

    if let Some(stg) = stage {
        if !stg.is_empty() {
            query.push_str(&format!(" AND stage = ?{}", param_index));
            params_vec.push(Box::new(stg.to_string()));
            param_index += 1;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("created_at") {
        "name" => "name",
        "deal_value" => "deal_value",
        "created_at" => "created_at",
        _ => "created_at",
    };

    let order = match sort_order.unwrap_or("DESC") {
        "ASC" => "ASC",
        _ => "DESC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!("SELECT id, name, contact_name, phone, email, stage, deal_value, notes, created_at, updated_at {}", query);
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Lead {
            id: row.get(0)?,
            name: row.get(1)?,
            contact_name: row.get(2)?,
            phone: row.get(3)?,
            email: row.get(4)?,
            stage: row.get(5)?,
            deal_value: row.get(6)?,
            notes: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?;

    let mut leads = Vec::new();
    for r in rows {
        leads.push(r?);
    }
    Ok((leads, total_count))
}

pub fn list_invoices_filtered(
    conn: &Connection,
    search: Option<&str>,
    payment_status: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Invoice>, i64)> {
    let mut query = String::from(
        "FROM invoices i
         JOIN customers c ON i.customer_id = c.id
         WHERE i.is_deleted = 0"
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (c.name LIKE ?{} OR i.invoice_number LIKE ?{})", param_index, param_index + 1));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 2;
        }
    }

    if let Some(status) = payment_status {
        if !status.is_empty() {
            query.push_str(&format!(" AND i.payment_status = ?{}", param_index));
            params_vec.push(Box::new(status.to_string()));
            param_index += 1;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("created_at") {
        "created_at" => "i.created_at",
        "total" => "i.total",
        "due_amount" => "i.due_amount",
        _ => "i.created_at",
    };

    let order = match sort_order.unwrap_or("DESC") {
        "ASC" => "ASC",
        _ => "DESC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!(
        "SELECT i.id, i.invoice_number, i.customer_id, c.name, i.subtotal, i.discount, i.tax, i.total,
                i.amount_paid, i.due_amount, i.payment_status, i.payment_method, i.date, i.created_at {}",
        query
    );
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Invoice {
            id: row.get(0)?,
            invoice_number: row.get(1)?,
            customer_id: row.get(2)?,
            customer_name: row.get(3)?,
            subtotal: row.get(4)?,
            discount: row.get(5)?,
            tax: row.get(6)?,
            total: row.get(7)?,
            amount_paid: row.get(8)?,
            due_amount: row.get(9)?,
            payment_status: row.get(10)?,
            payment_method: row.get(11)?,
            date: row.get(12)?,
            created_at: row.get(13)?,
            items: Vec::new(),
        })
    })?;

    let mut invoices = Vec::new();
    for r in rows {
        invoices.push(r?);
    }
    Ok((invoices, total_count))
}

pub fn list_quotations_filtered(
    conn: &Connection,
    search: Option<&str>,
    status: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Quotation>, i64)> {
    let mut query = String::from(
        "FROM quotations q
         JOIN customers c ON q.customer_id = c.id
         WHERE q.is_deleted = 0"
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (c.name LIKE ?{} OR q.quotation_number LIKE ?{})", param_index, param_index + 1));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 2;
        }
    }

    if let Some(st) = status {
        if !st.is_empty() {
            query.push_str(&format!(" AND q.status = ?{}", param_index));
            params_vec.push(Box::new(st.to_string()));
            param_index += 1;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("created_at") {
        "created_at" => "q.created_at",
        "total" => "q.total",
        _ => "q.created_at",
    };

    let order = match sort_order.unwrap_or("DESC") {
        "ASC" => "ASC",
        _ => "DESC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!(
        "SELECT q.id, q.quotation_number, q.customer_id, c.name, q.total, q.status, q.date, q.created_at {}",
        query
    );
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Quotation {
            id: row.get(0)?,
            quotation_number: row.get(1)?,
            customer_id: row.get(2)?,
            customer_name: row.get(3)?,
            total: row.get(4)?,
            status: row.get(5)?,
            date: row.get(6)?,
            created_at: row.get(7)?,
            items: Vec::new(),
        })
    })?;

    let mut quotations = Vec::new();
    for r in rows {
        quotations.push(r?);
    }
    Ok((quotations, total_count))
}

pub fn list_purchases_filtered(
    conn: &Connection,
    search: Option<&str>,
    status: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Purchase>, i64)> {
    let mut query = String::from(
        "FROM purchases p
         JOIN suppliers s ON p.supplier_id = s.id
         WHERE p.is_deleted = 0"
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (s.name LIKE ?{} OR p.purchase_number LIKE ?{})", param_index, param_index + 1));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 2;
        }
    }

    if let Some(st) = status {
        if !st.is_empty() {
            query.push_str(&format!(" AND p.status = ?{}", param_index));
            params_vec.push(Box::new(st.to_string()));
            param_index += 1;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("created_at") {
        "created_at" => "p.created_at",
        "total" => "p.total",
        _ => "p.created_at",
    };

    let order = match sort_order.unwrap_or("DESC") {
        "ASC" => "ASC",
        _ => "DESC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!(
        "SELECT p.id, p.purchase_number, p.supplier_id, s.name, p.total, p.status, p.date, p.created_at {}",
        query
    );
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Purchase {
            id: row.get(0)?,
            purchase_number: row.get(1)?,
            supplier_id: row.get(2)?,
            supplier_name: row.get(3)?,
            total: row.get(4)?,
            status: row.get(5)?,
            date: row.get(6)?,
            created_at: row.get(7)?,
            items: Vec::new(),
        })
    })?;

    let mut purchases = Vec::new();
    for r in rows {
        purchases.push(r?);
    }
    Ok((purchases, total_count))
}

pub fn list_expenses_filtered(
    conn: &Connection,
    search: Option<&str>,
    category: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Expense>, i64)> {
    let mut query = String::from("FROM expenses WHERE 1=1");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (title LIKE ?{} OR description LIKE ?{})", param_index, param_index + 1));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 2;
        }
    }

    if let Some(cat) = category {
        if !cat.is_empty() {
            query.push_str(&format!(" AND category = ?{}", param_index));
            params_vec.push(Box::new(cat.to_string()));
            param_index += 1;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("date") {
        "date" => "date",
        "amount" => "amount",
        _ => "date",
    };

    let order = match sort_order.unwrap_or("DESC") {
        "ASC" => "ASC",
        _ => "DESC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!("SELECT id, title, category, amount, date, description, created_at {}", query);
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Expense {
            id: row.get(0)?,
            title: row.get(1)?,
            category: row.get(2)?,
            amount: row.get(3)?,
            date: row.get(4)?,
            description: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;

    let mut expenses = Vec::new();
    for r in rows {
        expenses.push(r?);
    }
    Ok((expenses, total_count))
}

pub fn list_payments_filtered(
    conn: &Connection,
    search: Option<&str>,
    payment_type: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    page: i32,
    limit: i32,
) -> Result<(Vec<Payment>, i64)> {
    let mut query = String::from(
        "FROM payments p
         LEFT JOIN customers c ON p.entity_id = c.id
         LEFT JOIN suppliers s ON p.entity_id = s.id
         WHERE 1=1"
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_index = 1;

    if let Some(s) = search {
        if !s.trim().is_empty() {
            query.push_str(&format!(" AND (c.name LIKE ?{} OR s.name LIKE ?{} OR p.reference_no LIKE ?{})", param_index, param_index + 1, param_index + 2));
            let s_pattern = format!("%{}%", s);
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern.clone()));
            params_vec.push(Box::new(s_pattern));
            param_index += 3;
        }
    }

    if let Some(pt) = payment_type {
        if !pt.is_empty() {
            query.push_str(&format!(" AND p.type = ?{}", param_index));
            params_vec.push(Box::new(pt.to_string()));
            param_index += 1;
        }
    }

    let count_query = format!("SELECT COUNT(*) {}", query);
    let mut stmt_count = conn.prepare(&count_query)?;
    let count_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let total_count: i64 = stmt_count.query_row(&count_params[..], |row| row.get(0))?;

    let sort_col = match sort_by.unwrap_or("created_at") {
        "created_at" => "p.created_at",
        "amount" => "p.amount",
        _ => "p.created_at",
    };

    let order = match sort_order.unwrap_or("DESC") {
        "ASC" => "ASC",
        _ => "DESC",
    };

    query.push_str(&format!(" ORDER BY {} {}", sort_col, order));
    let offset = (page - 1) * limit;
    query.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_index, param_index + 1));
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let select_query = format!(
        "SELECT p.id, p.type, p.entity_id, COALESCE(c.name, s.name, 'Unknown') as entity_name,
                p.amount, p.payment_method, p.reference_no, p.date, p.created_at {}",
        query
    );
    let mut stmt_select = conn.prepare(&select_query)?;
    let select_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt_select.query_map(&select_params[..], |row| {
        Ok(Payment {
            id: row.get(0)?,
            payment_type: row.get(1)?,
            entity_id: row.get(2)?,
            entity_name: row.get(3)?,
            amount: row.get(4)?,
            payment_method: row.get(5)?,
            reference_no: row.get(6)?,
            date: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;

    let mut payments = Vec::new();
    for r in rows {
        payments.push(r?);
    }
    Ok((payments, total_count))
}

pub fn create_unit(conn: &Connection, name: &str, short_name: &str, actor_id: Option<&str>) -> Result<Unit> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO units (id, name, short_name, created_at, is_deleted)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![id, name, short_name, now],
    )?;

    let u = Unit {
        id: id.clone(),
        name: name.to_string(),
        short_name: short_name.to_string(),
        created_at: now,
    };
    let serialized = serde_json::to_string(&u).unwrap_or_default();
    log_audit(conn, actor_id, "CREATE", "units", Some(&id), None, Some(&serialized))?;
    Ok(u)
}

pub fn list_units(conn: &Connection) -> Result<Vec<Unit>> {
    let mut stmt = conn.prepare("SELECT id, name, short_name, created_at FROM units WHERE is_deleted = 0 ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Unit {
            id: row.get(0)?,
            name: row.get(1)?,
            short_name: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn delete_unit(conn: &Connection, id: &str, actor_id: Option<&str>) -> Result<()> {
    conn.execute("UPDATE units SET is_deleted = 1 WHERE id = ?1", params![id])?;
    log_audit(conn, actor_id, "DELETE", "units", Some(id), None, None)?;
    Ok(())
}

pub fn get_product_transactions(conn: &Connection, product_id: &str) -> Result<Vec<ProductTransaction>> {
    let mut stmt = conn.prepare(
        "SELECT 
            i.created_at as tx_date,
            'SALE' as tx_type,
            i.invoice_number as reference,
            -ii.quantity as quantity,
            ii.unit_price as price,
            NULL as performed_by
         FROM invoice_items ii
         JOIN invoices i ON ii.invoice_id = i.id
         WHERE ii.product_id = ?1

         UNION ALL

         SELECT 
            p.created_at as tx_date,
            'PURCHASE' as tx_type,
            p.purchase_number as reference,
            pi.quantity as quantity,
            pi.unit_price as price,
            NULL as performed_by
         FROM purchase_items pi
         JOIN purchases p ON pi.purchase_id = p.id
         WHERE pi.product_id = ?1 AND p.status = 'RECEIVED'

         UNION ALL

         SELECT 
            s.created_at as tx_date,
            'ADJUSTMENT' as tx_type,
            COALESCE(s.reason, s.adjustment_type) as reference,
            CASE 
                WHEN s.adjustment_type = 'SUBTRACT' THEN -s.quantity
                ELSE s.quantity 
            END as quantity,
            0.0 as price,
            u.name as performed_by
         FROM stock_adjustments s
         LEFT JOIN users u ON s.adjusted_by = u.id
         WHERE s.product_id = ?1

         ORDER BY tx_date DESC"
    )?;

    let rows = stmt.query_map(params![product_id], |row| {
        Ok(ProductTransaction {
            date: row.get(0)?,
            tx_type: row.get(1)?,
            reference: row.get(2)?,
            quantity: row.get(3)?,
            price: row.get(4)?,
            performed_by: row.get(5)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}
