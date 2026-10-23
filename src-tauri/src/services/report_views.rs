//! Saved report views (per user) and their optional scheduled export: a view is a report with
//! its period, grouping and filters; a schedule writes it as CSV into a folder every day, week or
//! month. Exports run in the background job on the hub (or a standalone PC), as the user who set
//! them up, with that user's permissions checked at every run.

use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::audit::AuditEntry;
use super::common::{date, required};
use super::reports::{self, ReportQuery, ReportTable, REPORTS};
use super::settings::{self, Settings};
use super::users::IdInput;
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

const PERIODS: [&str; 8] = [
    "today",
    "yesterday",
    "week",
    "month",
    "last_month",
    "fy",
    "year",
    "custom",
];
const MAX_VIEWS: i64 = 100;

/// What a view remembers. `period` is one of the screen's presets or "custom" (then `from` and
/// `to`); the rest are the report's own options.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewParams {
    pub period: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub group_by: Option<String>,
    pub warehouse_id: Option<String>,
    pub party_type: Option<String>,
    pub account_id: Option<String>,
    pub party_id: Option<String>,
    pub user_id: Option<String>,
    pub category_id: Option<String>,
    pub variant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInfo {
    /// daily (yesterday), weekly (last Monday to Sunday) or monthly (last month).
    pub frequency: String,
    pub folder: String,
    pub is_active: bool,
    pub last_run_at: Option<String>,
    pub last_period: Option<String>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub id: String,
    pub report: String,
    pub name: String,
    pub params: ViewParams,
    pub schedule: Option<ScheduleInfo>,
    pub updated_at: String,
}

fn views_of(conn: &Connection, user_id: &str, id: Option<&str>) -> AppResult<Vec<ReportView>> {
    let rows: Vec<(String, String, String, String, String)> = conn
        .prepare(
            "SELECT id, report, name, params_json, updated_at FROM report_views
             WHERE user_id = ?1 AND deleted_at IS NULL AND (?2 IS NULL OR id = ?2) ORDER BY lower(name)",
        )?
        .query_map(params![user_id, id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|(id, report, name, params_json, updated_at)| {
            let schedule = conn
                .query_row(
                    "SELECT frequency, folder, is_active, last_run_at, last_period, last_status, last_error
                     FROM report_schedules WHERE view_id = ?1 AND deleted_at IS NULL",
                    [&id],
                    |r| {
                        Ok(ScheduleInfo {
                            frequency: r.get(0)?,
                            folder: r.get(1)?,
                            is_active: r.get(2)?,
                            last_run_at: r.get(3)?,
                            last_period: r.get(4)?,
                            last_status: r.get(5)?,
                            last_error: r.get(6)?,
                        })
                    },
                )
                .optional()?;
            Ok(ReportView {
                id,
                report,
                name,
                params: serde_json::from_str(&params_json).unwrap_or_default(),
                schedule,
                updated_at,
            })
        })
        .collect()
}

/// The signed-in user's saved views.
pub fn list(ctx: &Ctx<'_>) -> AppResult<Vec<ReportView>> {
    ctx.require("reports.view")?;
    views_of(ctx.conn, &ctx.actor.user_id, None)
}

fn check_params(report: &str, p: &ViewParams) -> AppResult<ViewParams> {
    if !REPORTS.contains(&report) {
        return Err(AppError::field("report", "Unknown report."));
    }
    let mut p = p.clone();
    if p.period.is_empty() {
        p.period = "month".into();
    }
    if !PERIODS.contains(&p.period.as_str()) {
        return Err(AppError::field("params.period", "Unknown period."));
    }
    if p.period == "custom" {
        let from = date("params.from", p.from.as_deref().unwrap_or_default())?;
        let to = date("params.to", p.to.as_deref().unwrap_or_default())?;
        if from > to {
            return Err(AppError::field(
                "params.from",
                "The start is after the end.",
            ));
        }
        p.from = Some(from);
        p.to = Some(to);
    } else {
        p.from = None;
        p.to = None;
    }
    for value in [
        &p.group_by,
        &p.warehouse_id,
        &p.party_type,
        &p.account_id,
        &p.party_id,
        &p.user_id,
        &p.category_id,
        &p.variant_id,
    ] {
        if value.as_ref().is_some_and(|v| v.len() > 64) {
            return Err(AppError::field("params", "A filter value is too long."));
        }
    }
    Ok(p)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportViewInput {
    pub id: Option<String>,
    pub expected_updated_at: Option<String>,
    pub report: String,
    pub name: String,
    pub params: ViewParams,
}

pub fn save(ctx: &mut Ctx<'_>, input: &ReportViewInput) -> AppResult<ReportView> {
    ctx.require("reports.view")?;
    let name = required("name", &input.name, 80)?;
    let params = check_params(&input.report, &input.params)?;
    let json = serde_json::to_string(&params)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| {
        let taken: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM report_views WHERE user_id = ?1 AND lower(name) = lower(?2) AND deleted_at IS NULL AND id IS NOT ?3)",
            params![actor.user_id, name, input.id],
            |r| r.get(0),
        )?;
        if taken {
            return Err(AppError::field(
                "name",
                "You already have a view with this name.",
            ));
        }
        let id = match &input.id {
            None => {
                let count: i64 = tx.query_row(
                    "SELECT count(*) FROM report_views WHERE user_id = ?1 AND deleted_at IS NULL",
                    [&actor.user_id],
                    |r| r.get(0),
                )?;
                if count >= MAX_VIEWS {
                    return Err(AppError::conflict(format!(
                        "You can keep at most {MAX_VIEWS} views."
                    )));
                }
                let id = new_id();
                tx.execute(
                    "INSERT INTO report_views (id, user_id, report, name, params_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![id, actor.user_id, input.report, name, json, now],
                )?;
                id
            }
            Some(id) => {
                let current = views_of(tx, &actor.user_id, Some(id))?
                    .pop()
                    .ok_or_else(|| AppError::not_found("The view"))?;
                if input
                    .expected_updated_at
                    .as_ref()
                    .is_some_and(|e| *e != current.updated_at)
                {
                    return Err(AppError::conflict(
                        "This view was changed elsewhere. Reload and try again.",
                    ));
                }
                tx.execute(
                    "UPDATE report_views SET report = ?1, name = ?2, params_json = ?3, updated_at = ?4 WHERE id = ?5",
                    params![input.report, name, json, now, id],
                )?;
                id.clone()
            }
        };
        AuditEntry::new("REPORT_VIEW_SAVED", "report_views", Some(&id))
            .by(actor)
            .summary(format!("{}: {name}", input.report))
            .write(tx, &now)?;
        Ok(id)
    })?;
    views_of(ctx.conn, &ctx.actor.user_id, Some(&id))?
        .pop()
        .ok_or_else(|| AppError::not_found("The view"))
}

pub fn delete(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("reports.view")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let changed = tx.execute(
            "UPDATE report_views SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND user_id = ?3 AND deleted_at IS NULL",
            params![now, input.id, actor.user_id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("The view"));
        }
        tx.execute(
            "UPDATE report_schedules SET deleted_at = ?1, updated_at = ?1 WHERE view_id = ?2 AND deleted_at IS NULL",
            params![now, input.id],
        )?;
        AuditEntry::new("REPORT_VIEW_DELETED", "report_views", Some(&input.id))
            .by(actor)
            .write(tx, &now)
    })
}

// ------------------------------------------------------------------ schedules

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInput {
    pub view_id: String,
    /// daily, weekly or monthly; None removes the schedule.
    pub frequency: Option<String>,
    /// A folder on the hub PC (full path, must exist).
    pub folder: Option<String>,
    #[serde(default = "yes")]
    pub is_active: bool,
}

fn yes() -> bool {
    true
}

/// Sets, changes or removes a view's scheduled export. Writing files on the hub needs
/// `reports.export` and `settings.manage`.
pub fn set_schedule(ctx: &mut Ctx<'_>, input: &ScheduleInput) -> AppResult<ReportView> {
    ctx.require("reports.export")?;
    ctx.require("settings.manage")?;
    views_of(ctx.conn, &ctx.actor.user_id, Some(&input.view_id))?
        .pop()
        .ok_or_else(|| AppError::not_found("The view"))?;
    let schedule = match input.frequency.as_deref() {
        None => None,
        Some(f @ ("daily" | "weekly" | "monthly")) => {
            let folder = required("folder", input.folder.as_deref().unwrap_or_default(), 400)?;
            let path = Path::new(&folder);
            if !path.is_absolute() {
                return Err(AppError::field("folder", "Use a full folder path."));
            }
            if !path.is_dir() {
                return Err(AppError::field(
                    "folder",
                    "That folder doesn't exist on this PC (the hub).",
                ));
            }
            Some((f.to_string(), folder))
        }
        Some(_) => {
            return Err(AppError::field(
                "frequency",
                "Use daily, weekly or monthly.",
            ));
        }
    };
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "UPDATE report_schedules SET deleted_at = ?1, updated_at = ?1 WHERE view_id = ?2 AND deleted_at IS NULL",
            params![now, input.view_id],
        )?;
        if let Some((frequency, folder)) = &schedule {
            tx.execute(
                "INSERT INTO report_schedules (id, view_id, frequency, folder, is_active, created_at, updated_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
                params![new_id(), input.view_id, frequency, folder, input.is_active, now, actor.user_id],
            )?;
        }
        AuditEntry::new("REPORT_SCHEDULE_SET", "report_views", Some(&input.view_id))
            .by(actor)
            .summary(match &schedule {
                Some((f, folder)) => format!("{f} to {folder}"),
                None => "removed".into(),
            })
            .write(tx, &now)
    })?;
    views_of(ctx.conn, &ctx.actor.user_id, Some(&input.view_id))?
        .pop()
        .ok_or_else(|| AppError::not_found("The view"))
}

/// The period a scheduled export covers on `today`: yesterday, last Monday–Sunday, or last
/// month.
pub fn schedule_period(frequency: &str, today: &str) -> Option<(String, String)> {
    let t = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    match frequency {
        "daily" => {
            let y = t - Duration::days(1);
            Some((fmt(y), fmt(y)))
        }
        "weekly" => {
            let this_monday = t - Duration::days(i64::from(t.weekday().num_days_from_monday()));
            Some((
                fmt(this_monday - Duration::days(7)),
                fmt(this_monday - Duration::days(1)),
            ))
        }
        "monthly" => {
            let first = t.with_day(1)?;
            let last = first - Duration::days(1);
            Some((fmt(last.with_day(1)?), fmt(last)))
        }
        _ => None,
    }
}

fn csv_field(text: &str) -> String {
    // A cell starting with = + @ (or - not followed by a digit) would run as a formula.
    let risky = text.starts_with(['=', '+', '@'])
        || (text.starts_with('-') && !text[1..].starts_with(|c: char| c.is_ascii_digit()));
    let safe = if risky {
        format!("'{text}")
    } else {
        text.to_string()
    };
    if safe.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn scaled(value: i64, digits: u32) -> String {
    let text = crate::domain::money::format_minor(value, digits);
    if digits == 0 {
        return text;
    }
    // "12.50" → "12.5", "3.00" → "3" (the same plain numbers the screen's CSV writes).
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

/// The report as CSV, like the screen's export: exact decimals, a header and the totals line.
pub fn table_to_csv(table: &ReportTable, minor_digits: u32) -> String {
    let raw = |kind: &str, v: &Value| -> String {
        match v {
            Value::Null => String::new(),
            Value::Number(n) => match (kind, n.as_i64()) {
                ("money", Some(i)) => scaled(i, minor_digits),
                ("qty", Some(i)) => scaled(i, 3),
                ("rate" | "percent", Some(i)) => scaled(i, 6),
                _ => n.to_string(),
            },
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    };
    let mut lines = vec![table
        .columns
        .iter()
        .map(|c| csv_field(&c.label))
        .collect::<Vec<_>>()
        .join(",")];
    for row in table.rows.iter().chain(table.totals.iter()) {
        lines.push(
            table
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| csv_field(&raw(&c.kind, row.get(i).unwrap_or(&Value::Null))))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    format!("\u{feff}{}\r\n", lines.join("\r\n"))
}

fn file_safe(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .take(60)
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "report".into()
    } else {
        cleaned
    }
}

/// A schedule to run: id, frequency, folder, view name, last period, report, params, user.
type DueExport = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);

/// Runs every active schedule whose current period hasn't been exported yet. Returns
/// (written, failed). A user who lost the permission, or a folder that is gone, fails that
/// schedule (recorded on it and raised as a notice) without stopping the others.
pub fn run_due(
    conn: &Connection,
    settings: &Settings,
    now: &str,
    today: &str,
) -> AppResult<(usize, usize)> {
    let due: Vec<DueExport> = conn
        .prepare(
            "SELECT s.id, s.frequency, s.folder, v.name, s.last_period, v.report, v.params_json, v.user_id
             FROM report_schedules s JOIN report_views v ON v.id = s.view_id
             JOIN users u ON u.id = v.user_id
             WHERE s.deleted_at IS NULL AND s.is_active = 1 AND v.deleted_at IS NULL",
        )?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })?
        .collect::<Result<_, _>>()?;
    let (mut written, mut failed) = (0, 0);
    for (id, frequency, folder, name, last_period, report, params_json, user_id) in due {
        let Some((from, to)) = schedule_period(&frequency, today) else {
            continue;
        };
        let period = format!("{from}..{to}");
        if last_period.as_deref() == Some(period.as_str()) {
            continue;
        }
        let outcome = export_one(
            conn,
            settings,
            today,
            &user_id,
            &report,
            &params_json,
            &name,
            &folder,
            (&from, &to),
        );
        match &outcome {
            Ok(_) => written += 1,
            Err(e) => {
                failed += 1;
                super::notifications::raise(
                    conn,
                    &super::notifications::Notice {
                        kind: "report_export",
                        severity: "warning",
                        title: &format!("Scheduled export \"{name}\" failed"),
                        body: &e.message,
                        key: &format!("report_export:{id}:{period}"),
                        entity: Some(("report_views", &id)),
                    },
                    now,
                )?;
            }
        }
        conn.execute(
            "UPDATE report_schedules SET last_run_at = ?1, last_period = ?2, last_status = ?3, last_error = ?4, updated_at = ?1 WHERE id = ?5",
            params![
                now,
                period,
                if outcome.is_ok() { "ok" } else { "failed" },
                outcome.as_ref().err().map(|e| e.message.clone()),
                id
            ],
        )?;
    }
    Ok((written, failed))
}

#[allow(
    clippy::too_many_arguments,
    reason = "one export: who, what, where and which period"
)]
fn export_one(
    conn: &Connection,
    settings: &Settings,
    today: &str,
    user_id: &str,
    report: &str,
    params_json: &str,
    name: &str,
    folder: &str,
    range: (&str, &str),
) -> AppResult<String> {
    let allowed: bool = conn.query_row(
        "SELECT u.is_active = 1 AND u.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM role_permissions rp WHERE rp.role_id = u.role_id AND rp.permission_code = 'reports.export')
           AND EXISTS (SELECT 1 FROM role_permissions rp WHERE rp.role_id = u.role_id AND rp.permission_code = 'reports.view')
         FROM users u WHERE u.id = ?1",
        [user_id],
        |r| r.get(0),
    )?;
    if !allowed {
        return Err(AppError::validation(
            "The user who set up this export can no longer export reports.",
        ));
    }
    let p: ViewParams = serde_json::from_str(params_json).unwrap_or_default();
    let table = reports::run_on(
        conn,
        settings,
        today,
        &ReportQuery {
            report: report.into(),
            from: Some(range.0.into()),
            to: Some(range.1.into()),
            warehouse_id: p.warehouse_id,
            party_type: p.party_type,
            account_id: p.account_id,
            group_by: p.group_by,
            party_id: p.party_id,
            user_id: p.user_id,
            category_id: p.category_id,
            variant_id: p.variant_id,
        },
    )?;
    let dir = Path::new(folder);
    if !dir.is_dir() {
        return Err(AppError::validation(format!(
            "The folder {folder} is not there."
        )));
    }
    let file = if range.0 == range.1 {
        format!("{} {}.csv", file_safe(name), range.0)
    } else {
        format!("{} {} to {}.csv", file_safe(name), range.0, range.1)
    };
    let path = dir.join(&file);
    let tmp = path.with_extension("csv.tmp");
    std::fs::write(
        &tmp,
        table_to_csv(&table, settings.business.currency_minor_digits),
    )
    .and_then(|()| std::fs::rename(&tmp, &path))
    .map_err(|e| AppError::validation(format!("Couldn't write {file}: {e}")))?;
    Ok(file)
}

/// Runs one view's export now (to check the folder) and reports the file written.
pub fn run_now(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<String> {
    ctx.require("reports.export")?;
    ctx.require("settings.manage")?;
    let view = views_of(ctx.conn, &ctx.actor.user_id, Some(&input.id))?
        .pop()
        .ok_or_else(|| AppError::not_found("The view"))?;
    let schedule = view
        .schedule
        .as_ref()
        .ok_or_else(|| AppError::conflict("This view has no scheduled export."))?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let (from, to) = schedule_period(&schedule.frequency, &today)
        .ok_or_else(|| AppError::internal("unknown frequency"))?;
    let file = export_one(
        ctx.conn,
        &settings,
        &today,
        &ctx.actor.user_id,
        &view.report,
        &serde_json::to_string(&view.params)?,
        &view.name,
        &schedule.folder,
        (&from, &to),
    )?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        AuditEntry::new("REPORT_EXPORTED", "report_views", Some(&input.id))
            .by(actor)
            .summary(format!("{file} to {}", schedule.folder))
            .write(tx, &now)
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::{csv_field, scaled, schedule_period};

    #[test]
    fn periods_and_csv_cells() {
        assert_eq!(
            schedule_period("daily", "2026-10-23"),
            Some(("2026-10-22".into(), "2026-10-22".into()))
        );
        // 2026-10-23 is a Friday: last week is Monday 12th to Sunday 18th.
        assert_eq!(
            schedule_period("weekly", "2026-10-23"),
            Some(("2026-10-12".into(), "2026-10-18".into()))
        );
        assert_eq!(
            schedule_period("monthly", "2026-03-05"),
            Some(("2026-02-01".into(), "2026-02-28".into()))
        );
        assert_eq!(csv_field("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(csv_field("-12"), "-12");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(scaled(12_550, 2), "125.5");
        assert_eq!(scaled(300, 2), "3");
        assert_eq!(scaled(0, 2), "0");
        assert_eq!(scaled(-5, 2), "-0.05");
    }
}
