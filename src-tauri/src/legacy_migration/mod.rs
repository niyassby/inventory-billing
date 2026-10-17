//! One-way migration of a v1 database into a new v2 database (docs/DATABASE.md §7).
//!
//! Safety rules (Master Prompt §16):
//! * the v1 file is never written: it is copied with SQLite's online backup API, the copy is
//!   integrity-checked and checksummed, and only the copy is read;
//! * the v2 database is a new file; the migration refuses to overwrite an existing one;
//! * everything is written in one transaction and verified before commit, so a failure leaves
//!   no half-migrated database behind;
//! * a JSON report lists counts, rounding, balance differences and warnings.

mod etl;
pub mod reader;
pub mod report;

#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::db::{self, migrate, DbError};
pub use report::MigrationReport;

#[derive(Debug, Clone)]
pub struct MigrateOptions {
    /// The v1 database (`inventory_app.db`).
    pub legacy_path: PathBuf,
    /// Where to create the v2 database. Must not exist. Ignored for a dry run.
    pub target_path: PathBuf,
    /// Backups and reports go here.
    pub backup_dir: PathBuf,
    /// Run everything against a throw-away database and only write the report.
    pub dry_run: bool,
    /// Minor-unit digits of the business currency (2 for INR/USD, 3 for KWD, 0 for JPY).
    pub currency_minor_digits: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("v1 database not found at {0}")]
    LegacyNotFound(PathBuf),
    #[error("refusing to overwrite existing database at {0}")]
    TargetExists(PathBuf),
    #[error("the v1 database failed its integrity check: {0}")]
    IntegrityCheckFailed(String),
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("the v1 data needs fixing before it can be migrated: {0}")]
    NeedsAttention(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct MigrationOutcome {
    pub report: MigrationReport,
    pub report_path: PathBuf,
}

pub fn migrate_legacy(options: &MigrateOptions) -> Result<MigrationOutcome, MigrateError> {
    if !options.legacy_path.is_file() {
        return Err(MigrateError::LegacyNotFound(options.legacy_path.clone()));
    }
    if !options.dry_run && options.target_path.exists() {
        // The app creates an empty v2 database on start; that (and only that) may be replaced.
        if is_unused_v2(&options.target_path)? {
            remove_database_files(&options.target_path);
        } else {
            return Err(MigrateError::TargetExists(options.target_path.clone()));
        }
    }
    fs::create_dir_all(&options.backup_dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();

    let mut report = MigrationReport {
        started_at: chrono::Utc::now().to_rfc3339(),
        dry_run: options.dry_run,
        legacy_path: options.legacy_path.display().to_string(),
        ..MigrationReport::default()
    };

    // 1. Back up v1 and verify the copy. From here on only the copy is read.
    let backup_path = unique_path(&options.backup_dir, &format!("pre-v2-{stamp}"), "db");
    backup(&options.legacy_path, &backup_path)?;
    report.backup_path = backup_path.display().to_string();
    report.backup_sha256 = sha256_file(&backup_path)?;
    let legacy = Connection::open_with_flags(&backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = legacy.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(MigrateError::IntegrityCheckFailed(integrity));
    }

    // 2. Build v2 in a new file; remove it again if anything fails.
    let target = if options.dry_run {
        unique_path(&options.backup_dir, &format!("dry-run-{stamp}"), "db")
    } else {
        options.target_path.clone()
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let result = build_v2(&target, &legacy, &mut report, options.currency_minor_digits);
    drop(legacy);
    if let Err(error) = result {
        remove_database_files(&target);
        return Err(error);
    }
    if options.dry_run {
        remove_database_files(&target);
    } else {
        report.target_path = Some(target.display().to_string());
    }

    report.finished_at = chrono::Utc::now().to_rfc3339();
    let report_path = unique_path(
        &options.backup_dir,
        &format!("migration-report-{stamp}"),
        "json",
    );
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    Ok(MigrationOutcome {
        report,
        report_path,
    })
}

fn build_v2(
    target: &Path,
    legacy: &Connection,
    report: &mut MigrationReport,
    minor_digits: u32,
) -> Result<(), MigrateError> {
    let mut conn = db::open(target)?;
    migrate::run(&mut conn)?;
    report.schema_version = migrate::current_version(&conn)?;

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    etl::Etl::new(&tx, legacy, report, minor_digits).run()?;
    tx.commit()?;

    let fk_violations = conn
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |_| Ok(()))?
        .count();
    if fk_violations > 0 {
        return Err(MigrateError::Verification(format!(
            "{fk_violations} foreign key violation(s)"
        )));
    }
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(MigrateError::Verification(format!(
            "new database integrity check: {integrity}"
        )));
    }
    // Fold the WAL back into the main file so the result is a single, self-contained file.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

/// True for a v2 database that was created but never used: migrated schema, no users, no branches,
/// no documents. Anything else is real data and must not be overwritten.
fn is_unused_v2(path: &Path) -> Result<bool, MigrateError> {
    // Anything that cannot be read as SQLite (or lacks the tables) is treated as data to keep.
    Ok(unused_v2_check(path).unwrap_or(false))
}

fn unused_v2_check(path: &Path) -> rusqlite::Result<bool> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_schema: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('schema_migrations', 'users', 'branches', 'products', 'sales_invoices')",
        [],
        |r| r.get(0),
    )?;
    if has_schema < 5 {
        return Ok(false);
    }
    let rows: i64 = conn.query_row(
        "SELECT (SELECT count(*) FROM users) + (SELECT count(*) FROM branches) + (SELECT count(*) FROM products) + (SELECT count(*) FROM sales_invoices)",
        [],
        |r| r.get(0),
    )?;
    Ok(rows == 0)
}

fn backup(source: &Path, destination: &Path) -> Result<(), MigrateError> {
    let src = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut dst = Connection::open(destination)?;
    let backup = rusqlite::backup::Backup::new(&src, &mut dst)?;
    backup.run_to_completion(256, std::time::Duration::ZERO, None)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, MigrateError> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}

/// `dir/stem.ext`, or `dir/stem-2.ext`… if that already exists (two runs in the same second).
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    candidate
}

fn remove_database_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        let _ = fs::remove_file(PathBuf::from(file));
    }
}
