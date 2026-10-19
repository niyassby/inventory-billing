//! First start after the upgrade: bring the v1 data (`inventory_app.db`) into the v2 database
//! (docs/DATABASE.md §7, ASSUMPTIONS A-34/A-37).
//!
//! Only while v2 is still unused. The v1 file is never written: `legacy_migration` works on a
//! verified backup copy and builds v2 in a new file, which is then copied into the open database
//! with SQLite's backup API (the same way a restore works). A dry run shows the report first.

use std::fs;

use rusqlite::DatabaseName;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use crate::app::AppState;
use crate::db;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::legacy_migration::{migrate_legacy, MigrateError, MigrateOptions, MigrationReport};

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportLegacyInput {
    /// Only check and report; change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportLegacyResult {
    pub applied: bool,
    pub report: MigrationReport,
    /// Where the full report (JSON) was saved.
    pub report_path: String,
}

fn remove_db(path: &std::path::Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

pub fn import_legacy(state: &AppState, input: &ImportLegacyInput) -> AppResult<ImportLegacyResult> {
    let mut conn = state.conn()?;
    if !super::setup::needs_setup(&conn)? {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "This PC already has a business set up; the old data can't be imported over it.",
        ));
    }
    let dir = state.data_dir();
    let legacy_path = dir.join("inventory_app.db");
    if !legacy_path.is_file() {
        return Err(AppError::not_found("Data from the previous version"));
    }
    let target = dir.join(format!(".legacy-import-{}.db", new_id()));
    let outcome = migrate_legacy(&MigrateOptions {
        legacy_path,
        target_path: target.clone(),
        backup_dir: dir.join("backups").join("legacy"),
        dry_run: input.dry_run,
        currency_minor_digits: 2,
    })
    .map_err(|e| match e {
        MigrateError::IntegrityCheckFailed(_)
        | MigrateError::Verification(_)
        | MigrateError::NeedsAttention(_) => AppError::validation(e.to_string()),
        other => AppError::internal(other),
    })?;
    if !input.dry_run {
        let applied = (|| {
            conn.restore(
                DatabaseName::Main,
                &target,
                None::<fn(rusqlite::backup::Progress)>,
            )?;
            db::migrate::run(&mut conn)?;
            AuditEntry::new("LEGACY_IMPORTED", "legacy_records", None)
                .summary(format!("report {}", outcome.report_path.display()))
                .write(&conn, &state.now().to_rfc3339())
        })();
        remove_db(&target);
        applied?;
        tracing::info!(
            "v1 data imported; report at {}",
            outcome.report_path.display()
        );
    }
    Ok(ImportLegacyResult {
        applied: !input.dry_run,
        report: outcome.report,
        report_path: outcome.report_path.display().to_string(),
    })
}
