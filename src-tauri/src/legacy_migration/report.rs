//! The migration report: what was moved, what was converted with rounding, what didn't add up,
//! and every warning. Written as JSON next to the backup for the owner (or support) to review.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct MigrationReport {
    pub started_at: String,
    pub finished_at: String,
    pub dry_run: bool,
    pub legacy_path: String,
    /// SHA-256 of the backup copy (SQLite backups are logically, not byte-for-byte, identical to v1).
    pub backup_sha256: String,
    pub backup_path: String,
    /// None for a dry run (the trial database is deleted afterwards).
    pub target_path: Option<String>,
    pub schema_version: i64,
    /// Per v1 table: rows read, rows written (and where), rows skipped.
    pub tables: BTreeMap<String, TableCounts>,
    pub totals: Totals,
    pub lossy_amounts: Vec<LossyAmount>,
    pub balance_adjustments: Vec<BalanceAdjustment>,
    pub negative_stock: Vec<NegativeStock>,
    pub unbalanced_journal_entries: Vec<String>,
    pub users_requiring_password_change: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct TableCounts {
    pub legacy_rows: usize,
    pub migrated_rows: usize,
    pub skipped_rows: usize,
    pub migrated_to: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Totals {
    /// Sum of v1 invoice totals, excluding cancelled ones (major units, as stored in v1).
    pub legacy_sales_total: f64,
    /// Sum of finalized v2 invoice totals (minor units).
    pub sales_total_minor: i64,
    pub legacy_purchase_total: f64,
    pub purchase_total_minor: i64,
    /// Σ stock × cost in v1 (major units) vs Σ stock_levels.value in v2 (minor units).
    pub legacy_stock_value: f64,
    pub stock_value_minor: i64,
    pub legacy_expense_total: f64,
    pub expense_total_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct LossyAmount {
    pub table: String,
    pub id: String,
    pub column: String,
    pub legacy_value: f64,
    pub migrated_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct BalanceAdjustment {
    pub party_id: String,
    pub name: String,
    pub party_type: String,
    /// Balance v1 showed (customer due, or supplier balance), major units.
    pub legacy_balance: f64,
    /// Balance rebuilt from the migrated documents and payments (minor, +ve = owes us).
    pub from_documents_minor: i64,
    /// Opening-balance entry posted so the v2 balance equals what v1 showed.
    pub adjustment_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct NegativeStock {
    pub variant_id: String,
    pub sku: String,
    pub qty_milli: i64,
}

impl MigrationReport {
    pub fn table(&mut self, legacy_table: &str) -> &mut TableCounts {
        self.tables.entry(legacy_table.to_string()).or_default()
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}
