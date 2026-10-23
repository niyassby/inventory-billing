//! Locked periods. A day is locked when the closed-day lock is on and it is on or before the
//! latest closed day (unless the owner reopened it), and every day of a closed financial year is
//! locked. Every service that dates a document asks `check` once; inside a locked day it needs
//! `period.override` (the user's own or a manager's approval) and the override is audited.
//!
//! The owner reopens a day or a year here, with a reason; closing a year moves its profit to
//! retained earnings with one journal entry and locks it.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::accounts;
use super::audit::AuditEntry;
use super::common::{date, required};
use super::journal::{self, Entry, JLine};
use super::settings::{self, Settings};
use super::{write, Actor, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

/// The last locked day: the end of the latest closed year, or (with the lock on) the latest
/// closed day, whichever is later. Reopened days inside it are not locked (see `is_locked`).
pub fn locked_through(conn: &Connection, settings: &Settings) -> AppResult<Option<String>> {
    let year: Option<String> = conn.query_row(
        "SELECT max(end_date) FROM fiscal_year_closes WHERE status = 'closed'",
        [],
        |r| r.get(0),
    )?;
    let day: Option<String> = if settings.accounting.lock_closed_days {
        conn.query_row(
            "SELECT max(business_date) FROM day_closes WHERE status = 'closed'",
            [],
            |r| r.get(0),
        )?
    } else {
        None
    };
    Ok(year.max(day))
}

/// Whether documents dated `day` are locked.
pub fn is_locked(conn: &Connection, settings: &Settings, day: &str) -> AppResult<bool> {
    let in_closed_year: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM fiscal_year_closes WHERE status = 'closed' AND ?1 BETWEEN start_date AND end_date)",
        [day],
        |r| r.get(0),
    )?;
    if in_closed_year {
        return Ok(true);
    }
    if !settings.accounting.lock_closed_days {
        return Ok(false);
    }
    let (latest, reopened): (Option<String>, bool) = conn.query_row(
        "SELECT (SELECT max(business_date) FROM day_closes WHERE status = 'closed'),
                EXISTS (SELECT 1 FROM day_closes WHERE business_date = ?1 AND status = 'reopened')",
        [day],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(!reopened && latest.is_some_and(|l| day <= l.as_str()))
}

/// Permission to write into a locked day, kept until the document's transaction audits it.
#[derive(Debug, Clone)]
pub struct Override {
    pub date: String,
    pub what: String,
    /// The manager who approved (None when the user holds `period.override` themselves).
    pub approved_by: Option<String>,
}

impl Override {
    /// Writes the `PERIOD_OVERRIDE` audit row for the document that used it.
    pub fn audit(
        &self,
        conn: &Connection,
        actor: &Actor,
        entity_type: &str,
        entity_id: &str,
        now: &str,
    ) -> AppResult<()> {
        AuditEntry::new("PERIOD_OVERRIDE", entity_type, Some(entity_id))
            .by(actor)
            .approved_by(self.approved_by.as_deref())
            .summary(format!("{} dated {} (a locked day)", self.what, self.date))
            .write(conn, now)
    }
}

/// Refuses a document dated in a locked day unless the user (or an approving manager) holds
/// `period.override`. `what` names the document for the approval prompt ("an invoice").
pub fn check(
    ctx: &Ctx<'_>,
    settings: &Settings,
    day: &str,
    what: &str,
) -> AppResult<Option<Override>> {
    if !is_locked(ctx.conn, settings, day)? {
        return Ok(None);
    }
    let approved_by = ctx.approve(
        "period.override",
        &format!("{what} dated {day}, which is locked"),
    )?;
    Ok(Some(Override {
        date: day.to_string(),
        what: what.to_string(),
        approved_by,
    }))
}

/// Voiding a document changes its own day (it leaves that day's sales) and posts reversals
/// today: either being locked needs the override.
pub fn check_void(
    ctx: &Ctx<'_>,
    settings: &Settings,
    dated: &str,
    today: &str,
    what: &str,
) -> AppResult<Option<Override>> {
    match check(ctx, settings, dated, what)? {
        Some(o) => Ok(Some(o)),
        None => check(ctx, settings, today, what),
    }
}

/// Audits an override inside the document's transaction (no-op without one).
pub fn audit(
    lock: &Option<Override>,
    conn: &Connection,
    actor: &Actor,
    entity_type: &str,
    entity_id: &str,
    now: &str,
) -> AppResult<()> {
    match lock {
        Some(o) => o.audit(conn, actor, entity_type, entity_id, now),
        None => Ok(()),
    }
}

// ------------------------------------------------------------------ status and closed days

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStatus {
    pub lock_closed_days: bool,
    /// Documents on or before this day are locked (reopened days excepted).
    pub locked_through: Option<String>,
    pub reopened_days: Vec<String>,
    pub can_override: bool,
    pub can_manage: bool,
}

pub fn status(ctx: &Ctx<'_>) -> AppResult<PeriodStatus> {
    ctx.require("day_close.perform")
        .or_else(|_| ctx.require("accounting.view"))?;
    let settings = settings::load(ctx.conn)?;
    let reopened_days = ctx
        .conn
        .prepare("SELECT business_date FROM day_closes WHERE status = 'reopened' ORDER BY business_date DESC LIMIT 50")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(PeriodStatus {
        lock_closed_days: settings.accounting.lock_closed_days,
        locked_through: locked_through(ctx.conn, &settings)?,
        reopened_days,
        can_override: ctx.actor.can("period.override"),
        can_manage: ctx.actor.can("period.manage"),
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReopenDayInput {
    pub business_date: String,
    pub reason: String,
}

/// Reopens a closed day (owner): documents can be dated in it again until it is closed again.
pub fn reopen_day(ctx: &mut Ctx<'_>, input: &ReopenDayInput) -> AppResult<()> {
    ctx.require("period.manage")?;
    let day = date("businessDate", &input.business_date)?;
    let reason = required("reason", &input.reason, 300)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        if in_closed_year(tx, &day)? {
            return Err(AppError::conflict(
                "This day is in a closed financial year. Reopen the year first.",
            ));
        }
        let changed = tx.execute(
            "UPDATE day_closes SET status = 'reopened', reopened_at = ?1, reopened_by = ?2, reopen_reason = ?3, updated_at = ?1
             WHERE business_date = ?4 AND status = 'closed'",
            params![now, actor.user_id, reason, day],
        )?;
        if changed == 0 {
            return Err(AppError::conflict("This day is not closed."));
        }
        AuditEntry::new("DAY_REOPENED", "day_closes", Some(&day))
            .by(actor)
            .summary(format!("{day}: {reason}"))
            .write(tx, &now)
    })
}

fn in_closed_year(conn: &Connection, day: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM fiscal_year_closes WHERE status = 'closed' AND ?1 BETWEEN start_date AND end_date)",
        [day],
        |r| r.get(0),
    )?)
}

// ------------------------------------------------------------------ financial years

/// The first and last day of the financial year containing `day`.
pub fn year_bounds(day: &str, start_month: u32) -> AppResult<(String, String)> {
    let d = NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| AppError::field("endDate", "Use a date like 2026-03-31."))?;
    let month = start_month.clamp(1, 12);
    let start_year = if d.month() >= month {
        d.year()
    } else {
        d.year() - 1
    };
    let start = NaiveDate::from_ymd_opt(start_year, month, 1)
        .ok_or_else(|| AppError::internal("bad financial year start"))?;
    let next = NaiveDate::from_ymd_opt(start_year + 1, month, 1)
        .ok_or_else(|| AppError::internal("bad financial year start"))?;
    let end = next.pred_opt().unwrap_or(next);
    Ok((
        start.format("%Y-%m-%d").to_string(),
        end.format("%Y-%m-%d").to_string(),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearRow {
    pub fy_label: String,
    pub start_date: String,
    pub end_date: String,
    /// "open", "closed" or "reopened".
    pub status: String,
    /// Profit (or loss, negative) of the year as booked now; the closing figure once closed.
    pub net_profit: i64,
    pub closed_at: Option<String>,
    pub closed_by: Option<String>,
    pub reopened_at: Option<String>,
    pub reopen_reason: Option<String>,
    /// The year has ended and every earlier year with entries is closed.
    pub can_close: bool,
}

/// A year's close as stored: status, net profit, closed at, by whom, reopened at, why.
type CloseRecord = (String, i64, String, String, Option<String>, Option<String>);
/// An account's balance in a year: id, code, name, type, debit − credit.
type BalanceLine = (String, String, String, String, i64);

/// Income less expenses booked in the year, leaving out year-close entries.
fn year_profit(conn: &Connection, start: &str, end: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(sum(l.credit - l.debit), 0)
         FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id JOIN accounts a ON a.id = l.account_id
         WHERE e.business_date BETWEEN ?1 AND ?2 AND a.account_type IN ('income', 'expense') AND e.source_type <> 'year_close'",
        params![start, end],
        |r| r.get(0),
    )?)
}

/// Every financial year from the first entry to the current one, newest first.
pub fn years(ctx: &Ctx<'_>) -> AppResult<Vec<YearRow>> {
    ctx.require("accounting.view")?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let start_month = settings.business.fy_start_month;
    let first: Option<String> =
        ctx.conn
            .query_row("SELECT min(business_date) FROM journal_entries", [], |r| {
                r.get(0)
            })?;
    let mut rows = Vec::new();
    let (mut start, mut end) = year_bounds(first.as_deref().unwrap_or(&today), start_month)?;
    let mut earlier_open = false;
    loop {
        let closed: Option<CloseRecord> = ctx
            .conn
            .query_row(
                "SELECT f.status, f.net_profit, f.closed_at, u.name, f.reopened_at, f.reopen_reason
                 FROM fiscal_year_closes f JOIN users u ON u.id = f.closed_by
                 WHERE f.start_date = ?1 ORDER BY f.status = 'closed' DESC, f.closed_at DESC LIMIT 1",
                [&start],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?;
        let status = closed.as_ref().map_or("open".to_string(), |c| c.0.clone());
        let is_closed = status == "closed";
        let net_profit = match &closed {
            Some(c) if is_closed => c.1,
            _ => year_profit(ctx.conn, &start, &end)?,
        };
        rows.push(YearRow {
            fy_label: settings.fy_label(&start),
            start_date: start.clone(),
            end_date: end.clone(),
            status,
            net_profit,
            closed_at: closed.as_ref().map(|c| c.2.clone()),
            closed_by: closed.as_ref().map(|c| c.3.clone()),
            reopened_at: closed.as_ref().and_then(|c| c.4.clone()),
            reopen_reason: closed.as_ref().and_then(|c| c.5.clone()),
            can_close: !is_closed && end < today && !earlier_open,
        });
        if !is_closed {
            earlier_open = true;
        }
        if end >= today {
            break;
        }
        let next = NaiveDate::parse_from_str(&end, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.succ_opt())
            .map(|d| d.format("%Y-%m-%d").to_string())
            .ok_or_else(|| AppError::internal("bad financial year end"))?;
        (start, end) = year_bounds(&next, start_month)?;
    }
    rows.reverse();
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearCloseInput {
    /// Any day of the year; the year is the financial year containing it.
    pub end_date: String,
    /// The net profit the owner saw in the preview; a different figure means something was
    /// posted meanwhile, and nothing is closed.
    pub expected_net_profit: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearCloseLine {
    pub code: String,
    pub name: String,
    /// The account's balance moved out (income credit-positive, expense debit-positive).
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearClosePreview {
    pub fy_label: String,
    pub start_date: String,
    pub end_date: String,
    pub income: Vec<YearCloseLine>,
    pub expenses: Vec<YearCloseLine>,
    pub net_profit: i64,
    /// Reasons the year can't be closed yet (empty = it can).
    pub blockers: Vec<String>,
}

fn year_lines(conn: &Connection, start: &str, end: &str) -> AppResult<Vec<BalanceLine>> {
    Ok(conn
        .prepare(
            "SELECT a.id, a.code, a.name, a.account_type, sum(l.debit - l.credit)
             FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id JOIN accounts a ON a.id = l.account_id
             WHERE e.business_date BETWEEN ?1 AND ?2 AND a.account_type IN ('income', 'expense') AND e.source_type <> 'year_close'
             GROUP BY a.id HAVING sum(l.debit - l.credit) <> 0 ORDER BY a.code",
        )?
        .query_map(params![start, end], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?)
}

fn preview_of(
    conn: &Connection,
    settings: &Settings,
    today: &str,
    day: &str,
) -> AppResult<YearClosePreview> {
    let (start, end) = year_bounds(day, settings.business.fy_start_month)?;
    let lines = year_lines(conn, &start, &end)?;
    let mut blockers = Vec::new();
    if end.as_str() >= today {
        blockers.push(format!("The year ends on {end}; close it after that day."));
    }
    let closed: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM fiscal_year_closes WHERE start_date = ?1 AND status = 'closed')",
        [&start],
        |r| r.get(0),
    )?;
    if closed {
        blockers.push("This year is already closed.".into());
    }
    let first: Option<String> =
        conn.query_row("SELECT min(business_date) FROM journal_entries", [], |r| {
            r.get(0)
        })?;
    if let Some(first) = first.filter(|f| f.as_str() < start.as_str()) {
        // Every earlier year with entries must be closed first.
        let (mut s, _) = year_bounds(&first, settings.business.fy_start_month)?;
        while s < start {
            let (ys, ye) = year_bounds(&s, settings.business.fy_start_month)?;
            let has_entries: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM journal_entries WHERE business_date BETWEEN ?1 AND ?2)",
                params![ys, ye],
                |r| r.get(0),
            )?;
            let done: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM fiscal_year_closes WHERE start_date = ?1 AND status = 'closed')",
                [&ys],
                |r| r.get(0),
            )?;
            if has_entries && !done {
                blockers.push(format!("Close the year {} first.", settings.fy_label(&ys)));
                break;
            }
            s = NaiveDate::parse_from_str(&ye, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.succ_opt())
                .map(|d| d.format("%Y-%m-%d").to_string())
                .ok_or_else(|| AppError::internal("bad financial year end"))?;
        }
    }
    let pending: Vec<String> = conn
        .prepare(
            "SELECT doc_no FROM manual_journals WHERE status IN ('submitted', 'approved') AND deleted_at IS NULL
             AND business_date BETWEEN ?1 AND ?2 ORDER BY doc_no LIMIT 5",
        )?
        .query_map(params![start, end], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    if !pending.is_empty() {
        blockers.push(format!(
            "Post or send back these journals first: {}.",
            pending.join(", ")
        ));
    }
    let mut income = Vec::new();
    let mut expenses = Vec::new();
    for (_, code, name, kind, balance) in &lines {
        if kind == "income" {
            income.push(YearCloseLine {
                code: code.clone(),
                name: name.clone(),
                amount: -balance,
            });
        } else {
            expenses.push(YearCloseLine {
                code: code.clone(),
                name: name.clone(),
                amount: *balance,
            });
        }
    }
    let net_profit = -lines.iter().map(|l| l.4).sum::<i64>();
    Ok(YearClosePreview {
        fy_label: settings.fy_label(&start),
        start_date: start,
        end_date: end,
        income,
        expenses,
        net_profit,
        blockers,
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearPreviewInput {
    pub end_date: String,
}

pub fn year_close_preview(ctx: &Ctx<'_>, input: &YearPreviewInput) -> AppResult<YearClosePreview> {
    ctx.require("period.manage")?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    preview_of(
        ctx.conn,
        &settings,
        &today,
        &date("endDate", &input.end_date)?,
    )
}

/// Closes a financial year (owner): one journal entry dated its last day moves every income and
/// expense balance of the year to Retained Earnings, and the year is locked.
pub fn close_year(ctx: &mut Ctx<'_>, input: &YearCloseInput) -> AppResult<YearRow> {
    ctx.require("period.manage")?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let day = date("endDate", &input.end_date)?;
    let reason = super::common::optional("reason", input.reason.as_deref(), 300)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let start = write(ctx.conn, |tx| {
        let preview = preview_of(tx, &settings, &today, &day)?;
        if let Some(blocker) = preview.blockers.first() {
            return Err(AppError::conflict(blocker.clone()));
        }
        if preview.net_profit != input.expected_net_profit {
            return Err(AppError::conflict(
                "The year's figures changed since the preview. Check them again.",
            ));
        }
        let id = new_id();
        let mut lines: Vec<JLine> = year_lines(tx, &preview.start_date, &preview.end_date)?
            .into_iter()
            .map(|(account, _, _, _, balance)| JLine {
                account_id: account,
                party_id: None,
                amount: -balance,
                memo: None,
            })
            .collect();
        lines.push(JLine::credit(
            accounts::RETAINED_EARNINGS,
            preview.net_profit,
        ));
        let entry = journal::post(
            tx,
            Entry {
                business_date: &preview.end_date,
                source_type: "year_close",
                source_id: &id,
                narration: format!("Year close {}", preview.fy_label),
                lines,
                user_id: Some(&actor.user_id),
                now: &now,
            },
        )?;
        tx.execute(
            "INSERT INTO fiscal_year_closes (id, fy_label, start_date, end_date, status, journal_entry_id, net_profit, closed_at, closed_by, reason, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'closed', ?5, ?6, ?7, ?8, ?9, ?7, ?7)",
            params![id, preview.fy_label, preview.start_date, preview.end_date, entry, preview.net_profit, now, actor.user_id, reason],
        )?;
        AuditEntry::new("YEAR_CLOSED", "fiscal_year_closes", Some(&id))
            .by(actor)
            .summary(format!(
                "{}: net profit {} to retained earnings",
                preview.fy_label, preview.net_profit
            ))
            .write(tx, &now)?;
        Ok(preview.start_date)
    })?;
    year_row(ctx, &start)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct YearReopenInput {
    pub end_date: String,
    pub reason: String,
}

/// Reopens a closed year (owner): its closing entry is reversed and its days unlock. Later
/// closed years must be reopened first.
pub fn reopen_year(ctx: &mut Ctx<'_>, input: &YearReopenInput) -> AppResult<YearRow> {
    ctx.require("period.manage")?;
    let settings = settings::load(ctx.conn)?;
    let (start, end) = year_bounds(
        &date("endDate", &input.end_date)?,
        settings.business.fy_start_month,
    )?;
    let reason = required("reason", &input.reason, 300)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        reopen_year_in(tx, actor, &start, &end, &reason, &now)
    })?;
    year_row(ctx, &start)
}

fn reopen_year_in(
    tx: &Transaction<'_>,
    actor: &Actor,
    start: &str,
    end: &str,
    reason: &str,
    now: &str,
) -> AppResult<()> {
    let id: String = tx
        .query_row(
            "SELECT id FROM fiscal_year_closes WHERE start_date = ?1 AND status = 'closed'",
            [start],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::conflict("This year is not closed."))?;
    let later: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM fiscal_year_closes WHERE status = 'closed' AND start_date > ?1)",
        [start],
        |r| r.get(0),
    )?;
    if later {
        return Err(AppError::conflict("Reopen the later years first."));
    }
    journal::reverse_source(
        tx,
        "year_close",
        &id,
        end,
        "Year reopened",
        Some(&actor.user_id),
        now,
    )?;
    tx.execute(
        "UPDATE fiscal_year_closes SET status = 'reopened', reopened_at = ?1, reopened_by = ?2, reopen_reason = ?3, updated_at = ?1 WHERE id = ?4",
        params![now, actor.user_id, reason, id],
    )?;
    AuditEntry::new("YEAR_REOPENED", "fiscal_year_closes", Some(&id))
        .by(actor)
        .summary(format!("{start} – {end}: {reason}"))
        .write(tx, now)
}

fn year_row(ctx: &Ctx<'_>, start: &str) -> AppResult<YearRow> {
    years(ctx)?
        .into_iter()
        .find(|y| y.start_date == start)
        .ok_or_else(|| AppError::not_found("The financial year"))
}
