//! Customers and suppliers. Balances only ever change through the party ledger (documents,
//! payments, an opening balance at creation): the v1 "edit the due amount" field is gone (B-14).

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::{
    date, like_pattern, non_negative, optional, optional_email, optional_gstin, optional_phone,
    required, Page, PageQuery,
};
use super::journal::{self, Entry, JLine};
use super::party_ledger::{self, Posting};
use super::settings;
use super::users::IdInput;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyInput {
    pub id: Option<String>,
    pub expected_updated_at: Option<String>,
    /// "customer", "supplier" or "both".
    pub party_type: String,
    pub code: Option<String>,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub gstin: Option<String>,
    pub state_code: Option<String>,
    pub billing_address: Option<String>,
    pub shipping_address: Option<String>,
    pub credit_limit: Option<i64>,
    pub credit_days: Option<i64>,
    pub notes: Option<String>,
    #[serde(default = "yes")]
    pub is_active: bool,
    /// New parties only: > 0 they owe us, < 0 we owe them.
    pub opening_balance: Option<i64>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyDetail {
    pub id: String,
    pub party_type: String,
    pub code: Option<String>,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub gstin: Option<String>,
    pub state_code: Option<String>,
    pub billing_address: Option<String>,
    pub shipping_address: Option<String>,
    pub credit_limit: Option<i64>,
    pub credit_days: Option<i64>,
    /// > 0 the party owes us; < 0 we owe the party.
    pub balance: i64,
    pub notes: Option<String>,
    pub is_active: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyListQuery {
    pub search: Option<String>,
    /// "customer" or "supplier" (parties of type "both" appear in both lists).
    pub party_type: Option<String>,
    #[serde(default)]
    pub with_balance_only: bool,
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyListItem {
    pub id: String,
    pub party_type: String,
    pub name: String,
    pub phone: Option<String>,
    pub gstin: Option<String>,
    pub balance: i64,
    pub credit_limit: Option<i64>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementQuery {
    pub party_id: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementLine {
    pub business_date: String,
    pub entry_type: String,
    pub source_type: String,
    pub source_id: String,
    pub doc_no: Option<String>,
    pub narration: Option<String>,
    pub debit: i64,
    pub credit: i64,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyStatement {
    pub party: PartyDetail,
    pub from: Option<String>,
    pub to: Option<String>,
    pub opening_balance: i64,
    pub lines: Vec<StatementLine>,
    pub closing_balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocument {
    /// "sales_invoice", "purchase_bill", "sales_return", "purchase_return" or "expense".
    pub target_type: String,
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub total: i64,
    pub paid: i64,
    pub balance_due: i64,
}

const PARTY_SELECT: &str = "SELECT id, party_type, code, name, phone, email, gstin, state_code, billing_address, shipping_address,
    credit_limit, credit_days, balance, notes, is_active, updated_at FROM parties";

fn map_party(r: &rusqlite::Row<'_>) -> rusqlite::Result<PartyDetail> {
    Ok(PartyDetail {
        id: r.get(0)?,
        party_type: r.get(1)?,
        code: r.get(2)?,
        name: r.get(3)?,
        phone: r.get(4)?,
        email: r.get(5)?,
        gstin: r.get(6)?,
        state_code: r.get(7)?,
        billing_address: r.get(8)?,
        shipping_address: r.get(9)?,
        credit_limit: r.get(10)?,
        credit_days: r.get(11)?,
        balance: r.get(12)?,
        notes: r.get(13)?,
        is_active: r.get(14)?,
        updated_at: r.get(15)?,
    })
}

pub fn get_unchecked(conn: &Connection, id: &str) -> AppResult<PartyDetail> {
    conn.query_row(
        &format!("{PARTY_SELECT} WHERE id = ?1 AND deleted_at IS NULL"),
        [id],
        map_party,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The customer or supplier"))
}

pub fn get(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<PartyDetail> {
    ctx.require("parties.view")?;
    get_unchecked(ctx.conn, &input.id)
}

pub fn save(ctx: &mut Ctx<'_>, input: &PartyInput) -> AppResult<PartyDetail> {
    ctx.require("parties.manage")?;
    if !matches!(input.party_type.as_str(), "customer" | "supplier" | "both") {
        return Err(AppError::field(
            "partyType",
            "Choose customer, supplier or both.",
        ));
    }
    let name = required("name", &input.name, 160)?;
    let code = optional("code", input.code.as_deref(), 30)?;
    let phone = optional_phone("phone", input.phone.as_deref())?;
    let email = optional_email("email", input.email.as_deref())?;
    let gstin = optional_gstin("gstin", input.gstin.as_deref())?;
    let state_code = optional("stateCode", input.state_code.as_deref(), 2)?
        .or_else(|| gstin.as_ref().map(|g| g[..2].to_string()));
    if state_code
        .as_ref()
        .is_some_and(|s| s.len() != 2 || !s.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(AppError::field(
            "stateCode",
            "Use the two-digit GST state code.",
        ));
    }
    let billing = optional("billingAddress", input.billing_address.as_deref(), 500)?;
    let shipping = optional("shippingAddress", input.shipping_address.as_deref(), 500)?;
    let notes = optional("notes", input.notes.as_deref(), 2000)?;
    if let Some(limit) = input.credit_limit {
        non_negative("creditLimit", limit)?;
    }
    if let Some(days) = input.credit_days {
        non_negative("creditDays", days)?;
    }
    if input.id.is_some() && input.opening_balance.is_some_and(|b| b != 0) {
        return Err(AppError::field(
            "openingBalance",
            "An opening balance can only be set when creating the party.",
        ));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let id = match &input.id {
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO parties (id, party_type, code, name, phone, email, gstin, state_code, billing_address, shipping_address,
                        credit_limit, credit_days, notes, is_active, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16, ?16)",
                    params![
                        id, input.party_type, code, name, phone, email, gstin, state_code, billing, shipping,
                        input.credit_limit, input.credit_days, notes, input.is_active, now, actor.user_id
                    ],
                )?;
                let opening = input.opening_balance.unwrap_or(0);
                if opening != 0 {
                    party_ledger::post(
                        tx,
                        Posting {
                            party_id: &id,
                            business_date: &today,
                            entry_type: "opening",
                            source_type: "party_opening",
                            source_id: &id,
                            amount: opening,
                            narration: "Opening balance".into(),
                            user_id: Some(&actor.user_id),
                            now: &now,
                        },
                    )?;
                    let control = if input.party_type == "supplier" {
                        accounts::PAYABLE
                    } else {
                        accounts::RECEIVABLE
                    };
                    journal::post(
                        tx,
                        Entry {
                            business_date: &today,
                            source_type: "party_opening",
                            source_id: &id,
                            narration: format!("Opening balance: {name}"),
                            lines: vec![
                                JLine::debit(control, opening).party(Some(&id)),
                                JLine::credit(accounts::OPENING_BALANCES, opening),
                            ],
                            user_id: Some(&actor.user_id),
                            now: &now,
                        },
                    )?;
                }
                AuditEntry::new("PARTY_CREATED", "parties", Some(&id))
                    .by(actor)
                    .summary(name.clone())
                    .write(tx, &now)?;
                id
            }
            Some(id) => {
                let before = get_unchecked(tx, id)?;
                if input
                    .expected_updated_at
                    .as_ref()
                    .is_some_and(|e| *e != before.updated_at)
                {
                    return Err(AppError::conflict(
                        "Someone else changed this record. Reload it and try again.",
                    ));
                }
                tx.execute(
                    "UPDATE parties SET party_type = ?1, code = ?2, name = ?3, phone = ?4, email = ?5, gstin = ?6, state_code = ?7,
                       billing_address = ?8, shipping_address = ?9, credit_limit = ?10, credit_days = ?11, notes = ?12, is_active = ?13,
                       updated_at = ?14, updated_by = ?15 WHERE id = ?16",
                    params![
                        input.party_type, code, name, phone, email, gstin, state_code, billing, shipping, input.credit_limit,
                        input.credit_days, notes, input.is_active, now, actor.user_id, id
                    ],
                )?;
                if before.credit_limit != input.credit_limit {
                    AuditEntry::new("CREDIT_LIMIT_CHANGE", "parties", Some(id))
                        .by(actor)
                        .before(serde_json::json!({ "creditLimit": before.credit_limit }))
                        .after(serde_json::json!({ "creditLimit": input.credit_limit }))
                        .write(tx, &now)?;
                }
                id.clone()
            }
        };
        get_unchecked(tx, &id)
    })
}

pub fn delete(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("parties.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let party = get_unchecked(tx, &input.id)?;
        if party.balance != 0 {
            return Err(AppError::conflict(
                "This party still has a balance. Settle it before deleting.",
            ));
        }
        tx.execute(
            "UPDATE parties SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("PARTY_DELETED", "parties", Some(&input.id))
            .by(actor)
            .summary(party.name)
            .write(tx, &now)
    })
}

pub fn list(ctx: &Ctx<'_>, query: &PartyListQuery) -> AppResult<Page<PartyListItem>> {
    ctx.require("parties.view")?;
    let mut filters = vec!["deleted_at IS NULL".to_string()];
    let mut args: Vec<SqlValue> = Vec::new();
    if !query.include_inactive {
        filters.push("is_active = 1".into());
    }
    match query.party_type.as_deref() {
        Some(t @ ("customer" | "supplier")) => {
            filters.push("party_type IN (?, 'both')".into());
            args.push(t.to_string().into());
        }
        Some(_) => return Err(AppError::field("partyType", "Use customer or supplier.")),
        None => {}
    }
    if query.with_balance_only {
        filters.push("balance <> 0".into());
    }
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        filters.push("(lower(name) LIKE lower(?) ESCAPE '\\' OR phone LIKE ? ESCAPE '\\' OR gstin = upper(?) OR lower(code) = lower(?))".into());
        let pattern = like_pattern(search);
        args.extend([
            pattern.clone().into(),
            pattern.into(),
            search.to_string().into(),
            search.to_string().into(),
        ]);
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM parties WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("name", "lower(name)"),
            ("type", "party_type"),
            ("phone", "coalesce(phone, '')"),
            ("gstin", "coalesce(gstin, '')"),
            ("balance", "balance"),
            ("creditLimit", "coalesce(credit_limit, 0)"),
        ],
        "lower(name), id",
        "id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let mut stmt = ctx.conn.prepare(&format!(
        "SELECT id, party_type, name, phone, gstin, balance, credit_limit, is_active FROM parties WHERE {where_sql}
         ORDER BY {order} LIMIT ? OFFSET ?"
    ))?;
    let items = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(PartyListItem {
                id: r.get(0)?,
                party_type: r.get(1)?,
                name: r.get(2)?,
                phone: r.get(3)?,
                gstin: r.get(4)?,
                balance: r.get(5)?,
                credit_limit: r.get(6)?,
                is_active: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

/// Party statement with running balance; dates are business dates.
pub fn statement(ctx: &Ctx<'_>, query: &StatementQuery) -> AppResult<PartyStatement> {
    ctx.require("parties.view")?;
    let party = get_unchecked(ctx.conn, &query.party_id)?;
    let from = query.from.as_deref().map(|d| date("from", d)).transpose()?;
    let to = query.to.as_deref().map(|d| date("to", d)).transpose()?;
    let opening: i64 = match &from {
        Some(from) => ctx.conn.query_row(
            "SELECT coalesce(sum(debit - credit), 0) FROM party_ledger WHERE party_id = ?1 AND business_date < ?2",
            params![party.id, from],
            |r| r.get(0),
        )?,
        None => 0,
    };
    let mut stmt = ctx.conn.prepare(
        "SELECT l.business_date, l.entry_type, l.source_type, l.source_id,
           coalesce((SELECT doc_no FROM sales_invoices WHERE id = l.source_id),
                    (SELECT doc_no FROM sales_returns WHERE id = l.source_id),
                    (SELECT doc_no FROM purchase_bills WHERE id = l.source_id),
                    (SELECT doc_no FROM purchase_returns WHERE id = l.source_id),
                    (SELECT doc_no FROM payments WHERE id = l.source_id),
                    (SELECT doc_no FROM expenses WHERE id = l.source_id)),
           l.narration, l.debit, l.credit
         FROM party_ledger l
         WHERE l.party_id = ?1 AND (?2 IS NULL OR l.business_date >= ?2) AND (?3 IS NULL OR l.business_date <= ?3)
         ORDER BY l.business_date, l.created_at, l.rowid",
    )?;
    let mut balance = opening;
    let lines = stmt
        .query_map(params![party.id, from, to], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })?
        .map(|row| {
            let (
                business_date,
                entry_type,
                source_type,
                source_id,
                doc_no,
                narration,
                debit,
                credit,
            ) = row?;
            balance += debit - credit;
            Ok(StatementLine {
                business_date,
                entry_type,
                source_type,
                source_id,
                doc_no,
                narration,
                debit,
                credit,
                balance,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(PartyStatement {
        party,
        from,
        to,
        opening_balance: opening,
        closing_balance: balance,
        lines,
    })
}

/// Unpaid documents of a party, oldest first (for allocating a payment).
pub fn open_documents(conn: &Connection, party_id: &str) -> AppResult<Vec<OpenDocument>> {
    let mut stmt = conn.prepare(
        "SELECT 'sales_invoice', id, doc_no, business_date, total, amount_paid FROM sales_invoices
           WHERE party_id = ?1 AND status = 'finalized' AND amount_paid < total
         UNION ALL
         SELECT 'purchase_bill', id, doc_no, business_date, total, amount_paid FROM purchase_bills
           WHERE party_id = ?1 AND status = 'finalized' AND amount_paid < total
         UNION ALL
         SELECT 'sales_return', id, doc_no, business_date, total, amount_refunded FROM sales_returns
           WHERE party_id = ?1 AND status = 'finalized' AND amount_refunded < total
         UNION ALL
         SELECT 'purchase_return', id, doc_no, business_date, total, amount_refunded FROM purchase_returns
           WHERE party_id = ?1 AND status = 'finalized' AND amount_refunded < total
         UNION ALL
         SELECT 'expense', id, doc_no, business_date, total, amount_paid FROM expenses
           WHERE party_id = ?1 AND status = 'finalized' AND amount_paid < total
         ORDER BY 4, 3",
    )?;
    let rows = stmt
        .query_map([party_id], |r| {
            let total: i64 = r.get(4)?;
            let paid: i64 = r.get(5)?;
            Ok(OpenDocument {
                target_type: r.get(0)?,
                id: r.get(1)?,
                doc_no: r.get(2)?,
                business_date: r.get(3)?,
                total,
                paid,
                balance_due: total - paid,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn list_open_documents(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<Vec<OpenDocument>> {
    ctx.require("parties.view")?;
    open_documents(ctx.conn, &input.id)
}
