//! Background jobs on the hub and on standalone PCs (docs/ARCHITECTURE.md §8). A thread wakes up
//! every minute and runs whatever is due; `tick` is the whole scheduler, so tests drive it with a
//! fixed clock. Each job's last run is kept in `job_runs`; a failing job never stops the others.

use std::time::Duration;

use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::app::AppState;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};
use crate::services::backup;
use crate::services::settings::{self, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Backup,
    LowStock,
    Integrity,
    Cleanup,
}

impl Job {
    pub const ALL: [Job; 4] = [Job::Backup, Job::LowStock, Job::Integrity, Job::Cleanup];

    pub fn name(self) -> &'static str {
        match self {
            Job::Backup => "backup",
            Job::LowStock => "low_stock",
            Job::Integrity => "integrity",
            Job::Cleanup => "cleanup",
        }
    }
}

fn last_run(conn: &Connection, job: Job) -> AppResult<Option<DateTime<Utc>>> {
    let at: Option<String> = conn
        .query_row(
            "SELECT last_run_at FROM job_runs WHERE job = ?1",
            [job.name()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(at
        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .map(|t| t.with_timezone(&Utc)))
}

fn record(
    conn: &Connection,
    job: Job,
    now: DateTime<Utc>,
    result: &AppResult<String>,
) -> AppResult<()> {
    let (status, error) = match result {
        Ok(_) => ("ok", None),
        Err(e) => ("failed", Some(e.message.clone())),
    };
    conn.execute(
        "INSERT INTO job_runs (job, last_run_at, last_status, last_error) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (job) DO UPDATE SET last_run_at = excluded.last_run_at, last_status = excluded.last_status,
           last_error = excluded.last_error",
        params![job.name(), now.to_rfc3339(), status, error],
    )?;
    Ok(())
}

/// The most recent daily backup slot at or before `now` (business timezone).
fn daily_slot(settings: &Settings, now: DateTime<Utc>) -> DateTime<Utc> {
    let tz = settings.timezone();
    let time = NaiveTime::parse_from_str(&settings.backup.daily_at, "%H:%M")
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(21, 0, 0).unwrap_or_default());
    let local_today = now.with_timezone(&tz).date_naive();
    let slot = |day: chrono::NaiveDate| {
        tz.from_local_datetime(&day.and_time(time))
            .earliest()
            .map(|t| t.with_timezone(&Utc))
    };
    match slot(local_today) {
        Some(s) if s <= now => s,
        _ => slot(local_today - chrono::Duration::days(1)).unwrap_or(now),
    }
}

pub fn is_due(
    job: Job,
    settings: &Settings,
    last: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    let since = |d: chrono::Duration| last.is_none_or(|l| now - l >= d);
    match job {
        Job::Backup => {
            if !settings.backup.enabled {
                return false;
            }
            let Some(last) = last else { return true };
            let daily = last < daily_slot(settings, now);
            let interval = settings.backup.interval_hours > 0
                && now - last >= chrono::Duration::hours(settings.backup.interval_hours);
            daily || interval
        }
        Job::LowStock => settings.inventory.low_stock_alerts && since(chrono::Duration::hours(1)),
        Job::Integrity => since(chrono::Duration::days(7)),
        Job::Cleanup => since(chrono::Duration::days(1)),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "one notice row, written in one place"
)]
fn notify(
    conn: &Connection,
    kind: &str,
    severity: &str,
    title: &str,
    body: &str,
    key: &str,
    entity: Option<(&str, &str)>,
    now: &str,
) -> AppResult<bool> {
    let inserted = conn.execute(
        "INSERT INTO notifications (id, kind, severity, title, body, entity_type, entity_id, dedupe_key, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT DO NOTHING",
        params![new_id(), kind, severity, title, body, entity.map(|e| e.0), entity.map(|e| e.1), key, now],
    )?;
    Ok(inserted > 0)
}

/// Raises a notice for each variant at or below its reorder level, and closes notices for
/// variants that have been restocked.
fn low_stock(conn: &Connection, now: DateTime<Utc>) -> AppResult<String> {
    let now_str = now.to_rfc3339();
    let low: Vec<(String, String, i64, i64)> = conn
        .prepare(
            "SELECT v.id, p.name || coalesce(' — ' || v.variant_name, ''), coalesce(sum(l.qty), 0) AS qty, v.reorder_level
             FROM product_variants v JOIN products p ON p.id = v.product_id
             LEFT JOIN stock_levels l ON l.variant_id = v.id
             WHERE v.reorder_level > 0 AND v.is_active = 1 AND v.deleted_at IS NULL AND p.deleted_at IS NULL AND p.is_service = 0
             GROUP BY v.id HAVING qty <= v.reorder_level",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut raised = 0;
    for (variant_id, name, qty, level) in &low {
        let body = format!(
            "{} left (reorder level {}).",
            *qty as f64 / 1000.0,
            *level as f64 / 1000.0
        );
        if notify(
            conn,
            "low_stock",
            "warning",
            &format!("Low stock: {name}"),
            &body,
            &format!("low_stock:{variant_id}"),
            Some(("product_variants", variant_id)),
            &now_str,
        )? {
            raised += 1;
        }
    }
    let ids: Vec<&str> = low.iter().map(|l| l.0.as_str()).collect();
    let open: Vec<String> = conn
        .prepare(
            "SELECT entity_id FROM notifications WHERE kind = 'low_stock' AND read_at IS NULL",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut cleared = 0;
    for id in open.iter().filter(|id| !ids.contains(&id.as_str())) {
        cleared += conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE kind = 'low_stock' AND entity_id = ?2 AND read_at IS NULL",
            params![now_str, id],
        )?;
    }
    Ok(format!(
        "{} low, {raised} new, {cleared} cleared",
        low.len()
    ))
}

/// Weekly: SQLite's own check plus the ledger-vs-cache views. Problems are reported, never
/// silently "fixed".
fn integrity(conn: &Connection, now: DateTime<Utc>) -> AppResult<String> {
    let quick: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    let mut problems = Vec::new();
    if quick != "ok" {
        problems.push(format!("database: {quick}"));
    }
    for (view, label) in [
        (
            "v_stock_level_mismatches",
            "stock levels differ from the stock ledger",
        ),
        (
            "v_party_balance_mismatches",
            "party balances differ from their ledger",
        ),
        ("v_unbalanced_journal_entries", "unbalanced journal entries"),
    ] {
        let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {view}"), [], |r| r.get(0))?;
        if n > 0 {
            problems.push(format!("{n} {label}"));
        }
    }
    if problems.is_empty() {
        conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE dedupe_key = 'integrity' AND read_at IS NULL",
            [now.to_rfc3339()],
        )?;
        return Ok("ok".into());
    }
    let body = problems.join("; ");
    notify(
        conn,
        "integrity",
        "error",
        "Data check found problems",
        &body,
        "integrity",
        None,
        &now.to_rfc3339(),
    )?;
    Err(AppError::internal(format!("integrity check: {body}")))
}

/// Daily: forget expired sessions, old idempotency keys and used pairing codes.
fn cleanup(conn: &Connection, now: DateTime<Utc>) -> AppResult<String> {
    let month_ago = (now - chrono::Duration::days(30)).to_rfc3339();
    let now_str = now.to_rfc3339();
    let sessions = conn.execute(
        "DELETE FROM sessions WHERE expires_at < ?1 OR revoked_at < ?1",
        [&month_ago],
    )?;
    let keys = conn.execute(
        "DELETE FROM idempotency_keys WHERE created_at < ?1",
        [&month_ago],
    )?;
    let codes = conn.execute(
        "DELETE FROM pairing_codes WHERE expires_at < ?1",
        [&now_str],
    )?;
    let notices = conn.execute(
        "DELETE FROM notifications WHERE read_at IS NOT NULL AND read_at < ?1",
        [(now - chrono::Duration::days(90)).to_rfc3339()],
    )?;
    Ok(format!(
        "{sessions} sessions, {keys} request ids, {codes} pairing codes, {notices} notices removed"
    ))
}

fn run(state: &AppState, job: Job) -> AppResult<String> {
    let now = state.now();
    match job {
        Job::Backup => {
            let run = backup::create(state, "scheduled", None)?;
            let pruned = backup::prune(state)?;
            Ok(format!(
                "{} ({} old removed)",
                run.file_name.unwrap_or_default(),
                pruned
            ))
        }
        Job::LowStock => low_stock(&*state.conn()?, now),
        Job::Integrity => integrity(&*state.conn()?, now),
        Job::Cleanup => cleanup(&*state.conn()?, now),
    }
}

/// Runs every job that is due. Returns what ran and how it went.
pub fn tick(state: &AppState) -> AppResult<Vec<(Job, AppResult<String>)>> {
    let now = state.now();
    let settings = settings::load(&*state.conn()?)?;
    let mut ran = Vec::new();
    for job in Job::ALL {
        let last = last_run(&*state.conn()?, job)?;
        if !is_due(job, &settings, last, now) {
            continue;
        }
        let result = run(state, job);
        match &result {
            Ok(summary) => tracing::info!(job = job.name(), "{summary}"),
            Err(e) => tracing::warn!(job = job.name(), "failed: {}", e.message),
        }
        record(&*state.conn()?, job, now, &result)?;
        ran.push((job, result));
    }
    Ok(ran)
}

/// Starts the scheduler thread (hub and standalone mode).
pub fn spawn(state: AppState) {
    std::thread::Builder::new()
        .name("jobs".into())
        .spawn(move || loop {
            // Give the app a moment to start before the first backup.
            std::thread::sleep(Duration::from_secs(60));
            if let Err(e) = tick(&state) {
                tracing::warn!("job scheduler: {}", e.message);
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::error!("could not start the job scheduler: {e}"));
}
