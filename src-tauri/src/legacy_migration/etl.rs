//! v1 → v2 transformation. Runs inside one transaction on the new database; the v1 database is
//! only read (from the backup copy). See docs/DATABASE.md §7 for the mapping rules.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use super::reader::{read_table, LegacyRow};
use super::report::{BalanceAdjustment, LossyAmount, MigrationReport, NegativeStock};
use super::MigrateError;
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};

/// Fixed ids from migration 0006.
const ROLE_OWNER: &str = "01920000-0000-7000-8000-000000000001";
const ROLE_MANAGER: &str = "01920000-0000-7000-8000-000000000002";
const ROLE_CASHIER: &str = "01920000-0000-7000-8000-000000000003";
const ROLE_ACCOUNTANT: &str = "01920000-0000-7000-8000-000000000005";
const ACCOUNT_CASH: &str = "01920000-0000-7000-8001-000000001000";
const ACCOUNT_RENT: &str = "01920000-0000-7000-8001-000000005100";
const ACCOUNT_UTILITIES: &str = "01920000-0000-7000-8001-000000005200";

/// The password v1 seeded for its default admin account.
const V1_DEFAULT_PASSWORD: &str = "password123";

/// v1 tables copied verbatim into legacy_records (sessions are dropped: secrets, no value).
const ARCHIVED_TABLES: &[&str] = &[
    "users",
    "categories",
    "brands",
    "units",
    "tax_rates",
    "products",
    "stock_adjustments",
    "customers",
    "suppliers",
    "leads",
    "quotations",
    "quotation_items",
    "invoices",
    "invoice_items",
    "purchases",
    "purchase_items",
    "payments",
    "expenses",
    "chart_of_accounts",
    "journal_entries",
    "journal_lines",
    "audit_logs",
    "held_orders",
];

struct ProductInfo {
    variant_id: String,
    name: String,
    sku: String,
    unit_id: String,
    unit_name: String,
    cost: i64,
    tax_rate_id: Option<String>,
    tax_rate: i64,
}

struct PartyInfo {
    name: String,
    phone: Option<String>,
    address: Option<String>,
}

pub(super) struct Etl<'a> {
    db: &'a Connection,
    legacy: &'a Connection,
    report: &'a mut MigrationReport,
    now: String,
    today: String,
    minor_digits: u32,
    branch_id: String,
    warehouse_id: String,
    users: HashSet<String>,
    categories: HashSet<String>,
    brands: HashSet<String>,
    units: HashMap<String, (String, String)>,
    unit_by_code: HashMap<String, String>,
    tax_rates: HashMap<String, i64>,
    products: HashMap<String, ProductInfo>,
    parties: HashMap<String, PartyInfo>,
    accounts: HashMap<String, String>,
    series: HashMap<&'static str, String>,
    used_seq: HashSet<(String, i64)>,
    unknown_supplier: Option<String>,
    payment_seq: i64,
}

type Result<T> = std::result::Result<T, MigrateError>;

impl<'a> Etl<'a> {
    pub(super) fn new(
        db: &'a Connection,
        legacy: &'a Connection,
        report: &'a mut MigrationReport,
        minor_digits: u32,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            db,
            legacy,
            report,
            now: now.to_rfc3339(),
            today: now.format("%Y-%m-%d").to_string(),
            minor_digits,
            branch_id: String::new(),
            warehouse_id: String::new(),
            users: HashSet::new(),
            categories: HashSet::new(),
            brands: HashSet::new(),
            units: HashMap::new(),
            unit_by_code: HashMap::new(),
            tax_rates: HashMap::new(),
            products: HashMap::new(),
            parties: HashMap::new(),
            accounts: HashMap::new(),
            series: HashMap::new(),
            used_seq: HashSet::new(),
            unknown_supplier: None,
            payment_seq: 0,
        }
    }

    pub(super) fn run(&mut self) -> Result<()> {
        self.archive_legacy_rows()?;
        self.organisation()?;
        self.users()?;
        self.categories_and_brands()?;
        self.units()?;
        self.tax_rates()?;
        self.products_and_opening_stock()?;
        self.parties()?;
        self.leads()?;
        self.accounts()?;
        self.invoices()?;
        self.quotations()?;
        self.purchases()?;
        self.legacy_payments()?;
        self.expenses()?;
        self.journals()?;
        self.audit_log()?;
        self.held_orders()?;
        self.record_unmapped_tables()?;
        self.reconcile_party_balances()?;
        self.verify()
    }

    // ------------------------------------------------------------------ helpers

    fn amount(&mut self, table: &str, id: &str, column: &str, value: f64) -> i64 {
        match money::from_legacy_float(value, self.minor_digits) {
            Some((minor, lossy)) => {
                if lossy {
                    self.report.lossy_amounts.push(LossyAmount {
                        table: table.into(),
                        id: id.into(),
                        column: column.into(),
                        legacy_value: value,
                        migrated_minor: minor,
                    });
                }
                minor
            }
            None => {
                self.report.warn(format!(
                    "{table} {id}: {column} = {value} is not a number; stored as 0"
                ));
                0
            }
        }
    }

    /// Non-negative amount; negatives are reported and clamped to 0.
    fn amount_non_negative(&mut self, table: &str, id: &str, column: &str, value: f64) -> i64 {
        let minor = self.amount(table, id, column, value);
        if minor < 0 {
            self.report.warn(format!(
                "{table} {id}: negative {column} ({value}) stored as 0"
            ));
            return 0;
        }
        minor
    }

    fn qty(&mut self, table: &str, id: &str, column: &str, value: f64) -> i64 {
        match money::qty_from_legacy_float(value) {
            Some((milli, lossy)) => {
                if lossy {
                    self.report.warn(format!(
                        "{table} {id}: {column} {value} has more than 3 decimals; rounded to {milli} thousandths"
                    ));
                }
                milli
            }
            None => {
                self.report.warn(format!(
                    "{table} {id}: {column} = {value} is not a number; stored as 0"
                ));
                0
            }
        }
    }

    fn rate(value: f64) -> i64 {
        // percent → ppm, rounded the same way as everything else migrated from floats.
        money::from_legacy_float(value * 10_000.0, 0).map_or(0, |(ppm, _)| ppm.clamp(0, 1_000_000))
    }

    fn timestamp(&self, row: &LegacyRow, key: &str) -> String {
        row.text(key)
            .filter(|t| chrono::DateTime::parse_from_rfc3339(t).is_ok())
            .unwrap_or_else(|| self.now.clone())
    }

    fn business_date(&self, row: &LegacyRow, key: &str) -> String {
        let candidate = row
            .text(key)
            .or_else(|| row.text("created_at"))
            .unwrap_or_default();
        let prefix: String = candidate.chars().take(10).collect();
        if chrono::NaiveDate::parse_from_str(&prefix, "%Y-%m-%d").is_ok() {
            prefix
        } else {
            self.today.clone()
        }
    }

    fn user_ref(&self, id: Option<String>) -> Option<String> {
        id.filter(|u| self.users.contains(u))
    }

    fn count_read(&mut self, table: &str, rows: usize) {
        self.report.table(table).legacy_rows += rows;
    }

    fn count_written(&mut self, table: &str, target: &str) {
        let counts = self.report.table(table);
        counts.migrated_rows += 1;
        if !counts.migrated_to.iter().any(|t| t == target) {
            counts.migrated_to.push(target.to_string());
        }
    }

    fn count_skipped(&mut self, table: &str) {
        self.report.table(table).skipped_rows += 1;
    }

    /// Allocation that never fails: all-zero weights fall back to equal shares.
    fn spread(total: i64, weights: &[i64]) -> Vec<i64> {
        if weights.is_empty() {
            return Vec::new();
        }
        let usable = if weights.iter().all(|w| *w <= 0) {
            vec![1; weights.len()]
        } else {
            weights.iter().map(|w| (*w).max(0)).collect()
        };
        money::allocate(total, &usable).expect("weights are non-negative with a positive sum")
    }

    /// Trailing digits of a v1 number ("INV-000123" → 123), unique per series.
    fn doc_seq(&mut self, series_id: &str, doc_no: &str) -> Option<i64> {
        let digits: String = doc_no
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let seq: i64 = digits.parse().ok().filter(|s| *s >= 1)?;
        self.used_seq
            .insert((series_id.to_string(), seq))
            .then_some(seq)
    }

    fn next_payment_no(&mut self, prefix: &str) -> String {
        self.payment_seq += 1;
        format!("{prefix}-{:06}", self.payment_seq)
    }

    fn post_party_ledger(
        &self,
        party_id: &str,
        date: &str,
        entry_type: &str,
        source: (&str, &str),
        debit: i64,
        credit: i64,
        narration: &str,
    ) -> Result<()> {
        if debit == 0 && credit == 0 {
            return Ok(());
        }
        self.db.execute(
            "INSERT INTO party_ledger (id, party_id, business_date, entry_type, source_type, source_id,
                debit, credit, narration, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![new_id(), party_id, date, entry_type, source.0, source.1, debit, credit, narration, self.now],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------ steps

    fn archive_legacy_rows(&mut self) -> Result<()> {
        let mut stmt = self.db.prepare(
            "INSERT INTO legacy_records (source_table, source_id, row_json, migrated_at) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for table in ARCHIVED_TABLES {
            for row in read_table(self.legacy, table)? {
                let id = row.id();
                if id.is_empty() {
                    continue;
                }
                // Password hashes are not archived: a later password change must not leave the old
                // hash lying around.
                stmt.execute(params![
                    table,
                    id,
                    row.to_json_without(&["password_hash"]),
                    self.now
                ])?;
            }
        }
        Ok(())
    }

    fn organisation(&mut self) -> Result<()> {
        self.branch_id = new_id();
        self.warehouse_id = new_id();
        self.db.execute(
            "INSERT INTO branches (id, code, name, is_default, created_at, updated_at) VALUES (?1, 'MAIN', 'Main branch', 1, ?2, ?2)",
            params![self.branch_id, self.now],
        )?;
        self.db.execute(
            "INSERT INTO warehouses (id, branch_id, code, name, is_default, created_at, updated_at) VALUES (?1, ?2, 'MAIN', 'Main store', 1, ?3, ?3)",
            params![self.warehouse_id, self.branch_id, self.now],
        )?;
        // One "never resets" series per document type keeps v1 numbers unique and searchable.
        for (doc_type, prefix) in [
            ("sales_invoice", "INV"),
            ("quotation", "QT"),
            ("purchase_order", "PO"),
            ("purchase_bill", "PO"),
            ("payment_in", "V1-RCT"),
            ("payment_out", "V1-PAY"),
            ("expense", "V1-EXP"),
        ] {
            let id = new_id();
            self.db.execute(
                "INSERT INTO number_series (id, branch_id, doc_type, name, prefix, template, padding, reset_policy,
                    is_default, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'Migrated from v1', ?4, '{prefix}-{seq}', 6, 'never', 0, 0, ?5, ?5)",
                params![id, self.branch_id, doc_type, prefix, self.now],
            )?;
            self.series.insert(doc_type, id);
        }
        Ok(())
    }

    fn users(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "users")?;
        self.count_read("users", rows.len());
        // E-mail is the login. v1 compared it case-sensitively; v2 doesn't. Renaming someone's
        // login silently would lock them out, so stop and ask for it to be fixed in v1 instead.
        let mut seen: HashMap<String, String> = HashMap::new();
        for row in rows.iter().filter(|r| !r.flag("is_deleted")) {
            let email = row.text_or("email", "").to_lowercase();
            if let Some(other) = seen.insert(email.clone(), row.text_or("name", "")) {
                return Err(MigrateError::NeedsAttention(format!(
                    "users '{other}' and '{}' have the same e-mail ({email}) apart from letter case; change one in v1 (Settings → Users) and run the migration again",
                    row.text_or("name", "")
                )));
            }
        }
        for row in rows {
            let id = row.id();
            let legacy_role = row.text_or("role", "staff").to_lowercase();
            let role_id = match legacy_role.as_str() {
                "admin" => ROLE_OWNER,
                "manager" => ROLE_MANAGER,
                "accountant" => ROLE_ACCOUNTANT,
                "staff" | "cashier" => ROLE_CASHIER,
                other => {
                    self.report.warn(format!(
                        "user {id}: unknown v1 role '{other}' mapped to cashier"
                    ));
                    ROLE_CASHIER
                }
            };
            let hash = row.text_or("password_hash", "");
            let deleted = row.flag("is_deleted");
            let must_change = crate::auth::verify_password(&hash, V1_DEFAULT_PASSWORD);
            let email = row.text_or("email", &format!("user-{id}@invalid.local"));
            if must_change && !deleted {
                self.report
                    .users_requiring_password_change
                    .push(email.clone());
            }
            let updated = self.timestamp(&row, "updated_at");
            self.db.execute(
                "INSERT INTO users (id, name, email, password_hash, role_id, is_active, must_change_password,
                    created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    row.text_or("name", &email),
                    email,
                    hash,
                    role_id,
                    !deleted,
                    must_change,
                    self.timestamp(&row, "created_at"),
                    updated,
                    deleted.then_some(updated.clone()),
                ],
            )?;
            self.users.insert(id);
            self.count_written("users", "users");
        }
        Ok(())
    }

    fn categories_and_brands(&mut self) -> Result<()> {
        for (table, target) in [("categories", "categories"), ("brands", "brands")] {
            let rows = read_table(self.legacy, table)?;
            self.count_read(table, rows.len());
            let mut live_names = HashSet::new();
            for row in rows {
                let id = row.id();
                let deleted = row.flag("is_deleted");
                let mut name = row.text_or("name", "Unnamed");
                if !deleted && !live_names.insert(name.to_lowercase()) {
                    name = format!("{name} ({})", &id[..8.min(id.len())]);
                    self.report.warn(format!("{table} {id}: name differs only by letter case from another; renamed to '{name}'"));
                }
                let created = self.timestamp(&row, "created_at");
                self.db.execute(
                    &format!(
                        "INSERT INTO {target} (id, name, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?3, ?4)"
                    ),
                    params![id, name, created, deleted.then_some(self.now.clone())],
                )?;
                if table == "categories" {
                    self.categories.insert(id);
                } else {
                    self.brands.insert(id);
                }
                self.count_written(table, target);
            }
        }
        Ok(())
    }

    fn units(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "units")?;
        self.count_read("units", rows.len());
        for row in rows {
            let id = row.id();
            let code = row.text_or("short_name", "unit");
            let name = row.text_or("name", &code);
            let deleted = row.flag("is_deleted");
            let decimals = match code.to_lowercase().as_str() {
                "kg" | "g" | "gm" | "ltr" | "l" | "ml" | "m" | "cm" | "mtr" => 3,
                _ => 0,
            };
            self.db.execute(
                "INSERT INTO units (id, code, name, decimals, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
                params![
                    id,
                    code,
                    name,
                    decimals,
                    self.timestamp(&row, "created_at"),
                    deleted.then_some(self.now.clone())
                ],
            )?;
            if !deleted {
                self.unit_by_code.insert(code.to_lowercase(), id.clone());
            }
            self.units.insert(id, (code, name));
            self.count_written("units", "units");
        }
        Ok(())
    }

    /// Unit for a product: its unit_id, else the free-text unit, else a created unit.
    fn unit_for(&mut self, unit_id: Option<String>, unit_text: &str) -> Result<String> {
        if let Some(id) = unit_id.filter(|u| self.units.contains_key(u)) {
            return Ok(id);
        }
        let code = if unit_text.is_empty() {
            "pcs"
        } else {
            unit_text
        };
        if let Some(id) = self.unit_by_code.get(&code.to_lowercase()) {
            return Ok(id.clone());
        }
        let id = new_id();
        self.db.execute(
            "INSERT INTO units (id, code, name, decimals, created_at, updated_at) VALUES (?1, ?2, ?2, 0, ?3, ?3)",
            params![id, code, self.now],
        )?;
        self.report.warn(format!(
            "created unit '{code}' that products referenced but v1 did not define"
        ));
        self.unit_by_code.insert(code.to_lowercase(), id.clone());
        self.units
            .insert(id.clone(), (code.to_string(), code.to_string()));
        Ok(id)
    }

    fn tax_rates(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "tax_rates")?;
        self.count_read("tax_rates", rows.len());
        for row in rows {
            let id = row.id();
            let name = row.text_or("name", "Tax");
            let rate = Self::rate(row.real("rate"));
            let tax_type = if name.to_lowercase().contains("gst") && rate % 2 == 0 {
                "gst"
            } else {
                "other"
            };
            self.db.execute(
                "INSERT INTO tax_rates (id, name, tax_type, rate, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![id, name, tax_type, rate, row.real("is_active") != 0.0 || row.0.get("is_active").is_none(), self.timestamp(&row, "created_at")],
            )?;
            self.tax_rates.insert(id, rate);
            self.count_written("tax_rates", "tax_rates");
        }
        Ok(())
    }

    fn products_and_opening_stock(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "products")?;
        self.count_read("products", rows.len());
        let mut live_skus = HashSet::new();
        for row in rows {
            let id = row.id();
            let name = row.text_or("name", "Unnamed product");
            let deleted = row.flag("is_deleted");
            let deleted_at = deleted.then_some(self.timestamp(&row, "updated_at"));
            let created = self.timestamp(&row, "created_at");
            let updated = self.timestamp(&row, "updated_at");

            let category_id = row
                .text("category_id")
                .filter(|c| self.categories.contains(c));
            let brand_id = row.text("brand_id").filter(|b| self.brands.contains(b));
            if row.text("category_id").is_some() && category_id.is_none() {
                self.report.warn(format!(
                    "product {id}: category no longer exists; left blank"
                ));
            }
            let tax_rate_id = row
                .text("tax_id")
                .filter(|t| self.tax_rates.contains_key(t));
            let tax_rate = tax_rate_id
                .as_ref()
                .and_then(|t| self.tax_rates.get(t))
                .copied()
                .unwrap_or(0);
            let unit_id = self.unit_for(row.text("unit_id"), &row.text_or("unit", "pcs"))?;
            let track_expiry = row.flag("is_expiry_tracked");
            let expiry_date = row.text("expiry_date");

            self.db.execute(
                "INSERT INTO products (id, name, description, category_id, brand_id, base_unit_id, tax_rate_id,
                    track_batches, track_expiry, image_path, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    id, name, row.text("description"), category_id, brand_id, unit_id, tax_rate_id,
                    track_expiry && expiry_date.is_some(), track_expiry, row.text("image_path"),
                    created, updated, deleted_at
                ],
            )?;

            // v1 sold at offer_price when it was set, so that is the sale price; the list price
            // becomes the MRP.
            let price = self.amount_non_negative("products", &id, "price", row.real("price"));
            let offer =
                self.amount_non_negative("products", &id, "offer_price", row.real("offer_price"));
            let (sale_price, mrp) = if offer > 0 {
                (offer, Some(price))
            } else {
                (price, None)
            };
            let cost = self.amount_non_negative("products", &id, "cost", row.real("cost"));
            let reorder = self
                .qty("products", &id, "reorder_level", row.real("reorder_level"))
                .max(0);

            let mut sku = row.text_or("sku", &format!("V1-{}", &id[..8.min(id.len())]));
            if !deleted && !live_skus.insert(sku.to_lowercase()) {
                sku = format!("{sku}-{}", &id[..4.min(id.len())]);
                self.report.warn(format!("product {id}: SKU differs only by letter case from another; changed to '{sku}'"));
            }
            let variant_id = new_id();
            self.db.execute(
                "INSERT INTO product_variants (id, product_id, sku, barcode, sale_price, mrp, cost_price,
                    reorder_level, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![variant_id, id, sku, row.text("barcode"), sale_price, mrp, cost, reorder, created, updated, deleted_at],
            )?;

            // Opening stock: v1's stock figure becomes one ledger row, so the v2 cache equals it.
            let stock = self.qty("products", &id, "stock", row.real("stock"));
            if stock != 0 {
                let batch_id = if track_expiry && expiry_date.is_some() {
                    let batch = new_id();
                    self.db.execute(
                        "INSERT INTO batches (id, variant_id, batch_no, expiry_date, created_at, updated_at)
                         VALUES (?1, ?2, 'V1', ?3, ?4, ?4)",
                        params![batch, variant_id, expiry_date, self.now],
                    )?;
                    Some(batch)
                } else {
                    None
                };
                let value = money::multiply_by_qty(cost, stock, RoundingMode::HalfUp);
                self.db.execute(
                    "INSERT INTO stock_movements (id, business_date, occurred_at, warehouse_id, variant_id, batch_id,
                        movement_type, qty, unit_cost, value, source_type, source_id, note, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'opening', ?7, ?8, ?9, 'legacy_migration', ?10, 'Stock carried over from v1', ?3)",
                    params![new_id(), self.today, self.now, self.warehouse_id, variant_id, batch_id, stock, cost, value, id],
                )?;
                if stock < 0 {
                    self.report.negative_stock.push(NegativeStock {
                        variant_id: variant_id.clone(),
                        sku: sku.clone(),
                        qty_milli: stock,
                    });
                }
            }
            self.report.totals.legacy_stock_value += row.real("stock") * row.real("cost");

            let unit_name = self
                .units
                .get(&unit_id)
                .map(|(code, _)| code.clone())
                .unwrap_or_default();
            self.products.insert(
                id,
                ProductInfo {
                    variant_id,
                    name,
                    sku,
                    unit_id,
                    unit_name,
                    cost,
                    tax_rate_id,
                    tax_rate,
                },
            );
            self.count_written(
                "products",
                "products + product_variants (+ opening stock_movements)",
            );
        }
        Ok(())
    }

    /// v1 invoice lines can point at products that are gone; keep the line with a placeholder.
    fn product_or_placeholder(&mut self, product_id: &str) -> Result<String> {
        if self.products.contains_key(product_id) {
            return Ok(product_id.to_string());
        }
        let unit_id = self.unit_for(None, "pcs")?;
        let id = format!("v1-missing-{product_id}");
        if !self.products.contains_key(&id) {
            let name = format!(
                "Unknown product (v1 {})",
                &product_id[..8.min(product_id.len())]
            );
            let sku = format!("V1-MISSING-{}", self.products.len() + 1);
            self.db.execute(
                "INSERT INTO products (id, name, base_unit_id, is_active, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
                params![id, name, unit_id, self.now],
            )?;
            let variant_id = new_id();
            self.db.execute(
                "INSERT INTO product_variants (id, product_id, sku, is_active, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
                params![variant_id, id, sku, self.now],
            )?;
            self.report.warn(format!("documents referenced missing product {product_id}; created inactive placeholder '{name}'"));
            self.products.insert(
                id.clone(),
                ProductInfo {
                    variant_id,
                    name,
                    sku,
                    unit_id,
                    unit_name: "pcs".into(),
                    cost: 0,
                    tax_rate_id: None,
                    tax_rate: 0,
                },
            );
        }
        Ok(id)
    }

    fn parties(&mut self) -> Result<()> {
        for (table, party_type) in [("customers", "customer"), ("suppliers", "supplier")] {
            let rows = read_table(self.legacy, table)?;
            self.count_read(table, rows.len());
            for row in rows {
                let id = row.id();
                let name = row.text_or("name", "Unnamed");
                let deleted = row.flag("is_deleted");
                let updated = self.timestamp(&row, "updated_at");
                self.db.execute(
                    "INSERT INTO parties (id, party_type, name, phone, email, billing_address, created_at, updated_at, deleted_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        id, party_type, name, row.text("phone"), row.text("email"), row.text("address"),
                        self.timestamp(&row, "created_at"), updated, deleted.then_some(updated.clone())
                    ],
                )?;
                self.parties.insert(
                    id,
                    PartyInfo {
                        name,
                        phone: row.text("phone"),
                        address: row.text("address"),
                    },
                );
                self.count_written(table, "parties");
            }
        }
        Ok(())
    }

    fn supplier_or_placeholder(&mut self, supplier_id: Option<String>) -> Result<String> {
        if let Some(id) = supplier_id.filter(|s| self.parties.contains_key(s)) {
            return Ok(id);
        }
        if let Some(id) = &self.unknown_supplier {
            return Ok(id.clone());
        }
        let id = new_id();
        self.db.execute(
            "INSERT INTO parties (id, party_type, name, is_active, created_at, updated_at)
             VALUES (?1, 'supplier', 'Unknown supplier (v1)', 0, ?2, ?2)",
            params![id, self.now],
        )?;
        self.report.warn("purchases referenced a missing supplier; linked to placeholder 'Unknown supplier (v1)'");
        self.parties.insert(
            id.clone(),
            PartyInfo {
                name: "Unknown supplier (v1)".into(),
                phone: None,
                address: None,
            },
        );
        self.unknown_supplier = Some(id.clone());
        Ok(id)
    }

    fn leads(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "leads")?;
        self.count_read("leads", rows.len());
        for row in rows {
            let id = row.id();
            let stage_v1 = row.text_or("stage", "new").to_lowercase();
            let stage = match stage_v1.as_str() {
                s @ ("new" | "contacted" | "proposal" | "negotiation" | "won" | "lost") => {
                    s.to_string()
                }
                other => {
                    self.report.warn(format!(
                        "lead {id}: unknown stage '{other}' mapped to 'new'"
                    ));
                    "new".into()
                }
            };
            let deal_value =
                self.amount_non_negative("leads", &id, "deal_value", row.real("deal_value"));
            let updated = self.timestamp(&row, "updated_at");
            self.db.execute(
                "INSERT INTO leads (id, name, contact_name, phone, email, stage, deal_value, notes, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id, row.text_or("name", "Unnamed lead"), row.text("contact_name"), row.text("phone"), row.text("email"),
                    stage, deal_value, row.text("notes"), self.timestamp(&row, "created_at"), updated,
                    row.flag("is_deleted").then_some(updated.clone())
                ],
            )?;
            self.count_written("leads", "leads");
        }
        Ok(())
    }

    fn accounts(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "chart_of_accounts")?;
        self.count_read("chart_of_accounts", rows.len());
        for row in &rows {
            let id = row.id();
            let code = row.text_or("code", &id);
            let existing: Option<String> = self
                .db
                .query_row("SELECT id FROM accounts WHERE code = ?1", [&code], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(system_id) = existing {
                // Same code as a v2 system account: v1 entries post to the system account.
                self.accounts.insert(id, system_id);
                self.count_written("chart_of_accounts", "accounts (matched system account)");
                continue;
            }
            let account_type = match row.text_or("type", "").to_lowercase().as_str() {
                "asset" => "asset",
                "liability" => "liability",
                "equity" => "equity",
                "revenue" | "income" => "income",
                "expense" => "expense",
                other => {
                    self.report.warn(format!(
                        "account {code}: unknown type '{other}' treated as expense"
                    ));
                    "expense"
                }
            };
            self.db.execute(
                "INSERT INTO accounts (id, code, name, account_type, subtype, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'other', ?5, ?6, ?6)",
                params![id, code, row.text_or("name", &code), account_type, row.real("is_active") != 0.0, self.now],
            )?;
            self.accounts.insert(id.clone(), id);
            self.count_written("chart_of_accounts", "accounts");
        }
        for row in &rows {
            if let (Some(parent), Some(mapped)) =
                (row.text("parent_id"), self.accounts.get(&row.id()))
            {
                if let Some(mapped_parent) = self.accounts.get(&parent) {
                    if mapped_parent != mapped {
                        self.db.execute(
                            "UPDATE accounts SET parent_id = ?1 WHERE id = ?2 AND is_system = 0",
                            params![mapped_parent, mapped],
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn invoices(&mut self) -> Result<()> {
        let invoices = read_table(self.legacy, "invoices")?;
        let items = read_table(self.legacy, "invoice_items")?;
        self.count_read("invoices", invoices.len());
        self.count_read("invoice_items", items.len());
        let mut lines_by_invoice: HashMap<String, Vec<LegacyRow>> = HashMap::new();
        for item in items {
            lines_by_invoice
                .entry(item.text_or("invoice_id", ""))
                .or_default()
                .push(item);
        }
        let series_id = self.series["sales_invoice"].clone();

        for inv in invoices {
            let id = inv.id();
            let doc_no = inv.text_or("invoice_number", &format!("V1-INV-{id}"));
            let cancelled = inv
                .text_or("payment_status", "")
                .eq_ignore_ascii_case("cancel")
                || inv.flag("is_deleted");
            let created = self.timestamp(&inv, "created_at");
            let date = self.business_date(&inv, "date");

            let total = self.amount_non_negative("invoices", &id, "total", inv.real("total"));
            let mut tax = self.amount_non_negative("invoices", &id, "tax", inv.real("tax"));
            if tax > total {
                self.report.warn(format!(
                    "invoice {doc_no}: tax exceeds total; tax capped at the total"
                ));
                tax = total;
            }
            let taxable = total - tax;
            let gross = self.amount_non_negative("invoices", &id, "subtotal", inv.real("subtotal"));
            let discount =
                self.amount_non_negative("invoices", &id, "discount", inv.real("discount"));
            let paid = self.amount("invoices", &id, "amount_paid", inv.real("amount_paid"));

            let customer_id = inv
                .text("customer_id")
                .filter(|c| self.parties.contains_key(c));
            let (customer_name, customer_phone, billing_address) =
                match customer_id.as_ref().and_then(|c| self.parties.get(c)) {
                    Some(p) => (p.name.clone(), p.phone.clone(), p.address.clone()),
                    None => {
                        self.report.warn(format!(
                            "invoice {doc_no}: customer not found; kept as 'Unknown customer'"
                        ));
                        ("Unknown customer".to_string(), None, None)
                    }
                };
            let seq = self.doc_seq(&series_id, &doc_no);
            self.db.execute(
                "INSERT INTO sales_invoices (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no,
                    business_date, status, party_id, customer_name, customer_phone, billing_address, gross,
                    bill_discount, taxable, tax_total, total, snapshot_source, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'all', ?5, ?6, ?7, 'draft', ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'migrated', ?17, ?17)",
                params![
                    id, self.branch_id, self.warehouse_id, series_id, seq, doc_no, date, customer_id,
                    customer_name, customer_phone, billing_address, gross, discount, taxable, tax, total, created
                ],
            )?;

            // Lines: v1 stored only line gross; invoice-level tax and discount are spread over the
            // lines in proportion, so lines add up exactly to the invoice.
            let mut lines = Vec::new();
            for line in lines_by_invoice.remove(&id).unwrap_or_default() {
                let line_id = line.id();
                let qty = self.qty("invoice_items", &line_id, "quantity", line.real("quantity"));
                if qty <= 0 {
                    self.report.warn(format!(
                        "invoice {doc_no}: line with quantity {} skipped",
                        line.real("quantity")
                    ));
                    self.count_skipped("invoice_items");
                    continue;
                }
                let unit_price = self.amount_non_negative(
                    "invoice_items",
                    &line_id,
                    "unit_price",
                    line.real("unit_price"),
                );
                let line_gross = self.amount_non_negative(
                    "invoice_items",
                    &line_id,
                    "total",
                    line.real("total"),
                );
                let product_key =
                    self.product_or_placeholder(&line.text_or("product_id", "unknown"))?;
                lines.push((line_id, product_key, qty, unit_price, line_gross));
            }
            if lines.is_empty() && total > 0 {
                self.report
                    .warn(format!("invoice {doc_no}: has a total but no usable lines"));
            }
            let weights: Vec<i64> = lines.iter().map(|l| l.4).collect();
            let taxable_parts = Self::spread(taxable, &weights);
            let tax_parts = Self::spread(tax, &taxable_parts);
            let discount_parts = Self::spread(discount, &weights);
            for (index, (line_id, product_key, qty, unit_price, _)) in lines.iter().enumerate() {
                let product = &self.products[product_key];
                let (line_taxable, line_tax) = (taxable_parts[index], tax_parts[index]);
                self.db.execute(
                    "INSERT INTO sales_invoice_items (id, invoice_id, line_no, variant_id, unit_id, item_name, sku, unit_name,
                        qty, base_qty, unit_price, bill_discount_share, taxable, tax_rate_id, tax_rate, other_tax, tax_total,
                        line_total, unit_cost, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16, ?17, ?18, ?18)",
                    params![
                        line_id, id, index + 1, product.variant_id, product.unit_id, product.name, product.sku,
                        product.unit_name, qty, unit_price, discount_parts[index], line_taxable,
                        product.tax_rate_id, product.tax_rate, line_tax, line_taxable + line_tax, product.cost, created
                    ],
                )?;
                self.count_written("invoice_items", "sales_invoice_items");
            }

            self.db.execute(
                "UPDATE sales_invoices SET status = 'finalized', finalized_at = ?2 WHERE id = ?1",
                params![id, created],
            )?;
            self.count_written("invoices", "sales_invoices");

            if cancelled {
                self.db.execute(
                    "UPDATE sales_invoices SET status = 'voided', voided_at = ?2, void_reason = 'Cancelled in v1' WHERE id = ?1",
                    params![id, self.now],
                )?;
                if paid > 0 {
                    self.report.warn(format!(
                        "invoice {doc_no}: cancelled in v1 with {paid} (minor units) paid and no refund recorded; no payment migrated"
                    ));
                }
                continue;
            }
            self.report.totals.legacy_sales_total += inv.real("total");

            if let Some(party) = &customer_id {
                self.post_party_ledger(
                    party,
                    &date,
                    "sales_invoice",
                    ("sales_invoice", &id),
                    total,
                    0,
                    &format!("Invoice {doc_no}"),
                )?;
            }

            let applied = paid.clamp(0, total);
            if applied > 0 {
                let method = inv.text_or("payment_method", "").to_uppercase();
                let mode = match method.as_str() {
                    "CASH" => "cash",
                    "CARD" => "card",
                    "UPI" => "upi",
                    other => {
                        self.report.warn(format!(
                            "invoice {doc_no}: payment recorded with method '{other}'; migrated as 'other' (check whether this was really a credit sale)"
                        ));
                        "other"
                    }
                };
                // v1 stored the whole cash handed over as paid; the excess was change (audit B-4).
                let (tendered, change) = if paid > total {
                    (Some(paid), paid - total)
                } else {
                    (None, 0)
                };
                if change > 0 {
                    self.report.warn(format!("invoice {doc_no}: overpayment of {change} (minor units) migrated as change given"));
                }
                let payment_id = new_id();
                self.db.execute(
                    "INSERT INTO payments (id, branch_id, series_id, doc_no, business_date, direction, party_id, account_id,
                        mode, amount, tendered, change_given, notes, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'in', ?6, ?7, ?8, ?9, ?10, ?11, 'Paid at billing (v1)', ?12, ?12)",
                    params![
                        payment_id, self.branch_id, self.series["payment_in"], format!("V1-RCT-{doc_no}"), date, customer_id,
                        ACCOUNT_CASH, mode, applied, tendered, change, created
                    ],
                )?;
                self.db.execute(
                    "INSERT INTO payment_allocations (id, payment_id, target_type, target_id, amount, created_at)
                     VALUES (?1, ?2, 'sales_invoice', ?3, ?4, ?5)",
                    params![new_id(), payment_id, id, applied, created],
                )?;
                if let Some(party) = &customer_id {
                    self.post_party_ledger(
                        party,
                        &date,
                        "payment_in",
                        ("payment", &payment_id),
                        0,
                        applied,
                        &format!("Payment for {doc_no}"),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn quotations(&mut self) -> Result<()> {
        let quotes = read_table(self.legacy, "quotations")?;
        let items = read_table(self.legacy, "quotation_items")?;
        self.count_read("quotations", quotes.len());
        self.count_read("quotation_items", items.len());
        let mut lines_by_quote: HashMap<String, Vec<LegacyRow>> = HashMap::new();
        for item in items {
            lines_by_quote
                .entry(item.text_or("quotation_id", ""))
                .or_default()
                .push(item);
        }
        for quote in quotes {
            let id = quote.id();
            let doc_no = quote.text_or("quotation_number", &format!("V1-QT-{id}"));
            let status = match quote.text_or("status", "").to_lowercase().as_str() {
                "accepted" => "accepted",
                "declined" | "rejected" => "declined",
                _ => "open",
            };
            let total = self.amount_non_negative("quotations", &id, "total", quote.real("total"));
            let customer_id = quote
                .text("customer_id")
                .filter(|c| self.parties.contains_key(c));
            let customer_name = customer_id
                .as_ref()
                .and_then(|c| self.parties.get(c))
                .map_or_else(|| "Unknown customer".to_string(), |p| p.name.clone());
            let created = self.timestamp(&quote, "created_at");
            let date = self.business_date(&quote, "date");
            self.db.execute(
                "INSERT INTO sales_documents (id, doc_type, branch_id, series_id, number_period, doc_no, business_date, status,
                    party_id, customer_name, gross, taxable, total, created_at, updated_at, deleted_at)
                 VALUES (?1, 'quotation', ?2, ?3, 'all', ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?9, ?10, ?10, ?11)",
                params![
                    id, self.branch_id, self.series["quotation"], doc_no, date, status, customer_id, customer_name,
                    total, created, quote.flag("is_deleted").then_some(self.now.clone())
                ],
            )?;
            for (index, line) in lines_by_quote
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .enumerate()
            {
                let line_id = line.id();
                let qty = self.qty(
                    "quotation_items",
                    &line_id,
                    "quantity",
                    line.real("quantity"),
                );
                if qty <= 0 {
                    self.count_skipped("quotation_items");
                    continue;
                }
                let unit_price = self.amount_non_negative(
                    "quotation_items",
                    &line_id,
                    "unit_price",
                    line.real("unit_price"),
                );
                let line_total = self.amount_non_negative(
                    "quotation_items",
                    &line_id,
                    "total",
                    line.real("total"),
                );
                let product_key =
                    self.product_or_placeholder(&line.text_or("product_id", "unknown"))?;
                let product = &self.products[&product_key];
                self.db.execute(
                    "INSERT INTO sales_document_items (id, document_id, line_no, variant_id, unit_id, item_name, sku, qty, base_qty,
                        unit_price, taxable, line_total, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?10, ?11, ?11)",
                    params![
                        line_id, id, index + 1, product.variant_id, product.unit_id, product.name, product.sku, qty,
                        unit_price, line_total, created
                    ],
                )?;
                self.count_written("quotation_items", "sales_document_items");
            }
            self.count_written("quotations", "sales_documents (quotation)");
        }
        Ok(())
    }

    fn purchases(&mut self) -> Result<()> {
        let purchases = read_table(self.legacy, "purchases")?;
        let items = read_table(self.legacy, "purchase_items")?;
        self.count_read("purchases", purchases.len());
        self.count_read("purchase_items", items.len());
        let mut lines_by_purchase: HashMap<String, Vec<LegacyRow>> = HashMap::new();
        for item in items {
            lines_by_purchase
                .entry(item.text_or("purchase_id", ""))
                .or_default()
                .push(item);
        }
        for purchase in purchases {
            let id = purchase.id();
            let doc_no = purchase.text_or("purchase_number", &format!("V1-PO-{id}"));
            let received = purchase
                .text_or("status", "")
                .eq_ignore_ascii_case("received");
            let total = self.amount_non_negative("purchases", &id, "total", purchase.real("total"));
            let party_id = self.supplier_or_placeholder(purchase.text("supplier_id"))?;
            let supplier_name = self.parties[&party_id].name.clone();
            let created = self.timestamp(&purchase, "created_at");
            let date = self.business_date(&purchase, "date");
            let deleted_at = purchase.flag("is_deleted").then_some(self.now.clone());

            let mut lines = Vec::new();
            for line in lines_by_purchase.remove(&id).unwrap_or_default() {
                let line_id = line.id();
                let qty = self.qty(
                    "purchase_items",
                    &line_id,
                    "quantity",
                    line.real("quantity"),
                );
                if qty <= 0 {
                    self.count_skipped("purchase_items");
                    continue;
                }
                let unit_cost = self.amount_non_negative(
                    "purchase_items",
                    &line_id,
                    "unit_price",
                    line.real("unit_price"),
                );
                let line_total = self.amount_non_negative(
                    "purchase_items",
                    &line_id,
                    "total",
                    line.real("total"),
                );
                let product_key =
                    self.product_or_placeholder(&line.text_or("product_id", "unknown"))?;
                lines.push((line_id, product_key, qty, unit_cost, line_total));
            }
            // Line totals are re-spread so they add up exactly to the header total.
            let parts = Self::spread(total, &lines.iter().map(|l| l.4).collect::<Vec<_>>());

            if received {
                self.report.totals.legacy_purchase_total += purchase.real("total");
                let series_id = self.series["purchase_bill"].clone();
                let seq = self.doc_seq(&series_id, &doc_no);
                self.db.execute(
                    "INSERT INTO purchase_bills (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no,
                        business_date, status, party_id, supplier_name, receives_stock, gross, taxable, total,
                        snapshot_source, created_at, updated_at, deleted_at)
                     VALUES (?1, ?2, ?3, ?4, 'all', ?5, ?6, ?7, 'draft', ?8, ?9, 1, ?10, ?10, ?10, 'migrated', ?11, ?11, ?12)",
                    params![id, self.branch_id, self.warehouse_id, series_id, seq, doc_no, date, party_id, supplier_name, total, created, deleted_at],
                )?;
                for (index, (line_id, product_key, qty, unit_cost, _)) in lines.iter().enumerate() {
                    let product = &self.products[product_key];
                    self.db.execute(
                        "INSERT INTO purchase_bill_items (id, bill_id, line_no, variant_id, unit_id, item_name, sku, qty, base_qty,
                            unit_cost, taxable, line_total, landed_unit_cost, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?10, ?9, ?11, ?11)",
                        params![line_id, id, index + 1, product.variant_id, product.unit_id, product.name, product.sku, qty, unit_cost, parts[index], created],
                    )?;
                    self.count_written("purchase_items", "purchase_bill_items");
                }
                self.db.execute(
                    "UPDATE purchase_bills SET status = 'finalized', finalized_at = ?2 WHERE id = ?1",
                    params![id, created],
                )?;
                self.post_party_ledger(
                    &party_id,
                    &date,
                    "purchase_bill",
                    ("purchase_bill", &id),
                    0,
                    total,
                    &format!("Purchase {doc_no}"),
                )?;
                self.count_written("purchases", "purchase_bills (received)");
            } else {
                self.db.execute(
                    "INSERT INTO purchase_orders (id, branch_id, warehouse_id, series_id, number_period, doc_no, business_date,
                        status, party_id, supplier_name, gross, taxable, total, created_at, updated_at, deleted_at)
                     VALUES (?1, ?2, ?3, ?4, 'all', ?5, ?6, 'open', ?7, ?8, ?9, ?9, ?9, ?10, ?10, ?11)",
                    params![id, self.branch_id, self.warehouse_id, self.series["purchase_order"], doc_no, date, party_id, supplier_name, total, created, deleted_at],
                )?;
                for (index, (line_id, product_key, qty, unit_cost, _)) in lines.iter().enumerate() {
                    let product = &self.products[product_key];
                    self.db.execute(
                        "INSERT INTO purchase_order_items (id, purchase_order_id, line_no, variant_id, unit_id, item_name, sku, qty,
                            base_qty, unit_cost, taxable, line_total, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?10, ?11, ?11)",
                        params![line_id, id, index + 1, product.variant_id, product.unit_id, product.name, product.sku, qty, unit_cost, parts[index], created],
                    )?;
                    self.count_written("purchase_items", "purchase_order_items");
                }
                self.count_written("purchases", "purchase_orders (not received)");
            }
        }
        Ok(())
    }

    /// v1 "payments": money received from customers / paid to suppliers on account.
    fn legacy_payments(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "payments")?;
        self.count_read("payments", rows.len());
        for row in rows {
            let id = row.id();
            let amount = self.amount("payments", &id, "amount", row.real("amount"));
            if amount <= 0 {
                self.report.warn(format!(
                    "payment {id}: amount {} is not positive; skipped",
                    row.real("amount")
                ));
                self.count_skipped("payments");
                continue;
            }
            let incoming = row
                .text_or("type", "RECEIVE")
                .eq_ignore_ascii_case("RECEIVE");
            let party_id = row
                .text("entity_id")
                .filter(|p| self.parties.contains_key(p));
            if party_id.is_none() {
                self.report.warn(format!(
                    "payment {id}: party not found; migrated without a party"
                ));
            }
            let mode = match row.text_or("payment_method", "").to_lowercase().as_str() {
                m @ ("cash" | "card" | "upi" | "cheque") => m.to_string(),
                "bank" | "bank transfer" | "bank_transfer" | "neft" | "imps" | "rtgs" => {
                    "bank_transfer".into()
                }
                _ => "other".into(),
            };
            let date = self.business_date(&row, "date");
            let doc_no = self.next_payment_no(if incoming { "V1-RCT" } else { "V1-PAY" });
            self.db.execute(
                "INSERT INTO payments (id, branch_id, series_id, doc_no, business_date, direction, party_id, account_id, mode,
                    amount, reference_no, notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'On account (v1)', ?12, ?12)",
                params![
                    id, self.branch_id, self.series[if incoming { "payment_in" } else { "payment_out" }], doc_no, date,
                    if incoming { "in" } else { "out" }, party_id, ACCOUNT_CASH, mode, amount, row.text("reference_no"),
                    self.timestamp(&row, "created_at")
                ],
            )?;
            if let Some(party) = &party_id {
                let (debit, credit, entry) = if incoming {
                    (0, amount, "payment_in")
                } else {
                    (amount, 0, "payment_out")
                };
                self.post_party_ledger(
                    party,
                    &date,
                    entry,
                    ("payment", &id),
                    debit,
                    credit,
                    "Payment on account (v1)",
                )?;
            }
            self.count_written("payments", "payments");
        }
        Ok(())
    }

    fn expenses(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "expenses")?;
        self.count_read("expenses", rows.len());
        let mut categories: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self
                .db
                .prepare("SELECT lower(name), id FROM expense_categories")?;
            for pair in
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            {
                let (name, id) = pair?;
                categories.insert(name, id);
            }
        }
        for (index, row) in rows.into_iter().enumerate() {
            let id = row.id();
            let amount = self.amount("expenses", &id, "amount", row.real("amount"));
            if amount <= 0 {
                self.report.warn(format!(
                    "expense {id}: amount {} is not positive; skipped",
                    row.real("amount")
                ));
                self.count_skipped("expenses");
                continue;
            }
            let category_name = row.text_or("category", "General");
            let category_id = if let Some(existing) = categories.get(&category_name.to_lowercase())
            {
                existing.clone()
            } else {
                // Same account v1 posted to: rent → 5100, everything else → 5200.
                let account = if category_name.to_lowercase().contains("rent") {
                    ACCOUNT_RENT
                } else {
                    ACCOUNT_UTILITIES
                };
                let new_category = new_id();
                self.db.execute(
                    "INSERT INTO expense_categories (id, name, account_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                    params![new_category, category_name, account, self.now],
                )?;
                categories.insert(category_name.to_lowercase(), new_category.clone());
                new_category
            };
            let date = self.business_date(&row, "date");
            let created = self.timestamp(&row, "created_at");
            self.db.execute(
                "INSERT INTO expenses (id, branch_id, series_id, doc_no, business_date, category_id, title, description,
                    amount, total, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10)",
                params![
                    id, self.branch_id, self.series["expense"], format!("V1-EXP-{:06}", index + 1), date, category_id,
                    row.text_or("title", &category_name), row.text("description"), amount, created
                ],
            )?;
            // v1 expenses were paid in cash when entered.
            let payment_id = new_id();
            let doc_no = self.next_payment_no("V1-PAY");
            self.db.execute(
                "INSERT INTO payments (id, branch_id, series_id, doc_no, business_date, direction, account_id, mode, amount,
                    notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'out', ?6, 'cash', ?7, 'Expense paid in cash (v1)', ?8, ?8)",
                params![payment_id, self.branch_id, self.series["payment_out"], doc_no, date, ACCOUNT_CASH, amount, created],
            )?;
            self.db.execute(
                "INSERT INTO payment_allocations (id, payment_id, target_type, target_id, amount, created_at)
                 VALUES (?1, ?2, 'expense', ?3, ?4, ?5)",
                params![new_id(), payment_id, id, amount, created],
            )?;
            self.report.totals.legacy_expense_total += row.real("amount");
            self.count_written("expenses", "expenses (+ cash payments)");
        }
        Ok(())
    }

    fn journals(&mut self) -> Result<()> {
        let entries = read_table(self.legacy, "journal_entries")?;
        let lines = read_table(self.legacy, "journal_lines")?;
        self.count_read("journal_entries", entries.len());
        self.count_read("journal_lines", lines.len());
        let mut lines_by_entry: HashMap<String, Vec<LegacyRow>> = HashMap::new();
        for line in lines {
            lines_by_entry
                .entry(line.text_or("journal_entry_id", ""))
                .or_default()
                .push(line);
        }
        for entry in entries {
            let id = entry.id();
            self.db.execute(
                "INSERT INTO journal_entries (id, business_date, source_type, source_id, narration, created_at)
                 VALUES (?1, ?2, 'legacy', ?3, ?4, ?5)",
                params![
                    id, self.business_date(&entry, "entry_date"), entry.text("reference"),
                    entry.text_or("description", "Migrated from v1"), self.timestamp(&entry, "created_at")
                ],
            )?;
            self.count_written("journal_entries", "journal_entries");
            let mut line_no = 0;
            for line in lines_by_entry.remove(&id).unwrap_or_default() {
                let line_id = line.id();
                let Some(account) = line
                    .text("account_id")
                    .and_then(|a| self.accounts.get(&a))
                    .cloned()
                else {
                    self.report.warn(format!(
                        "journal line {line_id}: account not found; skipped"
                    ));
                    self.count_skipped("journal_lines");
                    continue;
                };
                let debit = self.amount("journal_lines", &line_id, "debit", line.real("debit"));
                let credit = self.amount("journal_lines", &line_id, "credit", line.real("credit"));
                // v2 lines are one-sided and non-negative; split or flip v1 lines as needed.
                let net = debit - credit;
                if net == 0 {
                    self.count_skipped("journal_lines");
                    continue;
                }
                line_no += 1;
                self.db.execute(
                    "INSERT INTO journal_lines (id, entry_id, line_no, account_id, debit, credit, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![line_id, id, line_no, account, net.max(0), (-net).max(0), self.now],
                )?;
                self.count_written("journal_lines", "journal_lines");
            }
        }
        Ok(())
    }

    fn audit_log(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "audit_logs")?;
        self.count_read("audit_logs", rows.len());
        let as_json = |value: Option<String>| -> Option<String> {
            value.map(|v| {
                if serde_json::from_str::<Value>(&v).is_ok() {
                    v
                } else {
                    Value::String(v).to_string()
                }
            })
        };
        for row in rows {
            self.db.execute(
                "INSERT INTO audit_log (id, occurred_at, user_id, action, entity_type, entity_id, summary, before_json, after_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Migrated from v1', ?7, ?8)",
                params![
                    row.id(), self.timestamp(&row, "timestamp"), self.user_ref(row.text("user_id")),
                    row.text_or("action", "UNKNOWN"), row.text_or("table_name", "unknown"), row.text("record_id"),
                    as_json(row.text("old_values")), as_json(row.text("new_values"))
                ],
            )?;
            self.count_written("audit_logs", "audit_log");
        }
        Ok(())
    }

    fn held_orders(&mut self) -> Result<()> {
        let rows = read_table(self.legacy, "held_orders")?;
        self.count_read("held_orders", rows.len());
        for row in rows {
            let items = row.text_or("items", "[]");
            let cart = if serde_json::from_str::<Value>(&items).is_ok() {
                items
            } else {
                "[]".to_string()
            };
            let created = self.timestamp(&row, "created_at");
            self.db.execute(
                "INSERT INTO held_bills (id, label, cart_json, cart_format, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'legacy-v1', ?4, ?4)",
                params![
                    new_id(),
                    row.text_or("customer_name", "Held bill"),
                    cart,
                    created
                ],
            )?;
            self.count_written("held_orders", "held_bills");
        }
        Ok(())
    }

    /// Tables kept only in legacy_records (and sessions, dropped on purpose).
    fn record_unmapped_tables(&mut self) -> Result<()> {
        let adjustments = read_table(self.legacy, "stock_adjustments")?.len();
        self.count_read("stock_adjustments", adjustments);
        let counts = self.report.table("stock_adjustments");
        counts.migrated_rows = adjustments;
        counts.migrated_to = vec![
            "legacy_records (history only; current stock comes from the opening balance)".into(),
        ];

        let sessions = read_table(self.legacy, "sessions")?.len();
        self.count_read("sessions", sessions);
        let counts = self.report.table("sessions");
        counts.skipped_rows = sessions;
        counts.migrated_to = vec!["dropped: everyone signs in again after the upgrade".into()];
        Ok(())
    }

    /// v2 balances must equal what v1 showed. Differences (v1 bugs, manual edits) become a
    /// flagged opening-balance entry and are listed in the report.
    fn reconcile_party_balances(&mut self) -> Result<()> {
        for (table, column, sign) in [("customers", "due_amount", 1), ("suppliers", "balance", -1)]
        {
            for row in read_table(self.legacy, table)? {
                let id = row.id();
                let legacy_value = row.real(column);
                let target = sign * self.amount(table, &id, column, legacy_value);
                let (current, name): (i64, String) = self.db.query_row(
                    "SELECT balance, name FROM parties WHERE id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let adjustment = target - current;
                if adjustment != 0 {
                    self.post_party_ledger(
                        &id,
                        &self.today,
                        "opening",
                        ("legacy_migration", &id),
                        adjustment.max(0),
                        (-adjustment).max(0),
                        "Balance carried over from v1 (difference vs. migrated documents)",
                    )?;
                    self.report.balance_adjustments.push(BalanceAdjustment {
                        party_id: id,
                        name,
                        party_type: table.trim_end_matches('s').into(),
                        legacy_balance: legacy_value,
                        from_documents_minor: current,
                        adjustment_minor: adjustment,
                    });
                }
            }
        }
        Ok(())
    }

    fn verify(&mut self) -> Result<()> {
        let stock_mismatches: i64 =
            self.db
                .query_row("SELECT count(*) FROM v_stock_level_mismatches", [], |r| {
                    r.get(0)
                })?;
        let party_mismatches: i64 =
            self.db
                .query_row("SELECT count(*) FROM v_party_balance_mismatches", [], |r| {
                    r.get(0)
                })?;
        if stock_mismatches > 0 || party_mismatches > 0 {
            return Err(MigrateError::Verification(format!(
                "{stock_mismatches} stock and {party_mismatches} party balance cache mismatch(es)"
            )));
        }
        {
            let mut stmt = self
                .db
                .prepare("SELECT entry_id FROM v_unbalanced_journal_entries ORDER BY entry_id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            for row in rows {
                self.report.unbalanced_journal_entries.push(row?);
            }
        }
        if !self.report.unbalanced_journal_entries.is_empty() {
            self.report.warn(format!(
                "{} journal entr(ies) from v1 were already unbalanced; kept as they were (see unbalanced_journal_entries)",
                self.report.unbalanced_journal_entries.len()
            ));
        }
        let totals = &mut self.report.totals;
        totals.sales_total_minor = self.db.query_row(
            "SELECT coalesce(sum(total), 0) FROM sales_invoices WHERE status = 'finalized'",
            [],
            |r| r.get(0),
        )?;
        totals.purchase_total_minor = self.db.query_row(
            "SELECT coalesce(sum(total), 0) FROM purchase_bills WHERE status = 'finalized'",
            [],
            |r| r.get(0),
        )?;
        totals.stock_value_minor = self.db.query_row(
            "SELECT coalesce(sum(value), 0) FROM stock_levels",
            [],
            |r| r.get(0),
        )?;
        totals.expense_total_minor = self.db.query_row(
            "SELECT coalesce(sum(total), 0) FROM expenses WHERE status = 'finalized'",
            [],
            |r| r.get(0),
        )?;
        Ok(())
    }
}
