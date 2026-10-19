//! The v2 database: connection setup and versioned migrations (docs/DATABASE.md). The app uses
//! only this database; the v1 file is read once, by the first-run import (services::upgrade).

pub mod migrate;

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration {version} ({name}) was changed after it was applied (checksum mismatch)")]
    ChecksumMismatch { version: i64, name: String },
    #[error("database schema version {found} is newer than this app supports ({supported})")]
    NewerThanApp { found: i64, supported: i64 },
    #[error("database has an unknown migration version {0}")]
    UnknownMigration(i64),
    #[error("foreign key check failed after migration: {0} violation(s)")]
    ForeignKeyViolations(usize),
}

/// Opens (creating if needed) a v2 database file with the standard connection settings.
pub fn open(path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;
    configure(&conn, true)?;
    Ok(conn)
}

/// In-memory database with the same settings (WAL does not apply). Used by tests.
pub fn open_in_memory() -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    configure(&conn, false)?;
    Ok(conn)
}

/// Settings every connection must have. Foreign keys are per-connection in SQLite, so this runs
/// on every open, not once.
pub(crate) fn configure(conn: &Connection, wal: bool) -> Result<(), DbError> {
    if wal {
        // WAL lets the hub serve readers while one writer commits (docs/ARCHITECTURE.md §5).
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        debug_assert_eq!(mode.to_lowercase(), "wal");
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
    }
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
    )?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(())
}

#[cfg(test)]
mod tests;
