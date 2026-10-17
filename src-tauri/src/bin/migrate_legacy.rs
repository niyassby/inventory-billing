//! migrate-legacy: copy a v1 database into a new v2 database and write a report.
//!
//! Usage:
//!   migrate-legacy [--legacy <v1.db>] [--out <v2.db>] [--backup-dir <dir>] [--dry-run]
//!                  [--currency-digits <n>]
//!
//! Defaults (Windows): --legacy %LOCALAPPDATA%\com.niyas.tauri-app\inventory_app.db,
//! --out inventory_v2.db and --backup-dir backups\ next to it.
//! Run with --dry-run first and read the report before migrating for real.

use std::path::PathBuf;
use std::process::ExitCode;

use tauri_app_lib::legacy_migration::{migrate_legacy, MigrateOptions};

fn usage() -> &'static str {
    "usage: migrate-legacy [--legacy <v1.db>] [--out <v2.db>] [--backup-dir <dir>] [--dry-run] [--currency-digits <n>]"
}

fn default_legacy_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| {
        PathBuf::from(dir)
            .join("com.niyas.tauri-app")
            .join("inventory_app.db")
    })
}

fn parse_args() -> Result<MigrateOptions, String> {
    let mut legacy = None;
    let mut out = None;
    let mut backup_dir = None;
    let mut dry_run = false;
    let mut digits = 2;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a value\n{}", usage()))
        };
        match arg.as_str() {
            "--legacy" => legacy = Some(PathBuf::from(value("--legacy")?)),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--backup-dir" => backup_dir = Some(PathBuf::from(value("--backup-dir")?)),
            "--currency-digits" => {
                digits = value("--currency-digits")?
                    .parse()
                    .map_err(|_| "--currency-digits must be 0, 2 or 3".to_string())?;
            }
            "--dry-run" => dry_run = true,
            "-h" | "--help" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument '{other}'\n{}", usage())),
        }
    }
    let legacy_path = legacy
        .or_else(default_legacy_path)
        .ok_or_else(|| format!("--legacy is required on this system\n{}", usage()))?;
    let dir = legacy_path.parent().map(PathBuf::from).unwrap_or_default();
    Ok(MigrateOptions {
        target_path: out.unwrap_or_else(|| dir.join("inventory_v2.db")),
        backup_dir: backup_dir.unwrap_or_else(|| dir.join("backups")),
        legacy_path,
        dry_run,
        currency_minor_digits: digits,
    })
}

fn main() -> ExitCode {
    let options = match parse_args() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match migrate_legacy(&options) {
        Ok(outcome) => {
            let report = &outcome.report;
            println!(
                "{} complete.",
                if report.dry_run {
                    "Dry run"
                } else {
                    "Migration"
                }
            );
            println!("  v1 backup:   {}", report.backup_path);
            if let Some(target) = &report.target_path {
                println!("  v2 database: {target}");
            }
            println!("  report:      {}", outcome.report_path.display());
            for (table, counts) in &report.tables {
                println!(
                    "  {table:<20} read {:>6}  migrated {:>6}  skipped {:>4}",
                    counts.legacy_rows, counts.migrated_rows, counts.skipped_rows
                );
            }
            println!(
                "  {} rounding adjustment(s), {} balance adjustment(s), {} warning(s)",
                report.lossy_amounts.len(),
                report.balance_adjustments.len(),
                report.warnings.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Migration failed: {error}");
            eprintln!(
                "Nothing was changed: the v1 database is untouched and no v2 database was kept."
            );
            ExitCode::FAILURE
        }
    }
}
