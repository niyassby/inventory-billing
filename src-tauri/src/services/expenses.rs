//! Expenses (rent, utilities, wages…), paid now or on credit with a supplier.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::{date, like_pattern, optional, positive, rate, required, Page, PageQuery};
use super::idempotency;
use super::journal::{self, Entry, JLine};
use super::numbering;
use super::party_ledger::{self, Posting};
use super::period;
use super::sales::{record_payment, validate_payments, void_payment, PaymentInput};
use super::settings;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseCategory {
    pub id: String,
    pub name: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseCategoryInput {
    pub id: Option<String>,
    pub name: String,
    /// Expense account to post to (default: General Expenses).
    pub account_id: Option<String>,
}

pub fn list_categories(ctx: &Ctx<'_>) -> AppResult<Vec<ExpenseCategory>> {
    ctx.require("expenses.manage")?;
    let rows = ctx
        .conn
        .prepare("SELECT id, name, account_id FROM expense_categories WHERE deleted_at IS NULL ORDER BY lower(name)")?
        .query_map([], |r| Ok(ExpenseCategory { id: r.get(0)?, name: r.get(1)?, account_id: r.get(2)? }))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn save_category(
    ctx: &mut Ctx<'_>,
    input: &ExpenseCategoryInput,
) -> AppResult<ExpenseCategory> {
    ctx.require("expenses.manage")?;
    let name = required("name", &input.name, 80)?;
    let account = input
        .account_id
        .clone()
        .unwrap_or_else(|| accounts::GENERAL_EXPENSES.to_string());
    let kind: Option<String> = ctx
        .conn
        .query_row(
            "SELECT account_type FROM accounts WHERE id = ?1 AND deleted_at IS NULL",
            [&account],
            |r| r.get(0),
        )
        .optional()?;
    if kind.as_deref() != Some("expense") {
        return Err(AppError::field("accountId", "Choose an expense account."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = input.id.clone().unwrap_or_else(new_id);
    write(ctx.conn, |tx| {
        tx.execute(
            "INSERT INTO expense_categories (id, name, account_id, created_at, updated_at, created_by, updated_by) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?5)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, account_id = excluded.account_id, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            params![id, name, account, now, actor.user_id],
        )?;
        AuditEntry::new("SAVED", "expense_categories", Some(&id))
            .by(actor)
            .write(tx, &now)
    })?;
    Ok(ExpenseCategory {
        id,
        name,
        account_id: account,
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseInput {
    pub idempotency_key: String,
    pub category_id: String,
    pub title: String,
    pub description: Option<String>,
    pub reference_no: Option<String>,
    /// Amount before tax.
    pub amount: i64,
    /// Input tax rate in ppm (0 when there's no tax invoice).
    #[serde(default)]
    pub tax_rate: i64,
    pub business_date: Option<String>,
    /// Supplier, when the expense is on credit or should appear on their statement.
    pub party_id: Option<String>,
    /// Paid now (otherwise owed to the supplier).
    pub payment: Option<PaymentInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseDetail {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub category_id: String,
    pub category_name: String,
    pub title: String,
    pub description: Option<String>,
    pub reference_no: Option<String>,
    pub party_id: Option<String>,
    pub amount: i64,
    pub tax_total: i64,
    pub total: i64,
    pub amount_paid: i64,
    pub status: String,
}

fn detail(conn: &Connection, id: &str) -> AppResult<ExpenseDetail> {
    conn.query_row(
        "SELECT e.id, e.doc_no, e.business_date, e.category_id, c.name, e.title, e.description, e.reference_no, e.party_id, e.amount, e.tax_total,
                e.total, e.amount_paid, e.status
         FROM expenses e JOIN expense_categories c ON c.id = e.category_id WHERE e.id = ?1",
        [id],
        |r| {
            Ok(ExpenseDetail {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                category_id: r.get(3)?,
                category_name: r.get(4)?,
                title: r.get(5)?,
                description: r.get(6)?,
                reference_no: r.get(7)?,
                party_id: r.get(8)?,
                amount: r.get(9)?,
                tax_total: r.get(10)?,
                total: r.get(11)?,
                amount_paid: r.get(12)?,
                status: r.get(13)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The expense"))
}

pub fn create(ctx: &mut Ctx<'_>, input: &ExpenseInput) -> AppResult<ExpenseDetail> {
    ctx.require("expenses.manage")?;
    let title = required("title", &input.title, 120)?;
    let description = optional("description", input.description.as_deref(), 1000)?;
    let reference = optional("referenceNo", input.reference_no.as_deref(), 60)?;
    let amount = positive("amount", input.amount)?;
    let tax_rate = rate("taxRate", input.tax_rate)?;
    let tax = money::apply_rate(amount, tax_rate, RoundingMode::HalfUp);
    let total = amount + tax;
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
            "An expense can't be dated in the future.",
        ));
    }
    let lock = period::check(ctx, &settings, &business_date, "an expense")?;
    let party = input
        .party_id
        .as_deref()
        .map(|p| party_ledger::load_party(ctx.conn, p, "supplier"))
        .transpose()?;
    let payment = input
        .payment
        .as_ref()
        .map(|p| validate_payments(ctx.conn, std::slice::from_ref(p)))
        .transpose()?
        .map(|mut v| v.remove(0));
    let paid = payment.as_ref().map_or(0, |p| p.amount);
    if paid > total {
        return Err(AppError::field(
            "payment.amount",
            "The payment is more than the expense.",
        ));
    }
    if party.is_none() && paid != total {
        return Err(AppError::field(
            "payment",
            "Choose a supplier for expenses that aren't fully paid now.",
        ));
    }
    let (branch_id, _) = super::setup::default_location(ctx.conn)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "expenses_create",
            actor,
            input,
            &now,
            || {
                let account: String = tx
                .query_row("SELECT account_id FROM expense_categories WHERE id = ?1 AND deleted_at IS NULL", [&input.category_id], |r| r.get(0))
                .optional()?
                .ok_or_else(|| AppError::field("categoryId", "Choose an expense category."))?;
                let number =
                    numbering::next(tx, &settings, &branch_id, "expense", &business_date, &now)?;
                let id = new_id();
                let party_id = party.as_ref().map(|p| p.id.as_str());
                tx.execute(
                "INSERT INTO expenses (id, branch_id, series_id, number_period, doc_seq, doc_no, business_date, category_id, party_id, title, description,
                    reference_no, amount, tax_rate, tax_total, total, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17, ?18, ?18)",
                params![id, branch_id, number.series_id, number.period, number.seq, number.doc_no, business_date, input.category_id, party_id, title, description, reference, amount, tax_rate, tax, total, now, actor.user_id],
            )?;
                if let Some(p) = party_id {
                    party_ledger::post(
                        tx,
                        Posting {
                            party_id: p,
                            business_date: &business_date,
                            entry_type: "expense",
                            source_type: "expense",
                            source_id: &id,
                            amount: -total,
                            narration: format!("Expense {}", number.doc_no),
                            user_id: Some(&actor.user_id),
                            now: &now,
                        },
                    )?;
                }
                journal::post(
                    tx,
                    Entry {
                        business_date: &business_date,
                        source_type: "expense",
                        source_id: &id,
                        narration: format!("{title} ({})", number.doc_no),
                        lines: vec![
                            JLine::debit(&account, amount),
                            JLine::debit(accounts::INPUT_TAX, tax),
                            JLine::credit(accounts::PAYABLE, total).party(party_id),
                        ],
                        user_id: Some(&actor.user_id),
                        now: &now,
                    },
                )?;
                if let Some(payment) = &payment {
                    record_payment(
                        tx,
                        &settings,
                        &branch_id,
                        &business_date,
                        "out",
                        party_id,
                        payment,
                        Some(("expense", &id)),
                        accounts::PAYABLE,
                        None,
                        actor,
                        &now,
                    )?;
                }
                period::audit(&lock, tx, actor, "expenses", &id, &now)?;
                AuditEntry::new("EXPENSE", "expenses", Some(&id))
                    .by(actor)
                    .summary(format!("{} {title} {total}", number.doc_no))
                    .write(tx, &now)?;
                detail(tx, &id)
            },
        )
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidExpenseInput {
    pub id: String,
    pub reason: String,
}

pub fn void(ctx: &mut Ctx<'_>, input: &VoidExpenseInput) -> AppResult<ExpenseDetail> {
    ctx.require("expenses.manage")?;
    let reason = required("reason", &input.reason, 300)?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let dated = detail(ctx.conn, &input.id)?.business_date;
    let lock = period::check_void(ctx, &settings, &dated, &today, "voiding an expense")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let expense = detail(tx, &input.id)?;
        if expense.status != "finalized" {
            return Err(AppError::conflict("This expense is already voided."));
        }
        let payments: Vec<String> = tx
            .prepare("SELECT DISTINCT a.payment_id FROM payment_allocations a JOIN payments p ON p.id = a.payment_id WHERE a.target_type = 'expense' AND a.target_id = ?1 AND p.status = 'finalized'")?
            .query_map([&expense.id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for payment in payments {
            void_payment(
                tx,
                &payment,
                &today,
                &format!("Expense {} voided", expense.doc_no),
                actor,
                &now,
            )?;
        }
        party_ledger::reverse_source(
            tx,
            "expense",
            &expense.id,
            &today,
            "Expense voided",
            Some(&actor.user_id),
            &now,
        )?;
        journal::reverse_source(
            tx,
            "expense",
            &expense.id,
            &today,
            &format!("Void {}", expense.doc_no),
            Some(&actor.user_id),
            &now,
        )?;
        tx.execute(
            "UPDATE expenses SET status = 'voided', voided_at = ?1, voided_by = ?2, void_reason = ?3, updated_at = ?1, updated_by = ?2 WHERE id = ?4",
            params![now, actor.user_id, reason, expense.id],
        )?;
        period::audit(&lock, tx, actor, "expenses", &expense.id, &now)?;
        AuditEntry::new("EXPENSE_VOIDED", "expenses", Some(&expense.id))
            .by(actor)
            .summary(reason.clone())
            .write(tx, &now)?;
        detail(tx, &expense.id)
    })
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseListQuery {
    pub category_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

pub fn list(ctx: &Ctx<'_>, query: &ExpenseListQuery) -> AppResult<Page<ExpenseDetail>> {
    ctx.require("expenses.manage")?;
    let mut filters = vec!["1 = 1".to_string()];
    let mut args: Vec<SqlValue> = Vec::new();
    if let Some(c) = &query.category_id {
        filters.push("e.category_id = ?".into());
        args.push(c.clone().into());
    }
    if let Some(from) = &query.from {
        filters.push("e.business_date >= ?".into());
        args.push(date("from", from)?.into());
    }
    if let Some(to) = &query.to {
        filters.push("e.business_date <= ?".into());
        args.push(date("to", to)?.into());
    }
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        filters.push(
            "(lower(e.title) LIKE lower(?) ESCAPE '\\' OR e.doc_no LIKE ? ESCAPE '\\')".into(),
        );
        let p = like_pattern(search);
        args.extend([p.clone().into(), p.into()]);
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM expenses e JOIN expense_categories c ON c.id = e.category_id WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("docNo", "e.doc_no"),
            ("date", "e.business_date"),
            ("title", "lower(e.title)"),
            ("category", "lower(c.name)"),
            ("total", "e.total"),
            ("paid", "e.amount_paid"),
            ("status", "e.status"),
        ],
        "e.business_date DESC, e.created_at DESC, e.id",
        "e.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let ids: Vec<String> = ctx
        .conn
        .prepare(&format!("SELECT e.id FROM expenses e JOIN expense_categories c ON c.id = e.category_id WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"))?
        .query_map(params_from_iter(args.iter()), |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let items = ids
        .iter()
        .map(|id| detail(ctx.conn, id))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Page::new(items, total, &query.page))
}
