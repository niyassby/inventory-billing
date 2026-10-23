//! Versioned, checksummed schema migrations.
//!
//! Rules: a migration file is never edited once released (its checksum is recorded and verified
//! on every start); changes go in a new numbered file. Each migration runs in its own IMMEDIATE
//! transaction, so a failure leaves the database at the previous version.

use std::time::Instant;

use rusqlite::{params, Connection, TransactionBehavior};
use sha2::{Digest, Sha256};

use super::DbError;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "core",
        sql: include_str!("migrations/0001_core.sql"),
    },
    Migration {
        version: 2,
        name: "catalog_stock",
        sql: include_str!("migrations/0002_catalog_stock.sql"),
    },
    Migration {
        version: 3,
        name: "parties_sales",
        sql: include_str!("migrations/0003_parties_sales.sql"),
    },
    Migration {
        version: 4,
        name: "purchases",
        sql: include_str!("migrations/0004_purchases.sql"),
    },
    Migration {
        version: 5,
        name: "accounting_cash",
        sql: include_str!("migrations/0005_accounting_cash.sql"),
    },
    Migration {
        version: 6,
        name: "seed_system",
        sql: include_str!("migrations/0006_seed_system.sql"),
    },
    Migration {
        version: 7,
        name: "credit_override_permission",
        sql: include_str!("migrations/0007_credit_override_permission.sql"),
    },
    Migration {
        version: 8,
        name: "grni_account",
        sql: include_str!("migrations/0008_grni_account.sql"),
    },
    Migration {
        version: 9,
        name: "hub_sync_backup",
        sql: include_str!("migrations/0009_hub_sync_backup.sql"),
    },
    Migration {
        version: 10,
        name: "printing",
        sql: include_str!("migrations/0010_printing.sql"),
    },
    Migration {
        version: 11,
        name: "inventory_depth",
        sql: include_str!("migrations/0011_inventory_depth.sql"),
    },
    Migration {
        version: 12,
        name: "accounting_depth",
        sql: include_str!("migrations/0012_accounting_depth.sql"),
    },
];

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// SHA-256 of the migration text with line endings normalised, so a CRLF checkout on Windows
/// doesn't look like an edited migration.
pub fn checksum(sql: &str) -> String {
    let normalized = sql.replace("\r\n", "\n");
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationOutcome {
    pub applied: Vec<i64>,
    pub version: i64,
}

pub fn current_version(conn: &Connection) -> Result<i64, DbError> {
    ensure_table(conn)?;
    Ok(conn.query_row(
        "SELECT coalesce(max(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )?)
}

fn ensure_table(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version      INTEGER PRIMARY KEY,
            name         TEXT NOT NULL,
            checksum     TEXT NOT NULL,
            applied_at   TEXT NOT NULL,
            duration_ms  INTEGER NOT NULL
        ) STRICT;",
    )?;
    Ok(())
}

/// Brings the database up to the latest schema. Safe to call on every start.
pub fn run(conn: &mut Connection) -> Result<MigrationOutcome, DbError> {
    run_migrations(conn, MIGRATIONS)
}

pub(crate) fn run_migrations(
    conn: &mut Connection,
    migrations: &[Migration],
) -> Result<MigrationOutcome, DbError> {
    ensure_table(conn)?;

    let applied: Vec<(i64, String, String)> = {
        let mut stmt =
            conn.prepare("SELECT version, name, checksum FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };

    let supported = migrations.last().map_or(0, |m| m.version);
    for (version, name, recorded) in &applied {
        match migrations.iter().find(|m| m.version == *version) {
            Some(m) if checksum(m.sql) == *recorded => {}
            Some(_) => {
                return Err(DbError::ChecksumMismatch {
                    version: *version,
                    name: name.clone(),
                })
            }
            None if *version > supported => {
                return Err(DbError::NewerThanApp {
                    found: *version,
                    supported,
                })
            }
            None => return Err(DbError::UnknownMigration(*version)),
        }
    }

    let mut outcome = MigrationOutcome::default();
    for m in migrations
        .iter()
        .filter(|m| !applied.iter().any(|(v, _, _)| *v == m.version))
    {
        let started = Instant::now();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(m.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                m.version,
                m.name,
                checksum(m.sql),
                chrono::Utc::now().to_rfc3339(),
                i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
            ],
        )?;
        tx.commit()?;
        outcome.applied.push(m.version);
    }

    let violations = {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let rows = stmt.query_map([], |_| Ok(()))?;
        rows.count()
    };
    if violations > 0 {
        return Err(DbError::ForeignKeyViolations(violations));
    }

    outcome.version = current_version(conn)?;
    Ok(outcome)
}
