//! Money in and out outside billing: customer receipts, supplier payments, advances and their
//! later allocation, voids, and till (register) sessions with cash-up.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::{date, like_pattern, non_negative, optional, Page, PageQuery};
use super::idempotency;
use super::parties::open_documents;
use super::party_ledger;
use super::sales::{record_payment, validate_payments, void_payment, PaymentInput};
use super::settings;
use super::users::IdInput;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocationInput {
    /// "sales_invoice", "purchase_bill", "sales_return", "purchase_return" or "expense".
    pub target_type: String,
    pub target_id: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentEntryInput {
    pub idempotency_key: String,
    pub party_id: String,
    /// "in" (received) or "out" (paid).
    pub direction: String,
    pub payment: PaymentInput,
    pub business_date: Option<String>,
    #[serde(default)]
    pub allocations: Vec<AllocationInput>,
    /// Apply to the party's oldest open documents (ignored when allocations are given).
    #[serde(default)]
    pub auto_allocate: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocationDetail {
    pub target_type: String,
    pub target_id: String,
    pub doc_no: Option<String>,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDetail {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub direction: String,
    pub party_id: Option<String>,
    pub party_name: Option<String>,
    pub account_id: String,
    pub mode: String,
    pub amount: i64,
    pub tendered: Option<i64>,
    pub change_given: i64,
    pub reference_no: Option<String>,
    pub status: String,
    pub allocated: i64,
    pub unallocated: i64,
    pub notes: Option<String>,
    pub allocations: Vec<AllocationDetail>,
}

pub fn payment_detail(conn: &Connection, id: &str) -> AppResult<PaymentDetail> {
    let mut detail = conn
        .query_row(
            "SELECT p.id, p.doc_no, p.business_date, p.direction, p.party_id, pa.name, p.account_id, p.mode, p.amount, p.tendered,
                    p.change_given, p.reference_no, p.status, p.notes,
                    coalesce((SELECT sum(amount) FROM payment_allocations WHERE payment_id = p.id), 0)
             FROM payments p LEFT JOIN parties pa ON pa.id = p.party_id WHERE p.id = ?1",
            [id],
            |r| {
                let amount: i64 = r.get(8)?;
                let allocated: i64 = r.get(14)?;
                Ok(PaymentDetail {
                    id: r.get(0)?,
                    doc_no: r.get(1)?,
                    business_date: r.get(2)?,
                    direction: r.get(3)?,
                    party_id: r.get(4)?,
                    party_name: r.get(5)?,
                    account_id: r.get(6)?,
                    mode: r.get(7)?,
                    amount,
                    tendered: r.get(9)?,
                    change_given: r.get(10)?,
                    reference_no: r.get(11)?,
                    status: r.get(12)?,
                    notes: r.get(13)?,
                    allocated,
                    unallocated: amount - allocated,
                    allocations: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The payment"))?;
    detail.allocations = conn
        .prepare(
            "SELECT a.target_type, a.target_id,
               coalesce((SELECT doc_no FROM sales_invoices WHERE id = a.target_id), (SELECT doc_no FROM purchase_bills WHERE id = a.target_id),
                        (SELECT doc_no FROM sales_returns WHERE id = a.target_id), (SELECT doc_no FROM purchase_returns WHERE id = a.target_id),
                        (SELECT doc_no FROM expenses WHERE id = a.target_id)),
               a.amount
             FROM payment_allocations a WHERE a.payment_id = ?1 ORDER BY a.created_at",
        )?
        .query_map([id], |r| Ok(AllocationDetail { target_type: r.get(0)?, target_id: r.get(1)?, doc_no: r.get(2)?, amount: r.get(3)? }))?
        .collect::<Result<_, _>>()?;
    Ok(detail)
}

pub fn get(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<PaymentDetail> {
    ctx.require("parties.view")?;
    payment_detail(ctx.conn, &input.id)
}

fn kinds_for(direction: &str) -> &'static [&'static str] {
    if direction == "in" {
        &["sales_invoice", "purchase_return"]
    } else {
        &["purchase_bill", "sales_return", "expense"]
    }
}

fn allocate_to(
    tx: &Connection,
    payment_id: &str,
    party_id: &str,
    direction: &str,
    allocations: &[AllocationInput],
    user: &str,
    now: &str,
) -> AppResult<()> {
    let open = open_documents(tx, party_id)?;
    for (i, a) in allocations.iter().enumerate() {
        if !kinds_for(direction).contains(&a.target_type.as_str()) {
            return Err(AppError::field(
                &format!("allocations.{i}.targetType"),
                "That document can't be paid in this direction.",
            ));
        }
        let doc = open
            .iter()
            .find(|d| d.id == a.target_id && d.target_type == a.target_type)
            .ok_or_else(|| {
                AppError::field(
                    &format!("allocations.{i}.targetId"),
                    "That document isn't open for this party.",
                )
            })?;
        if a.amount <= 0 || a.amount > doc.balance_due {
            return Err(AppError::field(
                &format!("allocations.{i}.amount"),
                format!(
                    "Allocate between 0 and {} to {}.",
                    doc.balance_due, doc.doc_no
                ),
            ));
        }
        tx.execute(
            "INSERT INTO payment_allocations (id, payment_id, target_type, target_id, amount, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), payment_id, a.target_type, a.target_id, a.amount, now, user],
        )?;
    }
    Ok(())
}

/// Oldest-first allocation of `amount` over the party's open documents of the right kind.
fn auto_allocations(
    conn: &Connection,
    party_id: &str,
    direction: &str,
    amount: i64,
) -> AppResult<Vec<AllocationInput>> {
    let mut left = amount;
    let mut out = Vec::new();
    for doc in open_documents(conn, party_id)?
        .into_iter()
        .filter(|d| kinds_for(direction).contains(&d.target_type.as_str()))
    {
        if left == 0 {
            break;
        }
        let take = left.min(doc.balance_due);
        out.push(AllocationInput {
            target_type: doc.target_type,
            target_id: doc.id,
            amount: take,
        });
        left -= take;
    }
    Ok(out)
}

/// Customer receipt or supplier payment; unallocated money stays as an advance on the account.
pub fn record(ctx: &mut Ctx<'_>, input: &PaymentEntryInput) -> AppResult<PaymentDetail> {
    match input.direction.as_str() {
        "in" => ctx.require("payments.receive")?,
        "out" => ctx.require("payments.pay")?,
        _ => return Err(AppError::field("direction", "Use in or out.")),
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let business_date = input
        .business_date
        .as_deref()
        .map(|d| date("businessDate", d))
        .transpose()?
        .unwrap_or_else(|| today.clone());
    if business_date > today {
        return Err(AppError::field(
            "businessDate",
            "A payment can't be dated in the future.",
        ));
    }
    let approved_by = if business_date == today {
        None
    } else {
        ctx.approve("sales.backdate", "date a payment in the past")?
    };
    let party = party_ledger::load_party(
        ctx.conn,
        &input.party_id,
        if input.direction == "in" {
            "customer"
        } else {
            "supplier"
        },
    )
    .or_else(|_| {
        party_ledger::load_party(
            ctx.conn,
            &input.party_id,
            if input.direction == "in" {
                "supplier"
            } else {
                "customer"
            },
        )
    })?;
    let payment = validate_payments(ctx.conn, std::slice::from_ref(&input.payment))?.remove(0);
    let allocations = if !input.allocations.is_empty() {
        input.allocations.clone()
    } else if input.auto_allocate {
        auto_allocations(ctx.conn, &party.id, &input.direction, payment.amount)?
    } else {
        Vec::new()
    };
    let allocated: i64 = allocations.iter().map(|a| a.amount).sum();
    if allocated > payment.amount {
        return Err(AppError::field(
            "allocations",
            "Allocations are more than the payment.",
        ));
    }
    let counter = if party.party_type == "supplier"
        || (party.party_type == "both" && input.direction == "out")
    {
        accounts::PAYABLE
    } else {
        accounts::RECEIVABLE
    };
    let notes = optional("notes", input.notes.as_deref(), 500)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let (branch_id, _) = super::setup::default_location(ctx.conn)?;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "payments_record",
            actor,
            input,
            &now,
            || {
                let id = record_payment(
                    tx,
                    &settings,
                    &branch_id,
                    &business_date,
                    &input.direction,
                    Some(&party.id),
                    &payment,
                    None,
                    counter,
                    None,
                    actor,
                    &now,
                )?;
                if notes.is_some() {
                    tx.execute(
                        "UPDATE payments SET notes = ?1 WHERE id = ?2",
                        params![notes, id],
                    )?;
                }
                allocate_to(
                    tx,
                    &id,
                    &party.id,
                    &input.direction,
                    &allocations,
                    &actor.user_id,
                    &now,
                )?;
                AuditEntry::new(
                    if input.direction == "in" {
                        "PAYMENT_RECEIVED"
                    } else {
                        "PAYMENT_MADE"
                    },
                    "payments",
                    Some(&id),
                )
                .by(actor)
                .approved_by(approved_by.as_deref())
                .summary(format!("{} {}", party.name, payment.amount))
                .write(tx, &now)?;
                payment_detail(tx, &id)
            },
        )
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocateInput {
    pub payment_id: String,
    pub allocations: Vec<AllocationInput>,
}

/// Applies an advance (unallocated part of a payment) to open documents.
pub fn allocate(ctx: &mut Ctx<'_>, input: &AllocateInput) -> AppResult<PaymentDetail> {
    let now = ctx.now_str();
    let actor = ctx.actor;
    let payment = payment_detail(ctx.conn, &input.payment_id)?;
    ctx.require(if payment.direction == "in" {
        "payments.receive"
    } else {
        "payments.pay"
    })?;
    if payment.status != "finalized" {
        return Err(AppError::conflict("This payment is voided."));
    }
    let party_id = payment
        .party_id
        .clone()
        .ok_or_else(|| AppError::conflict("This payment has no party."))?;
    if input.allocations.iter().map(|a| a.amount).sum::<i64>() > payment.unallocated {
        return Err(AppError::field(
            "allocations",
            format!("Only {} is unallocated.", payment.unallocated),
        ));
    }
    write(ctx.conn, |tx| {
        allocate_to(
            tx,
            &payment.id,
            &party_id,
            &payment.direction,
            &input.allocations,
            &actor.user_id,
            &now,
        )?;
        AuditEntry::new("PAYMENT_ALLOCATED", "payments", Some(&payment.id))
            .by(actor)
            .write(tx, &now)?;
        payment_detail(tx, &payment.id)
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidPaymentInput {
    pub id: String,
    pub reason: String,
}

pub fn void(ctx: &mut Ctx<'_>, input: &VoidPaymentInput) -> AppResult<PaymentDetail> {
    let payment = payment_detail(ctx.conn, &input.id)?;
    let approved_by = ctx.approve(
        if payment.direction == "in" {
            "sales.void"
        } else {
            "purchases.manage"
        },
        "void a payment",
    )?;
    let reason = super::common::required("reason", &input.reason, 300)?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        void_payment(tx, &payment.id, &today, &reason, actor, &now)?;
        AuditEntry::new("PAYMENT_VOIDED", "payments", Some(&payment.id))
            .by(actor)
            .approved_by(approved_by.as_deref())
            .summary(format!("{}: {reason}", payment.doc_no))
            .write(tx, &now)?;
        payment_detail(tx, &payment.id)
    })
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentListQuery {
    pub direction: Option<String>,
    pub party_id: Option<String>,
    pub mode: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentListItem {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub direction: String,
    pub party_name: Option<String>,
    pub mode: String,
    pub amount: i64,
    pub status: String,
    pub reference_no: Option<String>,
}

pub fn list(ctx: &Ctx<'_>, query: &PaymentListQuery) -> AppResult<Page<PaymentListItem>> {
    ctx.require("parties.view")?;
    let mut filters = vec!["1 = 1".to_string()];
    let mut args: Vec<SqlValue> = Vec::new();
    for (column, value) in [
        ("p.direction", &query.direction),
        ("p.party_id", &query.party_id),
        ("p.mode", &query.mode),
    ] {
        if let Some(v) = value {
            filters.push(format!("{column} = ?"));
            args.push(v.clone().into());
        }
    }
    if let Some(from) = &query.from {
        filters.push("p.business_date >= ?".into());
        args.push(date("from", from)?.into());
    }
    if let Some(to) = &query.to {
        filters.push("p.business_date <= ?".into());
        args.push(date("to", to)?.into());
    }
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        filters.push("(p.doc_no LIKE ? ESCAPE '\\' OR p.reference_no LIKE ? ESCAPE '\\' OR lower(pa.name) LIKE lower(?) ESCAPE '\\')".into());
        let pattern = like_pattern(search);
        args.extend([
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.into(),
        ]);
    }
    let where_sql = filters.join(" AND ");
    let from_sql =
        format!("FROM payments p LEFT JOIN parties pa ON pa.id = p.party_id WHERE {where_sql}");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) {from_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT p.id, p.doc_no, p.business_date, p.direction, pa.name, p.mode, p.amount, p.status, p.reference_no {from_sql}
             ORDER BY p.business_date DESC, p.created_at DESC LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(PaymentListItem {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                direction: r.get(3)?,
                party_name: r.get(4)?,
                mode: r.get(5)?,
                amount: r.get(6)?,
                status: r.get(7)?,
                reference_no: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentAccount {
    pub id: String,
    pub code: String,
    pub name: String,
    pub subtype: String,
}

/// Cash and bank accounts money can move through.
pub fn list_accounts(ctx: &Ctx<'_>) -> AppResult<Vec<PaymentAccount>> {
    ctx.require("sales.create")
        .or_else(|_| ctx.require("payments.pay"))?;
    let rows = ctx
        .conn
        .prepare("SELECT id, code, name, subtype FROM accounts WHERE subtype IN ('cash', 'bank') AND is_active = 1 AND deleted_at IS NULL ORDER BY code")?
        .query_map([], |r| Ok(PaymentAccount { id: r.get(0)?, code: r.get(1)?, name: r.get(2)?, subtype: r.get(3)? }))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

// ------------------------------------------------------------------ till sessions

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenRegisterInput {
    pub register_id: Option<String>,
    pub opening_float: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloseRegisterInput {
    pub session_id: String,
    pub counted_cash: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSession {
    pub id: String,
    pub register_id: String,
    pub register_name: String,
    pub status: String,
    pub opened_at: String,
    pub opened_by_name: Option<String>,
    pub opening_float: i64,
    pub cash_in: i64,
    pub cash_out: i64,
    pub expected_cash: i64,
    pub counted_cash: Option<i64>,
    pub difference: Option<i64>,
    pub closed_at: Option<String>,
}

fn session(conn: &Connection, id: &str) -> AppResult<RegisterSession> {
    conn.query_row(
        "SELECT s.id, s.register_id, r.name, s.status, s.opened_at, u.name, s.opening_float,
           coalesce((SELECT sum(amount) FROM payments WHERE register_session_id = s.id AND status = 'finalized' AND mode = 'cash' AND direction = 'in'), 0),
           coalesce((SELECT sum(amount) FROM payments WHERE register_session_id = s.id AND status = 'finalized' AND mode = 'cash' AND direction = 'out'), 0),
           s.counted_cash, s.closed_at
         FROM register_sessions s JOIN cash_registers r ON r.id = s.register_id LEFT JOIN users u ON u.id = s.opened_by WHERE s.id = ?1",
        [id],
        |r| {
            let float: i64 = r.get(6)?;
            let cash_in: i64 = r.get(7)?;
            let cash_out: i64 = r.get(8)?;
            let counted: Option<i64> = r.get(9)?;
            let expected = float + cash_in - cash_out;
            Ok(RegisterSession {
                id: r.get(0)?,
                register_id: r.get(1)?,
                register_name: r.get(2)?,
                status: r.get(3)?,
                opened_at: r.get(4)?,
                opened_by_name: r.get(5)?,
                opening_float: float,
                cash_in,
                cash_out,
                expected_cash: expected,
                counted_cash: counted,
                difference: counted.map(|c| c - expected),
                closed_at: r.get(10)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The till session"))
}

pub fn open_register(ctx: &mut Ctx<'_>, input: &OpenRegisterInput) -> AppResult<RegisterSession> {
    ctx.require("sales.create")?;
    non_negative("openingFloat", input.opening_float)?;
    let register_id = match &input.register_id {
        Some(id) => id.clone(),
        None => ctx
            .conn
            .query_row("SELECT id FROM cash_registers WHERE deleted_at IS NULL AND is_active = 1 ORDER BY created_at LIMIT 1", [], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::not_found("A till"))?,
    };
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = new_id();
    write(ctx.conn, |tx| {
        let open: i64 = tx.query_row(
            "SELECT count(*) FROM register_sessions WHERE register_id = ?1 AND status = 'open'",
            [&register_id],
            |r| r.get(0),
        )?;
        if open > 0 {
            return Err(AppError::conflict("This till is already open."));
        }
        tx.execute(
            "INSERT INTO register_sessions (id, register_id, status, opened_at, opened_by, opening_float, created_at, updated_at)
             VALUES (?1, ?2, 'open', ?3, ?4, ?5, ?3, ?3)",
            params![id, register_id, now, actor.user_id, input.opening_float],
        )?;
        AuditEntry::new("TILL_OPENED", "register_sessions", Some(&id))
            .by(actor)
            .summary(format!("float {}", input.opening_float))
            .write(tx, &now)
    })?;
    session(ctx.conn, &id)
}

/// Closes a till with the counted cash; the difference to the expected cash is recorded and audited.
pub fn close_register(ctx: &mut Ctx<'_>, input: &CloseRegisterInput) -> AppResult<RegisterSession> {
    ctx.require("sales.create")?;
    non_negative("countedCash", input.counted_cash)?;
    let notes = optional("notes", input.notes.as_deref(), 500)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let current = session(tx, &input.session_id)?;
        if current.status != "open" {
            return Err(AppError::conflict("This till session is already closed."));
        }
        tx.execute(
            "UPDATE register_sessions SET status = 'closed', closed_at = ?1, closed_by = ?2, expected_cash = ?3, counted_cash = ?4, notes = ?5, updated_at = ?1
             WHERE id = ?6",
            params![now, actor.user_id, current.expected_cash, input.counted_cash, notes, input.session_id],
        )?;
        AuditEntry::new("TILL_CLOSED", "register_sessions", Some(&input.session_id))
            .by(actor)
            .summary(format!(
                "expected {} counted {}",
                current.expected_cash, input.counted_cash
            ))
            .write(tx, &now)?;
        session(tx, &input.session_id)
    })
}

pub fn current_register(ctx: &Ctx<'_>) -> AppResult<Option<RegisterSession>> {
    ctx.require("sales.create")?;
    let id: Option<String> = ctx
        .conn
        .query_row("SELECT id FROM register_sessions WHERE status = 'open' AND opened_by = ?1 ORDER BY opened_at DESC LIMIT 1", [&ctx.actor.user_id], |r| r.get(0))
        .optional()?;
    id.map(|id| session(ctx.conn, &id)).transpose()
}
