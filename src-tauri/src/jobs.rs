//! Background jobs on the hub and on standalone PCs (docs/ARCHITECTURE.md §8). A thread wakes up
//! every minute and runs whatever is due; `tick` is the whole scheduler, so tests drive it with a
//! fixed clock. Each job's last run is kept in `job_runs`; a failing job never stops the others.

use std::time::Duration;

use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::app::AppState;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};
use crate::services::settings::{self, Settings};
use crate::services::{backup, cheques, images, printing, report_views};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Backup,
    LowStock,
    Expiry,
    Cheques,
    ReportExports,
    Integrity,
    Cleanup,
}

impl Job {
    pub const ALL: [Job; 7] = [
        Job::Backup,
        Job::LowStock,
        Job::Expiry,
        Job::Cheques,
        Job::ReportExports,
        Job::Integrity,
        Job::Cleanup,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Job::Backup => "backup",
            Job::LowStock => "low_stock",
            Job::Expiry => "expiry",
            Job::Cheques => "cheques",
            Job::ReportExports => "report_exports",
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
        Job::Expiry => settings.inventory.expiry_alerts && since(chrono::Duration::hours(1)),
        Job::Cheques => since(chrono::Duration::hours(6)),
        Job::ReportExports => since(chrono::Duration::hours(1)),
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

/// A notice per pending cheque whose date has come (deposit it, or pay it in), raised once.
fn cheques_due(conn: &Connection, settings: &Settings, now: DateTime<Utc>) -> AppResult<String> {
    let today = settings.business_date(now);
    let now_str = now.to_rfc3339();
    let mut raised = 0;
    for (id, direction, number, party, amount) in cheques::due(conn, &today)? {
        let (title, body) = if direction == "in" {
            (
                format!("Deposit cheque {number} from {party}"),
                format!(
                    "Its date has come ({}). Mark it cleared when the bank credits it.",
                    crate::domain::money::format_minor(
                        amount,
                        settings.business.currency_minor_digits
                    )
                ),
            )
        } else {
            (
                format!("Cheque {number} to {party} is due"),
                format!(
                    "Keep {} in the bank for it. Mark it cleared when it is paid.",
                    crate::domain::money::format_minor(
                        amount,
                        settings.business.currency_minor_digits
                    )
                ),
            )
        };
        if notify(
            conn,
            "cheque_due",
            "warning",
            &title,
            &body,
            &format!("cheque:{id}"),
            Some(("cheques", &id)),
            &now_str,
        )? {
            raised += 1;
        }
    }
    Ok(format!("{raised} cheque notice(s) raised"))
}

/// Hourly: a notice per batch that has expired or expires within the alert window while there is
/// stock of it; notices close when the stock is gone (sold or written off).
fn expiry(conn: &Connection, settings: &Settings, now: DateTime<Utc>) -> AppResult<String> {
    let now_str = now.to_rfc3339();
    let today = settings.business_date(now);
    let horizon = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .map(|d| {
            (d + chrono::Duration::days(settings.inventory.expiry_alert_days))
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| today.clone());
    let batches: Vec<(String, String, String, String, i64)> = conn
        .prepare(
            "SELECT b.id, p.name || coalesce(' — ' || v.variant_name, ''), b.batch_no, b.expiry_date, sum(l.qty)
             FROM batches b JOIN stock_levels l ON l.batch_id = b.id JOIN product_variants v ON v.id = b.variant_id
             JOIN products p ON p.id = v.product_id
             WHERE b.expiry_date IS NOT NULL AND b.expiry_date <= ?1 AND b.deleted_at IS NULL
             GROUP BY b.id HAVING sum(l.qty) > 0",
        )?
        .query_map([&horizon], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut raised = 0;
    let mut keys = Vec::new();
    for (batch_id, name, batch_no, expiry, qty) in &batches {
        let expired = expiry.as_str() < today.as_str();
        let key = format!(
            "expiry:{batch_id}:{}",
            if expired { "expired" } else { "near" }
        );
        let (severity, title) = if expired {
            ("error", format!("Expired: {name} batch {batch_no}"))
        } else {
            ("warning", format!("Expires soon: {name} batch {batch_no}"))
        };
        let body = format!(
            "{} in stock, expiry {expiry}. Sell it first, return it to the supplier or write it off.",
            *qty as f64 / 1000.0
        );
        if notify(
            conn,
            "expiry",
            severity,
            &title,
            &body,
            &key,
            Some(("batches", batch_id)),
            &now_str,
        )? {
            raised += 1;
        }
        keys.push(key);
    }
    let open: Vec<String> = conn
        .prepare("SELECT dedupe_key FROM notifications WHERE kind = 'expiry' AND read_at IS NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut cleared = 0;
    for key in open.iter().filter(|k| !keys.contains(k)) {
        cleared += conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE dedupe_key = ?2 AND read_at IS NULL",
            params![now_str, key],
        )?;
    }
    Ok(format!(
        "{} batches, {raised} new, {cleared} cleared",
        batches.len()
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
        (
            "v_cost_layer_mismatches",
            "FIFO cost layers differ from the stock ledger",
        ),
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
    let print_jobs = printing::purge_old_jobs(conn, now)?;
    Ok(format!(
        "{sessions} sessions, {keys} request ids, {codes} pairing codes, {notices} notices, {print_jobs} print jobs removed"
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
        Job::Expiry => expiry(&*state.conn()?, &settings::load(&*state.conn()?)?, now),
        Job::Cheques => cheques_due(&*state.conn()?, &settings::load(&*state.conn()?)?, now),
        Job::ReportExports => {
            let conn = state.conn()?;
            let settings = settings::load(&conn)?;
            let (written, failed) = report_views::run_due(
                &conn,
                &settings,
                &now.to_rfc3339(),
                &settings.business_date(now),
            )?;
            Ok(format!("{written} report(s) exported, {failed} failed"))
        }
        Job::Integrity => integrity(&*state.conn()?, now),
        Job::Cleanup => {
            let report = cleanup(&*state.conn()?, now)?;
            let pictures = images::sweep_orphans(state)?;
            Ok(format!("{report}, {pictures} stray picture files removed"))
        }
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
            // The hub's print queue: failed jobs whose next try is due (not a recorded job: it
            // runs every minute and only logs when there was something to print).
            match printing::retry_due(&state) {
                Ok(summary) if !summary.starts_with("0 ") => {
                    tracing::info!("print queue: {summary}")
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("print queue: {}", e.message),
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::error!("could not start the job scheduler: {e}"));
}
