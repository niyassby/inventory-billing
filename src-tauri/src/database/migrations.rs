use rusqlite::{Connection, Result};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use rand::rngs::OsRng;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON;", [])?;

    // Create tables
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL, -- 'admin', 'manager', 'accountant', 'staff'
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS brands (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS units (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            short_name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS products (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            sku TEXT NOT NULL UNIQUE,
            barcode TEXT UNIQUE,
            qr_code TEXT,
            category_id TEXT,
            brand_id TEXT,
            cost REAL NOT NULL DEFAULT 0.0,
            price REAL NOT NULL DEFAULT 0.0,
            stock REAL NOT NULL DEFAULT 0.0,
            reorder_level REAL NOT NULL DEFAULT 0.0,
            is_expiry_tracked INTEGER DEFAULT 0,
            expiry_date TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0,
            unit TEXT DEFAULT 'pcs',
            description TEXT,
            image_path TEXT,
            tax_id TEXT,
            unit_id TEXT,
            offer_price REAL DEFAULT 0.0,
            FOREIGN KEY(category_id) REFERENCES categories(id) ON DELETE SET NULL,
            FOREIGN KEY(brand_id) REFERENCES brands(id) ON DELETE SET NULL,
            FOREIGN KEY(tax_id) REFERENCES tax_rates(id) ON DELETE SET NULL,
            FOREIGN KEY(unit_id) REFERENCES units(id) ON DELETE SET NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS stock_adjustments (
            id TEXT PRIMARY KEY,
            product_id TEXT NOT NULL,
            adjustment_type TEXT NOT NULL, -- 'ADD', 'SUBTRACT', 'SET'
            quantity REAL NOT NULL,
            reason TEXT,
            adjusted_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE CASCADE,
            FOREIGN KEY(adjusted_by) REFERENCES users(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS customers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT,
            phone TEXT,
            address TEXT,
            due_amount REAL DEFAULT 0.0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS suppliers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT,
            phone TEXT,
            address TEXT,
            balance REAL DEFAULT 0.0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS leads (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            contact_name TEXT,
            phone TEXT,
            email TEXT,
            stage TEXT NOT NULL, -- 'New', 'Contacted', 'Proposal', 'Negotiation', 'Won', 'Lost'
            deal_value REAL DEFAULT 0.0,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS quotations (
            id TEXT PRIMARY KEY,
            quotation_number TEXT NOT NULL UNIQUE,
            customer_id TEXT NOT NULL,
            total REAL NOT NULL DEFAULT 0.0,
            status TEXT NOT NULL, -- 'Draft', 'Sent', 'Accepted', 'Declined'
            date TEXT NOT NULL,
            created_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0,
            FOREIGN KEY(customer_id) REFERENCES customers(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS quotation_items (
            id TEXT PRIMARY KEY,
            quotation_id TEXT NOT NULL,
            product_id TEXT NOT NULL,
            quantity REAL NOT NULL,
            unit_price REAL NOT NULL,
            total REAL NOT NULL,
            FOREIGN KEY(quotation_id) REFERENCES quotations(id) ON DELETE CASCADE,
            FOREIGN KEY(product_id) REFERENCES products(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS invoices (
            id TEXT PRIMARY KEY,
            invoice_number TEXT NOT NULL UNIQUE,
            customer_id TEXT NOT NULL,
            subtotal REAL NOT NULL DEFAULT 0.0,
            discount REAL NOT NULL DEFAULT 0.0,
            tax REAL NOT NULL DEFAULT 0.0,
            total REAL NOT NULL DEFAULT 0.0,
            amount_paid REAL NOT NULL DEFAULT 0.0,
            due_amount REAL NOT NULL DEFAULT 0.0,
            payment_status TEXT NOT NULL, -- 'Paid', 'Partial', 'Unpaid'
            payment_method TEXT, -- 'Cash', 'Card', 'UPI', 'Split'
            date TEXT NOT NULL,
            created_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0,
            FOREIGN KEY(customer_id) REFERENCES customers(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS invoice_items (
            id TEXT PRIMARY KEY,
            invoice_id TEXT NOT NULL,
            product_id TEXT NOT NULL,
            quantity REAL NOT NULL,
            unit_price REAL NOT NULL,
            total REAL NOT NULL,
            FOREIGN KEY(invoice_id) REFERENCES invoices(id) ON DELETE CASCADE,
            FOREIGN KEY(product_id) REFERENCES products(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS purchases (
            id TEXT PRIMARY KEY,
            purchase_number TEXT NOT NULL UNIQUE,
            supplier_id TEXT NOT NULL,
            total REAL NOT NULL DEFAULT 0.0,
            status TEXT NOT NULL, -- 'Ordered', 'Received', 'Pending'
            date TEXT NOT NULL,
            created_at TEXT NOT NULL,
            is_deleted INTEGER DEFAULT 0,
            FOREIGN KEY(supplier_id) REFERENCES suppliers(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS purchase_items (
            id TEXT PRIMARY KEY,
            purchase_id TEXT NOT NULL,
            product_id TEXT NOT NULL,
            quantity REAL NOT NULL,
            unit_price REAL NOT NULL,
            total REAL NOT NULL,
            FOREIGN KEY(purchase_id) REFERENCES purchases(id) ON DELETE CASCADE,
            FOREIGN KEY(product_id) REFERENCES products(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS payments (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL, -- 'RECEIVE', 'PAY'
            entity_id TEXT NOT NULL, -- Customer ID or Supplier ID
            amount REAL NOT NULL,
            payment_method TEXT NOT NULL,
            reference_no TEXT,
            date TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS expenses (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            category TEXT NOT NULL,
            amount REAL NOT NULL,
            date TEXT NOT NULL,
            description TEXT,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS tax_rates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            rate REAL NOT NULL,
            is_active INTEGER DEFAULT 1,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS chart_of_accounts (
            id TEXT PRIMARY KEY,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            type TEXT NOT NULL, -- 'Asset', 'Liability', 'Equity', 'Revenue', 'Expense'
            parent_id TEXT,
            is_active INTEGER DEFAULT 1,
            FOREIGN KEY(parent_id) REFERENCES chart_of_accounts(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS journal_entries (
            id TEXT PRIMARY KEY,
            entry_date TEXT NOT NULL,
            description TEXT NOT NULL,
            reference TEXT,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS journal_lines (
            id TEXT PRIMARY KEY,
            journal_entry_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            debit REAL NOT NULL DEFAULT 0.0,
            credit REAL NOT NULL DEFAULT 0.0,
            FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id) ON DELETE CASCADE,
            FOREIGN KEY(account_id) REFERENCES chart_of_accounts(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            user_id TEXT,
            action TEXT NOT NULL, -- 'CREATE', 'UPDATE', 'DELETE', 'LOGIN'
            table_name TEXT NOT NULL,
            record_id TEXT,
            old_values TEXT, -- JSON
            new_values TEXT, -- JSON
            timestamp TEXT NOT NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS held_orders (
            id TEXT PRIMARY KEY,
            customer_name TEXT NOT NULL,
            discount REAL NOT NULL,
            tax_rate REAL NOT NULL,
            items TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    // Create indexes for optimization
    conn.execute("CREATE INDEX IF NOT EXISTS idx_products_sku ON products(sku);", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_products_barcode ON products(barcode);", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_invoices_number ON invoices(invoice_number);", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token_hash);", [])?;

    // Seed default roles / default admin user if none exists
    let user_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users",
        [],
        |row| row.get(0),
    )?;

    if user_count == 0 {
        let admin_id = uuid::Uuid::new_v4().to_string();
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password = "password123";
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .to_string();

        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO users (id, name, email, password_hash, role, created_at, updated_at, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            (
                &admin_id,
                "Super Admin",
                "admin@example.com",
                &password_hash,
                "admin",
                &now,
                &now,
            ),
        )?;

        // Seed some sample chart of accounts
        let sample_accounts = vec![
            ("1000", "Cash on Hand", "Asset"),
            ("1010", "Bank Account", "Asset"),
            ("1200", "Accounts Receivable", "Asset"),
            ("1400", "Inventory Asset", "Asset"),
            ("2000", "Accounts Payable", "Liability"),
            ("3000", "Owner's Equity", "Equity"),
            ("4000", "Sales Revenue", "Revenue"),
            ("5000", "Cost of Goods Sold", "Expense"),
            ("5100", "Rent Expense", "Expense"),
            ("5200", "Utility Expense", "Expense"),
        ];

        for (code, name, acct_type) in sample_accounts {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO chart_of_accounts (id, code, name, type, parent_id, is_active)
                 VALUES (?1, ?2, ?3, ?4, NULL, 1)",
                (&id, code, name, acct_type),
            )?;
        }

        // Seed sample tax rate
        let tax_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tax_rates (id, name, rate, is_active, created_at)
             VALUES (?1, ?2, ?3, 1, ?4)",
            (&tax_id, "GST 18%", &18.0, &now),
        )?;
    }

    // Check if new product columns exist and add them if missing
    let table_info: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(products)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut cols = Vec::new();
        for r in rows {
            cols.push(r?);
        }
        cols
    };

    if !table_info.contains(&"description".to_string()) {
        conn.execute("ALTER TABLE products ADD COLUMN description TEXT;", [])?;
    }
    if !table_info.contains(&"image_path".to_string()) {
        conn.execute("ALTER TABLE products ADD COLUMN image_path TEXT;", [])?;
    }
    if !table_info.contains(&"tax_id".to_string()) {
        conn.execute("ALTER TABLE products ADD COLUMN tax_id TEXT;", [])?;
    }
    if !table_info.contains(&"unit_id".to_string()) {
        conn.execute("ALTER TABLE products ADD COLUMN unit_id TEXT;", [])?;
    }
    if !table_info.contains(&"offer_price".to_string()) {
        conn.execute("ALTER TABLE products ADD COLUMN offer_price REAL DEFAULT 0.0;", [])?;
    }

    // Seed default units if none exist
    let unit_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM units",
        [],
        |row| row.get(0),
    )?;

    if unit_count == 0 {
        let default_units = vec![
            ("pcs", "Pieces"),
            ("box", "Box"),
            ("pack", "Pack"),
            ("kg", "Kilograms"),
            ("g", "Grams"),
            ("ltr", "Liters"),
            ("ml", "Milliliters"),
        ];
        let now = chrono::Utc::now().to_rfc3339();
        for (short_name, name) in default_units {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO units (id, name, short_name, created_at, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, 0)",
                (&id, name, short_name, &now),
            )?;
        }
    }

    Ok(())
}
