//! Structured logging to daily-rotated files (14 kept) in the app's log folder.
//!
//! Never log request inputs: they can contain passwords, PINs, tokens or customer data. Commands
//! log the command name, the user id, the duration and the error code only.

use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Installs the global subscriber once. Safe to call again (later calls are ignored).
pub fn init(log_dir: &Path) {
    if GUARD.get().is_some() {
        return;
    }
    let appender = match Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("app")
        .filename_suffix("log")
        .max_log_files(14)
        .build(log_dir)
    {
        Ok(appender) => appender,
        Err(error) => {
            eprintln!("file logging disabled: {error}");
            return;
        }
    };
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("APP_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let installed = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .try_init()
        .is_ok();
    if installed {
        let _ = GUARD.set(guard);
    }
}
