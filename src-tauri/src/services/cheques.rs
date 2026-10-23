//! Post-dated cheques. A cheque received (or given) with a future date is written down here; it
//! is not money yet, so nothing is posted. When it clears, the receipt (or payment) is recorded
//! through the normal payment service, dated the clearing day, and the cheque points to it. A
//! cheque that bounces after clearing voids that payment. The due list shows what to deposit.

use rusqlite::types::Value as Sql;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::{date, optional, positive, required, Page, PageQuery};
use super::idempotency;
use super::party_ledger;
use super::payments::{self, PaymentEntryInput, VoidPaymentInput};
use super::sales::PaymentInput;
use super::settings;
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

fn require_direction(ctx: &Ctx<'_>, direction: &str) -> AppResult<()> {
    match direction {
        "in" => ctx.require("payments.receive"),
        "out" => ctx.require("payments.pay"),
        _ => Err(AppError::field("direction", "Use in or out.")),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Cheque {
    pub id: String,
    /// "in" (received from a customer) or "out" (given to a supplier).
    pub direction: String,
    pub party_id: String,
    pub party_name: String,
    pub account_id: String,
    pub account_name: String,
    pub cheque_no: String,
    pub bank_name: Option<String>,
    pub cheque_date: String,
    pub amount: i64,
    /// pending, cleared, bounced or cancelled.
    pub status: String,
    pub payment_id: Option<String>,
    pub payment_doc_no: Option<String>,
    pub cleared_date: Option<String>,
    pub status_note: Option<String>,
    pub notes: Option<String>,
    /// Days until the cheque's date (negative: overdue).
    pub days_left: i64,
    pub updated_at: String,
}

fn cheque(conn: &Connection, id: &str, today: &str) -> AppResult<Cheque> {
    conn.query_row(
        &format!("{SELECT} WHERE c.id = ?1 AND c.deleted_at IS NULL"),
        [id],
        row,
    )
    .optional()?
    .map(|c| with_days(c, today))
    .ok_or_else(|| AppError::not_found("The cheque"))
}

fn with_days(mut c: Cheque, today: &str) -> Cheque {
    let parse = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    if let (Some(due), Some(now)) = (parse(&c.cheque_date), parse(today)) {
        c.days_left = (due - now).num_days();
    }
    c
}

const SELECT: &str = "SELECT c.id, c.direction, c.party_id, p.name, c.account_id, a.name, c.cheque_no, c.bank_name, c.cheque_date,
    c.amount, c.status, c.payment_id, pay.doc_no, c.cleared_date, c.status_note, c.notes, c.updated_at
  FROM cheques c JOIN parties p ON p.id = c.party_id JOIN accounts a ON a.id = c.account_id
  LEFT JOIN payments pay ON pay.id = c.payment_id";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Cheque> {
    Ok(Cheque {
        id: r.get(0)?,
        direction: r.get(1)?,
        party_id: r.get(2)?,
        party_name: r.get(3)?,
        account_id: r.get(4)?,
        account_name: r.get(5)?,
        cheque_no: r.get(6)?,
        bank_name: r.get(7)?,
        cheque_date: r.get(8)?,
        amount: r.get(9)?,
        status: r.get(10)?,
        payment_id: r.get(11)?,
        payment_doc_no: r.get(12)?,
        cleared_date: r.get(13)?,
        status_note: r.get(14)?,
        notes: r.get(15)?,
        days_left: 0,
        updated_at: r.get(16)?,
    })
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChequeQuery {
    pub status: Option<String>,
    pub direction: Option<String>,
    /// Pending cheques dated on or before this day.
    pub due_by: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

pub fn list(ctx: &Ctx<'_>, q: &ChequeQuery) -> AppResult<Page<Cheque>> {
    ctx.require("payments.receive")
        .or_else(|_| ctx.require("payments.pay"))?;
    let today = settings::load(ctx.conn)?.business_date(ctx.now);
    let mut filters = vec!["c.deleted_at IS NULL".to_string()];
    let mut args: Vec<Sql> = Vec::new();
    if let Some(s) = q.status.as_deref().filter(|s| !s.is_empty()) {
        if !matches!(s, "pending" | "cleared" | "bounced" | "cancelled") {
            return Err(AppError::field("status", "Unknown status."));
        }
        filters.push(format!("c.status = ?{}", args.len() + 1));
        args.push(s.to_string().into());
    }
    if let Some(d) = q.direction.as_deref().filter(|s| !s.is_empty()) {
        filters.push(format!("c.direction = ?{}", args.len() + 1));
        args.push(d.to_string().into());
    }
    if let Some(d) = &q.due_by {
        filters.push(format!(
            "c.status = 'pending' AND c.cheque_date <= ?{}",
            args.len() + 1
        ));
        args.push(date("dueBy", d)?.into());
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM cheques c WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = q.page.order_by(
        &[
            ("date", "c.cheque_date"),
            ("amount", "c.amount"),
            ("party", "lower(p.name)"),
            ("status", "c.status"),
        ],
        "c.status = 'pending' DESC, c.cheque_date, c.created_at",
        "c.id",
    )?;
    let (limit, offset) = q.page.limit_offset();
    let n = args.len();
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "{SELECT} WHERE {where_sql} ORDER BY {order} LIMIT ?{} OFFSET ?{}",
            n + 1,
            n + 2
        ))?
        .query_map(params_from_iter(args.iter()), row)?
        .map(|c| c.map(|c| with_days(c, &today)))
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &q.page))
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChequeInput {
    pub id: Option<String>,
    pub expected_updated_at: Option<String>,
    pub idempotency_key: Option<String>,
    pub direction: String,
    pub party_id: String,
    /// The bank account it is (or will be) deposited into or paid from.
    pub account_id: String,
    pub cheque_no: String,
    pub bank_name: Option<String>,
    pub cheque_date: String,
    pub amount: i64,
    pub notes: Option<String>,
}

/// Records a cheque (or edits a pending one).
pub fn save(ctx: &mut Ctx<'_>, input: &ChequeInput) -> AppResult<Cheque> {
    require_direction(ctx, &input.direction)?;
    let cheque_no = required("chequeNo", &input.cheque_no, 20)?;
    let bank_name = optional("bankName", input.bank_name.as_deref(), 80)?;
    let notes = optional("notes", input.notes.as_deref(), 300)?;
    let cheque_date = date("chequeDate", &input.cheque_date)?;
    let amount = positive("amount", input.amount)?;
    party_ledger::load_party(
        ctx.conn,
        &input.party_id,
        if input.direction == "in" {
            "customer"
        } else {
            "supplier"
        },
    )?;
    let bank: bool = ctx.conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM accounts WHERE id = ?1 AND subtype = 'bank' AND is_active = 1 AND deleted_at IS NULL)",
        [&input.account_id],
        |r| r.get(0),
    )?;
    if !bank {
        return Err(AppError::field("accountId", "Choose a bank account."));
    }
    let today = settings::load(ctx.conn)?.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| match &input.id {
        None => {
            let key = input
                .idempotency_key
                .as_deref()
                .ok_or_else(|| AppError::field("idempotencyKey", "A request id is required."))?;
            idempotency::once(tx, key, "cheques_save", actor, input, &now, || {
                let id = new_id();
                tx.execute(
                    "INSERT INTO cheques (id, direction, party_id, account_id, cheque_no, bank_name, cheque_date, amount, notes, idempotency_key,
                        created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?12)",
                    params![id, input.direction, input.party_id, input.account_id, cheque_no, bank_name, cheque_date, amount, notes, key, now, actor.user_id],
                )?;
                AuditEntry::new("CHEQUE_RECORDED", "cheques", Some(&id))
                    .by(actor)
                    .summary(format!(
                        "{} no. {cheque_no} dated {cheque_date}, {amount}",
                        input.direction
                    ))
                    .write(tx, &now)?;
                Ok(id)
            })
        }
        Some(id) => {
            let current = cheque(tx, id, &today)?;
            if input
                .expected_updated_at
                .as_ref()
                .is_some_and(|e| *e != current.updated_at)
            {
                return Err(AppError::conflict(
                    "Someone else changed this cheque. Reload it and try again.",
                ));
            }
            if current.status != "pending" {
                return Err(AppError::conflict("Only a pending cheque can be edited."));
            }
            if current.direction != input.direction {
                return Err(AppError::field(
                    "direction",
                    "A cheque's direction can't change.",
                ));
            }
            tx.execute(
                "UPDATE cheques SET party_id = ?1, account_id = ?2, cheque_no = ?3, bank_name = ?4, cheque_date = ?5, amount = ?6, notes = ?7,
                   updated_at = ?8, updated_by = ?9 WHERE id = ?10",
                params![input.party_id, input.account_id, cheque_no, bank_name, cheque_date, amount, notes, now, actor.user_id, id],
            )?;
            AuditEntry::new("CHEQUE_EDITED", "cheques", Some(id))
                .by(actor)
                .summary(format!("no. {cheque_no} dated {cheque_date}, {amount}"))
                .write(tx, &now)?;
            Ok(id.clone())
        }
    })?;
    cheque(ctx.conn, &id, &today)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChequeClearInput {
    pub id: String,
    /// The day the bank credited (or debited) it; default today.
    pub cleared_date: Option<String>,
    /// Apply the receipt to the party's oldest open bills.
    #[serde(default)]
    pub auto_allocate: bool,
}

/// Records the payment for a cheque that cleared. Safe to retry: the payment's request id is the
/// cheque's, so a second try finds the same payment.
pub fn clear(ctx: &mut Ctx<'_>, input: &ChequeClearInput) -> AppResult<Cheque> {
    let today = settings::load(ctx.conn)?.business_date(ctx.now);
    let current = cheque(ctx.conn, &input.id, &today)?;
    require_direction(ctx, &current.direction)?;
    if current.status != "pending" {
        return Err(AppError::conflict("Only a pending cheque can clear."));
    }
    let cleared = match &input.cleared_date {
        Some(d) => date("clearedDate", d)?,
        None => today.clone(),
    };
    if cleared < current.cheque_date {
        return Err(AppError::field(
            "clearedDate",
            "A cheque can't clear before its date.",
        ));
    }
    let payment = payments::record(
        ctx,
        &PaymentEntryInput {
            idempotency_key: format!("cheque-clear-{}", current.id),
            party_id: current.party_id.clone(),
            direction: current.direction.clone(),
            payment: PaymentInput {
                mode: "cheque".into(),
                amount: current.amount,
                tendered: None,
                account_id: Some(current.account_id.clone()),
                reference: Some(current.cheque_no.clone()),
            },
            business_date: Some(cleared.clone()),
            allocations: Vec::new(),
            auto_allocate: input.auto_allocate,
            notes: Some(format!("Cheque {}", current.cheque_no)),
        },
    )?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let changed = tx.execute(
            "UPDATE cheques SET status = 'cleared', payment_id = ?1, cleared_date = ?2, updated_at = ?3, updated_by = ?4
             WHERE id = ?5 AND status = 'pending'",
            params![payment.id, cleared, now, actor.user_id, current.id],
        )?;
        if changed == 0 {
            return Err(AppError::conflict("This cheque is no longer pending."));
        }
        AuditEntry::new("CHEQUE_CLEARED", "cheques", Some(&current.id))
            .by(actor)
            .summary(format!("no. {} as {}", current.cheque_no, payment.doc_no))
            .write(tx, &now)
    })?;
    cheque(ctx.conn, &input.id, &today)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChequeStatusInput {
    pub id: String,
    pub reason: String,
}

/// A cheque that bounced: a cleared one has its payment voided (the party owes it again).
pub fn bounce(ctx: &mut Ctx<'_>, input: &ChequeStatusInput) -> AppResult<Cheque> {
    finish(ctx, input, "bounced")
}

/// A pending cheque that won't be deposited (returned to the party, replaced, lost).
pub fn cancel(ctx: &mut Ctx<'_>, input: &ChequeStatusInput) -> AppResult<Cheque> {
    finish(ctx, input, "cancelled")
}

fn finish(ctx: &mut Ctx<'_>, input: &ChequeStatusInput, to: &str) -> AppResult<Cheque> {
    let reason = required("reason", &input.reason, 300)?;
    let today = settings::load(ctx.conn)?.business_date(ctx.now);
    let current = cheque(ctx.conn, &input.id, &today)?;
    require_direction(ctx, &current.direction)?;
    let allowed = match to {
        "bounced" => matches!(current.status.as_str(), "pending" | "cleared"),
        _ => current.status == "pending",
    };
    if !allowed {
        return Err(AppError::conflict(format!(
            "A {} cheque can't be marked {to}.",
            current.status
        )));
    }
    if let Some(payment_id) = &current.payment_id {
        let status: String = ctx.conn.query_row(
            "SELECT status FROM payments WHERE id = ?1",
            [payment_id],
            |r| r.get(0),
        )?;
        if status == "finalized" {
            payments::void(
                ctx,
                &VoidPaymentInput {
                    id: payment_id.clone(),
                    reason: format!("Cheque {} bounced: {reason}", current.cheque_no),
                },
            )?;
        }
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "UPDATE cheques SET status = ?1, status_note = ?2, updated_at = ?3, updated_by = ?4 WHERE id = ?5",
            params![to, reason, now, actor.user_id, current.id],
        )?;
        AuditEntry::new(
            if to == "bounced" {
                "CHEQUE_BOUNCED"
            } else {
                "CHEQUE_CANCELLED"
            },
            "cheques",
            Some(&current.id),
        )
        .by(actor)
        .summary(format!("no. {}: {reason}", current.cheque_no))
        .write(tx, &now)
    })?;
    cheque(ctx.conn, &input.id, &today)
}

/// A cheque due for the notice: (id, direction, number, party, amount).
pub type DueCheque = (String, String, String, String, i64);

/// Pending cheques due by `day` (for the daily notice).
pub fn due(conn: &Connection, day: &str) -> AppResult<Vec<DueCheque>> {
    Ok(conn
        .prepare(
            "SELECT c.id, c.direction, c.cheque_no, p.name, c.amount FROM cheques c JOIN parties p ON p.id = c.party_id
             WHERE c.status = 'pending' AND c.deleted_at IS NULL AND c.cheque_date <= ?1 ORDER BY c.cheque_date",
        )?
        .query_map([day], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<Result<_, _>>()?)
}
