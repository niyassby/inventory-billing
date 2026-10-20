//! Accounting-lite: the chart of accounts (with cash and bank accounts the owner can add), the
//! end-of-day summary and day close, and the audit log viewer.

use rusqlite::types::Value as Sql;
use rusqlite::{params, params_from_iter, Connection};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::common::{date, like_pattern, optional, required, Page, PageQuery};
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

// ------------------------------------------------------------------ chart of accounts

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    pub id: String,
    pub code: String,
    pub name: String,
    /// asset, liability, equity, income or expense.
    pub account_type: String,
    pub subtype: String,
    pub is_system: bool,
    pub is_active: bool,
    pub bank_name: Option<String>,
    pub bank_account_no: Option<String>,
    /// Debit minus credit over all time (assets and expenses are positive).
    pub balance: i64,
}

pub fn list_accounts(ctx: &Ctx<'_>) -> AppResult<Vec<AccountRow>> {
    ctx.require("accounting.view")?;
    let rows = ctx
        .conn
        .prepare(
            "SELECT a.id, a.code, a.name, a.account_type, a.subtype, a.is_system, a.is_active, a.bank_name, a.bank_account_no,
                coalesce((SELECT sum(l.debit - l.credit) FROM journal_lines l WHERE l.account_id = a.id), 0)
             FROM accounts a WHERE a.deleted_at IS NULL ORDER BY a.code",
        )?
        .query_map([], |r| {
            Ok(AccountRow {
                id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                account_type: r.get(3)?,
                subtype: r.get(4)?,
                is_system: r.get(5)?,
                is_active: r.get(6)?,
                bank_name: r.get(7)?,
                bank_account_no: r.get(8)?,
                balance: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoneyAccountInput {
    pub id: Option<String>,
    /// "cash" or "bank".
    pub kind: String,
    pub name: String,
    pub bank_name: Option<String>,
    pub bank_account_no: Option<String>,
    pub bank_ifsc: Option<String>,
    #[serde(default = "yes")]
    pub is_active: bool,
}

fn yes() -> bool {
    true
}

/// Adds or edits a cash drawer or bank account (codes 1000–1099). System accounts stay as they
/// are, apart from their bank details.
pub fn save_money_account(ctx: &mut Ctx<'_>, input: &MoneyAccountInput) -> AppResult<AccountRow> {
    ctx.require("accounting.manage")?;
    if !matches!(input.kind.as_str(), "cash" | "bank") {
        return Err(AppError::field("kind", "Use cash or bank."));
    }
    let name = required("name", &input.name, 80)?;
    let bank_name = optional("bankName", input.bank_name.as_deref(), 80)?;
    let account_no = optional("bankAccountNo", input.bank_account_no.as_deref(), 40)?;
    let ifsc = optional("bankIfsc", input.bank_ifsc.as_deref(), 20)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| {
        let id = match &input.id {
            Some(id) => {
                let system: Option<bool> = tx
                    .query_row("SELECT is_system FROM accounts WHERE id = ?1 AND deleted_at IS NULL AND subtype IN ('cash', 'bank')", [id], |r| r.get(0))
                    .ok();
                let Some(system) = system else {
                    return Err(AppError::not_found("The account"));
                };
                if system {
                    tx.execute(
                        "UPDATE accounts SET bank_name = ?1, bank_account_no = ?2, bank_ifsc = ?3, updated_at = ?4, updated_by = ?5 WHERE id = ?6",
                        params![bank_name, account_no, ifsc, now, actor.user_id, id],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE accounts SET name = ?1, bank_name = ?2, bank_account_no = ?3, bank_ifsc = ?4, is_active = ?5, updated_at = ?6, updated_by = ?7 WHERE id = ?8",
                        params![name, bank_name, account_no, ifsc, input.is_active, now, actor.user_id, id],
                    )?;
                }
                id.clone()
            }
            None => {
                let next: i64 = tx.query_row(
                    "SELECT coalesce(max(CAST(code AS INTEGER)), 1009) + 1 FROM accounts WHERE code GLOB '10[0-9][0-9]'",
                    [],
                    |r| r.get(0),
                )?;
                if next > 1099 {
                    return Err(AppError::validation(
                        "There is no room for more cash or bank accounts.",
                    ));
                }
                let id = new_id();
                tx.execute(
                    "INSERT INTO accounts (id, code, name, account_type, subtype, bank_name, bank_account_no, bank_ifsc, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, 'asset', ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
                    params![id, next.to_string(), name, input.kind, bank_name, account_no, ifsc, now, actor.user_id],
                )?;
                id
            }
        };
        AuditEntry::new("ACCOUNT_SAVE", "accounts", Some(&id))
            .by(actor)
            .summary(name.clone())
            .write(tx, &now)?;
        Ok(id)
    })?;
    list_accounts(ctx)?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| AppError::not_found("The account"))
}

// ------------------------------------------------------------------ day close

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModeAmount {
    pub mode: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TillSummary {
    pub register: String,
    pub counted_by: Option<String>,
    pub expected_cash: i64,
    pub counted_cash: i64,
    pub difference: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub business_date: String,
    pub bills: i64,
    pub sales_total: i64,
    pub tax_total: i64,
    pub discount_total: i64,
    pub returns_total: i64,
    pub purchases_total: i64,
    pub expenses_total: i64,
    pub received: Vec<ModeAmount>,
    pub paid: Vec<ModeAmount>,
    pub tills: Vec<TillSummary>,
    /// Till sessions still open (they must be closed before the day can be).
    pub open_tills: i64,
    pub closed_at: Option<String>,
    pub closed_by: Option<String>,
    pub notes: Option<String>,
}

fn one(conn: &Connection, sql: &str, day: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [day], |r| r.get(0))?)
}

fn modes(conn: &Connection, direction: &str, day: &str) -> AppResult<Vec<ModeAmount>> {
    Ok(conn
        .prepare("SELECT mode, sum(amount) FROM payments WHERE status = 'finalized' AND direction = ?1 AND business_date = ?2 GROUP BY mode ORDER BY mode")?
        .query_map(params![direction, day], |r| Ok(ModeAmount { mode: r.get(0)?, amount: r.get(1)? }))?
        .collect::<Result<_, _>>()?)
}

pub fn day_summary_of(
    conn: &Connection,
    day: &str,
    timezone: chrono_tz::Tz,
) -> AppResult<DaySummary> {
    let (bills, sales_total, tax_total, discount_total): (i64, i64, i64, i64) = conn.query_row(
        "SELECT count(*), coalesce(sum(total), 0), coalesce(sum(tax_total), 0), coalesce(sum(line_discount + bill_discount), 0)
         FROM sales_invoices WHERE status = 'finalized' AND business_date = ?1",
        [day],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let tills = conn
        .prepare(
            "SELECT c.name, u.name, s.expected_cash, s.counted_cash, s.counted_cash - s.expected_cash, s.closed_at
             FROM register_sessions s JOIN cash_registers c ON c.id = s.register_id LEFT JOIN users u ON u.id = s.closed_by
             WHERE s.status = 'closed' ORDER BY s.closed_at",
        )?
        .query_map([], |r| {
            Ok((
                TillSummary {
                    register: r.get(0)?,
                    counted_by: r.get(1)?,
                    expected_cash: r.get(2)?,
                    counted_cash: r.get(3)?,
                    difference: r.get(4)?,
                },
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, closed_at)| {
            chrono::DateTime::parse_from_rfc3339(closed_at)
                .is_ok_and(|t| t.with_timezone(&timezone).format("%Y-%m-%d").to_string() == day)
        })
        .map(|(t, _)| t)
        .collect();
    let closed: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT d.closed_at, u.name, d.notes FROM day_closes d JOIN users u ON u.id = d.closed_by WHERE d.business_date = ?1",
            [day],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    Ok(DaySummary {
        business_date: day.to_string(),
        bills,
        sales_total,
        tax_total,
        discount_total,
        returns_total: one(conn, "SELECT coalesce(sum(total), 0) FROM sales_returns WHERE status = 'finalized' AND business_date = ?1", day)?,
        purchases_total: one(conn, "SELECT coalesce(sum(total), 0) FROM purchase_bills WHERE status = 'finalized' AND business_date = ?1", day)?,
        expenses_total: one(conn, "SELECT coalesce(sum(total), 0) FROM expenses WHERE status = 'finalized' AND business_date = ?1", day)?,
        received: modes(conn, "in", day)?,
        paid: modes(conn, "out", day)?,
        tills,
        open_tills: conn.query_row("SELECT count(*) FROM register_sessions WHERE status = 'open'", [], |r| r.get(0))?,
        closed_at: closed.as_ref().map(|c| c.0.clone()),
        closed_by: closed.as_ref().map(|c| c.1.clone()),
        notes: closed.and_then(|c| c.2),
    })
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DayInput {
    /// Default: today.
    pub business_date: Option<String>,
    pub notes: Option<String>,
}

pub fn day_summary(ctx: &Ctx<'_>, input: &DayInput) -> AppResult<DaySummary> {
    ctx.require("day_close.perform")?;
    let settings = super::settings::load(ctx.conn)?;
    let day = match &input.business_date {
        Some(d) => date("businessDate", d)?,
        None => settings.business_date(ctx.now),
    };
    day_summary_of(ctx.conn, &day, settings.timezone())
}

/// Records the day as closed with its summary. It is a record and a checkpoint (tills must be
/// closed first), not a lock: documents can still be made for the day with the right approval.
pub fn close_day(ctx: &mut Ctx<'_>, input: &DayInput) -> AppResult<DaySummary> {
    ctx.require("day_close.perform")?;
    let settings = super::settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let day = match &input.business_date {
        Some(d) => date("businessDate", d)?,
        None => today.clone(),
    };
    if day > today {
        return Err(AppError::field(
            "businessDate",
            "A day can't be closed before it has happened.",
        ));
    }
    let notes = optional("notes", input.notes.as_deref(), 500)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let tz = settings.timezone();
    write(ctx.conn, |tx| {
        let summary = day_summary_of(tx, &day, tz)?;
        if summary.closed_at.is_some() {
            return Err(AppError::conflict("This day is already closed."));
        }
        if summary.open_tills > 0 {
            return Err(AppError::conflict(
                "Close every till first (Point of sale → Close till).",
            ));
        }
        let (branch_id, _) = super::setup::default_location(tx)?;
        tx.execute(
            "INSERT INTO day_closes (id, branch_id, business_date, closed_at, closed_by, summary_json, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?4, ?4)",
            params![new_id(), branch_id, day, now, actor.user_id, json!(summary).to_string(), notes],
        )?;
        AuditEntry::new("DAY_CLOSED", "day_closes", Some(&day))
            .by(actor)
            .summary(format!(
                "{day}: sales {} in {} bills",
                summary.sales_total, summary.bills
            ))
            .write(tx, &now)?;
        day_summary_of(tx, &day, tz)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DayCloseRow {
    pub business_date: String,
    pub closed_at: String,
    pub closed_by: String,
    pub sales_total: i64,
    pub bills: i64,
    pub notes: Option<String>,
}

pub fn list_day_closes(ctx: &Ctx<'_>) -> AppResult<Vec<DayCloseRow>> {
    ctx.require("day_close.perform")?;
    Ok(ctx
        .conn
        .prepare(
            "SELECT d.business_date, d.closed_at, u.name, coalesce(json_extract(d.summary_json, '$.salesTotal'), 0),
                coalesce(json_extract(d.summary_json, '$.bills'), 0), d.notes
             FROM day_closes d JOIN users u ON u.id = d.closed_by ORDER BY d.business_date DESC LIMIT 90",
        )?
        .query_map([], |r| {
            Ok(DayCloseRow {
                business_date: r.get(0)?,
                closed_at: r.get(1)?,
                closed_by: r.get(2)?,
                sales_total: r.get(3)?,
                bills: r.get(4)?,
                notes: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

// ------------------------------------------------------------------ audit log

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    pub search: Option<String>,
    pub action: Option<String>,
    pub user_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditRow {
    pub id: String,
    pub occurred_at: String,
    pub user_name: Option<String>,
    pub approved_by_name: Option<String>,
    pub device_name: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub summary: Option<String>,
}

pub fn list_audit(ctx: &Ctx<'_>, query: &AuditQuery) -> AppResult<Page<AuditRow>> {
    ctx.require("audit.view")?;
    let mut filters = vec!["1 = 1".to_string()];
    let mut args: Vec<Sql> = Vec::new();
    if let Some(a) = query.action.as_deref().filter(|a| !a.is_empty()) {
        filters.push("a.action = ?".into());
        args.push(a.to_string().into());
    }
    if let Some(u) = query.user_id.as_deref().filter(|u| !u.is_empty()) {
        filters.push("a.user_id = ?".into());
        args.push(u.to_string().into());
    }
    if let Some(from) = &query.from {
        filters.push("substr(a.occurred_at, 1, 10) >= ?".into());
        args.push(date("from", from)?.into());
    }
    if let Some(to) = &query.to {
        filters.push("substr(a.occurred_at, 1, 10) <= ?".into());
        args.push(date("to", to)?.into());
    }
    if let Some(s) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        filters.push("(lower(coalesce(a.summary, '')) LIKE lower(?) ESCAPE '\\' OR lower(a.action) LIKE lower(?) ESCAPE '\\')".into());
        args.push(like_pattern(s).into());
        args.push(like_pattern(s).into());
    }
    let where_sql = filters.join(" AND ");
    let from_sql = "audit_log a LEFT JOIN users u ON u.id = a.user_id LEFT JOIN users ap ON ap.id = a.approved_by LEFT JOIN devices d ON d.id = a.device_id";
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM {from_sql} WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT a.id, a.occurred_at, u.name, ap.name, d.name, a.action, a.entity_type, a.entity_id, a.summary
             FROM {from_sql} WHERE {where_sql} ORDER BY a.occurred_at DESC, a.id DESC LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(AuditRow {
                id: r.get(0)?,
                occurred_at: r.get(1)?,
                user_name: r.get(2)?,
                approved_by_name: r.get(3)?,
                device_name: r.get(4)?,
                action: r.get(5)?,
                entity_type: r.get(6)?,
                entity_id: r.get(7)?,
                summary: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

/// The distinct actions in the log (for the filter box).
pub fn audit_actions(ctx: &Ctx<'_>) -> AppResult<Vec<String>> {
    ctx.require("audit.view")?;
    Ok(ctx
        .conn
        .prepare("SELECT DISTINCT action FROM audit_log ORDER BY action")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?)
}
