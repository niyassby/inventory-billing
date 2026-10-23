//! Manual journal entries, reviewed: a draft is written and submitted, someone with
//! `journal.approve` approves it, then it is posted (an ordinary journal entry tagged
//! `manual_journal`). A posted journal never changes: it is corrected by a reversal, which is a
//! journal of its own going through the same review.
//!
//! Control accounts (receivables, payables, tax) are refused unless an owner allows them on that
//! journal with a reason; receivable and payable lines then need a party and post to its ledger
//! too, so the control accounts keep matching the parties. Stock accounts are never posted by
//! hand: stock value changes only through stock documents.

use rusqlite::types::Value as Sql;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::accounts;
use super::attachments::{self, AttachmentInfo};
use super::audit::AuditEntry;
use super::common::{date, like_pattern, optional, required, Page, PageQuery};
use super::idempotency;
use super::journal::{self, Entry, JLine};
use super::party_ledger::{self, Posting};
use super::period;
use super::settings::{self, Settings};
use super::{write, Actor, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

/// Subtypes that need an owner's permission on the journal.
const RESTRICTED: [&str; 4] = ["receivable", "payable", "tax_input", "tax_output"];
const MAX_LINES: usize = 100;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineInput {
    pub account_id: String,
    /// Required on receivable and payable lines (and only allowed there).
    pub party_id: Option<String>,
    #[serde(default)]
    pub debit: i64,
    #[serde(default)]
    pub credit: i64,
    pub memo: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalInput {
    /// None creates a draft (with `idempotency_key`); an id edits a draft.
    pub id: Option<String>,
    pub expected_updated_at: Option<String>,
    pub idempotency_key: Option<String>,
    /// Default: today.
    pub business_date: Option<String>,
    pub narration: String,
    pub lines: Vec<JournalLineInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineRow {
    pub line_no: i64,
    pub account_id: String,
    pub account_code: String,
    pub account_name: String,
    pub account_subtype: String,
    pub party_id: Option<String>,
    pub party_name: Option<String>,
    pub debit: i64,
    pub credit: i64,
    pub memo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocRef {
    pub id: String,
    pub doc_no: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalDetail {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub narration: String,
    /// draft, submitted, approved, posted or reversed.
    pub status: String,
    pub lines: Vec<JournalLineRow>,
    pub total_debit: i64,
    pub total_credit: i64,
    /// Lines on control accounts (they need `allow_restricted`).
    pub uses_restricted: bool,
    pub allow_restricted: bool,
    pub restricted_reason: Option<String>,
    pub restricted_by_name: Option<String>,
    pub reversal_of: Option<DocRef>,
    pub reversed_by: Option<DocRef>,
    pub review_note: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
    pub created_by_name: Option<String>,
    pub submitted_at: Option<String>,
    pub submitted_by_name: Option<String>,
    pub approved_at: Option<String>,
    pub approved_by_name: Option<String>,
    pub posted_at: Option<String>,
    pub posted_by_name: Option<String>,
    pub attachments: Vec<AttachmentInfo>,
    pub updated_at: String,
}

fn doc_ref(conn: &Connection, id: Option<String>) -> AppResult<Option<DocRef>> {
    let Some(id) = id else { return Ok(None) };
    Ok(conn
        .query_row(
            "SELECT doc_no FROM manual_journals WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )
        .optional()?
        .map(|doc_no| DocRef { id, doc_no }))
}

pub fn detail(conn: &Connection, id: &str) -> AppResult<JournalDetail> {
    type Head = (
        String,
        String,
        String,
        String,
        bool,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let head: Option<(Head, Vec<Option<String>>)> = conn
        .query_row(
            "SELECT j.doc_no, j.business_date, j.narration, j.status, j.allow_restricted, j.restricted_reason, ru.name,
                j.reversal_of, j.reversed_by_journal, j.review_note,
                j.created_at, j.created_by, cu.name, j.submitted_at, su.name, j.approved_at, au.name, j.posted_at, pu.name, j.updated_at
             FROM manual_journals j
             LEFT JOIN users ru ON ru.id = j.restricted_by LEFT JOIN users cu ON cu.id = j.created_by
             LEFT JOIN users su ON su.id = j.submitted_by LEFT JOIN users au ON au.id = j.approved_by
             LEFT JOIN users pu ON pu.id = j.posted_by
             WHERE j.id = ?1 AND j.deleted_at IS NULL",
            [id],
            |r| {
                Ok((
                    (
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                    ),
                    (10..20).map(|i| r.get(i)).collect::<Result<Vec<_>, _>>()?,
                ))
            },
        )
        .optional()?;
    let Some((h, rest)) = head else {
        return Err(AppError::not_found("The journal"));
    };
    let lines: Vec<JournalLineRow> = conn
        .prepare(
            "SELECT l.line_no, l.account_id, a.code, a.name, a.subtype, l.party_id, p.name, l.debit, l.credit, l.memo
             FROM manual_journal_lines l JOIN accounts a ON a.id = l.account_id LEFT JOIN parties p ON p.id = l.party_id
             WHERE l.journal_id = ?1 ORDER BY l.line_no",
        )?
        .query_map([id], |r| {
            Ok(JournalLineRow {
                line_no: r.get(0)?,
                account_id: r.get(1)?,
                account_code: r.get(2)?,
                account_name: r.get(3)?,
                account_subtype: r.get(4)?,
                party_id: r.get(5)?,
                party_name: r.get(6)?,
                debit: r.get(7)?,
                credit: r.get(8)?,
                memo: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let at = |i: usize| rest.get(i).cloned().flatten();
    Ok(JournalDetail {
        id: id.to_string(),
        doc_no: h.0,
        business_date: h.1,
        narration: h.2,
        status: h.3,
        total_debit: lines.iter().map(|l| l.debit).sum(),
        total_credit: lines.iter().map(|l| l.credit).sum(),
        uses_restricted: lines
            .iter()
            .any(|l| RESTRICTED.contains(&l.account_subtype.as_str())),
        lines,
        allow_restricted: h.4,
        restricted_reason: h.5,
        restricted_by_name: h.6,
        reversal_of: doc_ref(conn, h.7)?,
        reversed_by: doc_ref(conn, h.8)?,
        review_note: h.9,
        created_at: at(0).unwrap_or_default(),
        created_by: at(1),
        created_by_name: at(2),
        submitted_at: at(3),
        submitted_by_name: at(4),
        approved_at: at(5),
        approved_by_name: at(6),
        posted_at: at(7),
        posted_by_name: at(8),
        updated_at: at(9).unwrap_or_default(),
        attachments: attachments::list_of(conn, "manual_journal", id)?,
    })
}

fn can_read(ctx: &Ctx<'_>) -> AppResult<()> {
    ctx.require("journal.manage")
        .or_else(|_| ctx.require("journal.approve"))
        .or_else(|_| ctx.require("accounting.view"))
}

pub fn get(ctx: &Ctx<'_>, id: &str) -> AppResult<JournalDetail> {
    can_read(ctx)?;
    detail(ctx.conn, id)
}

// ------------------------------------------------------------------ list

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalListQuery {
    pub status: Option<String>,
    pub search: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalListItem {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub narration: String,
    pub status: String,
    pub total: i64,
    pub created_by_name: Option<String>,
    pub attachment_count: i64,
}

pub fn list(ctx: &Ctx<'_>, query: &JournalListQuery) -> AppResult<Page<JournalListItem>> {
    can_read(ctx)?;
    let mut filters = vec!["j.deleted_at IS NULL".to_string()];
    let mut args: Vec<Sql> = Vec::new();
    if let Some(s) = query.status.as_deref().filter(|s| !s.is_empty()) {
        if !matches!(
            s,
            "draft" | "submitted" | "approved" | "posted" | "reversed"
        ) {
            return Err(AppError::field("status", "Unknown status."));
        }
        filters.push("j.status = ?".into());
        args.push(s.to_string().into());
    }
    if let Some(from) = &query.from {
        filters.push("j.business_date >= ?".into());
        args.push(date("from", from)?.into());
    }
    if let Some(to) = &query.to {
        filters.push("j.business_date <= ?".into());
        args.push(date("to", to)?.into());
    }
    if let Some(s) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        filters.push(
            "(lower(j.doc_no) LIKE lower(?) ESCAPE '\\' OR lower(j.narration) LIKE lower(?) ESCAPE '\\')"
                .into(),
        );
        args.push(like_pattern(s).into());
        args.push(like_pattern(s).into());
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM manual_journals j WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("date", "j.business_date"),
            ("docNo", "j.doc_seq"),
            ("status", "j.status"),
            ("total", "total"),
        ],
        "j.business_date DESC, j.doc_seq DESC",
        "j.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT j.id, j.doc_no, j.business_date, j.narration, j.status,
                coalesce((SELECT sum(debit) FROM manual_journal_lines WHERE journal_id = j.id), 0) AS total, u.name,
                (SELECT count(*) FROM attachments a WHERE a.entity_type = 'manual_journal' AND a.entity_id = j.id AND a.deleted_at IS NULL)
             FROM manual_journals j LEFT JOIN users u ON u.id = j.created_by
             WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(JournalListItem {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                narration: r.get(3)?,
                status: r.get(4)?,
                total: r.get(5)?,
                created_by_name: r.get(6)?,
                attachment_count: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

// ------------------------------------------------------------------ drafts

struct CheckedLine {
    account_id: String,
    party_id: Option<String>,
    debit: i64,
    credit: i64,
    memo: Option<String>,
}

fn check_lines(conn: &Connection, lines: &[JournalLineInput]) -> AppResult<Vec<CheckedLine>> {
    if lines.len() < 2 {
        return Err(AppError::field(
            "lines",
            "A journal needs at least two lines.",
        ));
    }
    if lines.len() > MAX_LINES {
        return Err(AppError::field(
            "lines",
            format!("A journal can have at most {MAX_LINES} lines."),
        ));
    }
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let field = |name: &str| format!("lines.{i}.{name}");
        if line.debit < 0 || line.credit < 0 || (line.debit > 0) == (line.credit > 0) {
            return Err(AppError::field(
                &field("debit"),
                "Enter a debit or a credit on each line, not both.",
            ));
        }
        let account: Option<(String, bool)> = conn
            .query_row(
                "SELECT subtype, is_active FROM accounts WHERE id = ?1 AND deleted_at IS NULL",
                [&line.account_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((subtype, active)) = account else {
            return Err(AppError::field(&field("accountId"), "Choose an account."));
        };
        if !active {
            return Err(AppError::field(
                &field("accountId"),
                "That account is not in use.",
            ));
        }
        if subtype == "inventory" || line.account_id == accounts::GOODS_RECEIVED_NOT_INVOICED {
            return Err(AppError::field(
                &field("accountId"),
                "Stock accounts change only through stock documents (adjustments, counts, purchases).",
            ));
        }
        let needs_party = matches!(subtype.as_str(), "receivable" | "payable");
        let party_id = line.party_id.clone().filter(|p| !p.is_empty());
        match (&party_id, needs_party) {
            (None, true) => {
                return Err(AppError::field(
                    &field("partyId"),
                    "Choose the customer or supplier for this line.",
                ))
            }
            (Some(_), false) => {
                return Err(AppError::field(
                    &field("partyId"),
                    "Only receivable and payable lines have a customer or supplier.",
                ))
            }
            (Some(p), true) => {
                let ok: bool = conn.query_row(
                    "SELECT EXISTS (SELECT 1 FROM parties WHERE id = ?1 AND deleted_at IS NULL)",
                    [p],
                    |r| r.get(0),
                )?;
                if !ok {
                    return Err(AppError::field(
                        &field("partyId"),
                        "That customer or supplier doesn't exist.",
                    ));
                }
            }
            (None, false) => {}
        }
        out.push(CheckedLine {
            account_id: line.account_id.clone(),
            party_id,
            debit: line.debit,
            credit: line.credit,
            memo: optional(&field("memo"), line.memo.as_deref(), 200)?,
        });
    }
    Ok(out)
}

fn replace_lines(
    conn: &Connection,
    journal_id: &str,
    lines: &[CheckedLine],
    now: &str,
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM manual_journal_lines WHERE journal_id = ?1",
        [journal_id],
    )?;
    for (i, l) in lines.iter().enumerate() {
        conn.execute(
            "INSERT INTO manual_journal_lines (id, journal_id, line_no, account_id, party_id, debit, credit, memo, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![new_id(), journal_id, i + 1, l.account_id, l.party_id, l.debit, l.credit, l.memo, now],
        )?;
    }
    Ok(())
}

fn journal_date(settings: &Settings, ctx: &Ctx<'_>, input: Option<&str>) -> AppResult<String> {
    let today = settings.business_date(ctx.now);
    let day = match input {
        Some(d) => date("businessDate", d)?,
        None => today.clone(),
    };
    if day > today {
        return Err(AppError::field(
            "businessDate",
            "A journal can't be dated in the future.",
        ));
    }
    Ok(day)
}

/// Next number of the year: JV/25-26/00001.
fn next_number(
    conn: &Connection,
    settings: &Settings,
    day: &str,
) -> AppResult<(String, i64, String)> {
    let period = settings.fy_label(day);
    let seq: i64 = conn.query_row(
        "SELECT coalesce(max(doc_seq), 0) + 1 FROM manual_journals WHERE number_period = ?1",
        [&period],
        |r| r.get(0),
    )?;
    Ok((period.clone(), seq, format!("JV/{period}/{seq:05}")))
}

fn check_current(current: &JournalDetail, expected: Option<&String>) -> AppResult<()> {
    if expected.is_some_and(|e| *e != current.updated_at) {
        return Err(AppError::conflict(
            "Someone else changed this journal. Reload it and try again.",
        ));
    }
    Ok(())
}

/// Creates or edits a draft.
pub fn save(ctx: &mut Ctx<'_>, input: &JournalInput) -> AppResult<JournalDetail> {
    ctx.require("journal.manage")?;
    let settings = settings::load(ctx.conn)?;
    let narration = required("narration", &input.narration, 300)?;
    let day = journal_date(&settings, ctx, input.business_date.as_deref())?;
    let lines = check_lines(ctx.conn, &input.lines)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| match &input.id {
        None => {
            let key = input
                .idempotency_key
                .as_deref()
                .ok_or_else(|| AppError::field("idempotencyKey", "A request id is required."))?;
            idempotency::once(tx, key, "journals_save", actor, input, &now, || {
                let id = new_id();
                let (period, seq, doc_no) = next_number(tx, &settings, &day)?;
                tx.execute(
                    "INSERT INTO manual_journals (id, doc_no, number_period, doc_seq, business_date, narration, status, idempotency_key,
                        created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'draft', ?7, ?8, ?8, ?9, ?9)",
                    params![id, doc_no, period, seq, day, narration, key, now, actor.user_id],
                )?;
                replace_lines(tx, &id, &lines, &now)?;
                AuditEntry::new("JOURNAL_DRAFTED", "manual_journals", Some(&id))
                    .by(actor)
                    .summary(format!("{doc_no}: {narration}"))
                    .write(tx, &now)?;
                Ok(id)
            })
        }
        Some(id) => {
            let current = detail(tx, id)?;
            check_current(&current, input.expected_updated_at.as_ref())?;
            if current.status != "draft" {
                return Err(AppError::conflict(
                    "Only a draft can be edited. Send it back to draft first.",
                ));
            }
            if current.reversal_of.is_some() && !same_lines(&current.lines, &lines) {
                return Err(AppError::field(
                    "lines",
                    "A reversal undoes its journal exactly: only its date and narration can change.",
                ));
            }
            tx.execute(
                "UPDATE manual_journals SET business_date = ?1, narration = ?2, updated_at = ?3, updated_by = ?4 WHERE id = ?5",
                params![day, narration, now, actor.user_id, id],
            )?;
            replace_lines(tx, id, &lines, &now)?;
            AuditEntry::new("JOURNAL_EDITED", "manual_journals", Some(id))
                .by(actor)
                .summary(format!("{}: {narration}", current.doc_no))
                .write(tx, &now)?;
            Ok(id.clone())
        }
    })?;
    detail(ctx.conn, &id)
}

fn same_lines(rows: &[JournalLineRow], lines: &[CheckedLine]) -> bool {
    rows.len() == lines.len()
        && rows.iter().zip(lines).all(|(a, b)| {
            a.account_id == b.account_id
                && a.party_id == b.party_id
                && a.debit == b.debit
                && a.credit == b.credit
        })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalActionInput {
    pub id: String,
    pub expected_updated_at: Option<String>,
    /// Sending back: why. Allowing control accounts: why.
    pub note: Option<String>,
}

/// Throws away a draft (soft delete; its number is not reused).
pub fn delete(ctx: &mut Ctx<'_>, input: &JournalActionInput) -> AppResult<()> {
    ctx.require("journal.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let current = detail(tx, &input.id)?;
        check_current(&current, input.expected_updated_at.as_ref())?;
        if current.status != "draft" {
            return Err(AppError::conflict("Only a draft can be deleted."));
        }
        tx.execute(
            "UPDATE manual_journals SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("JOURNAL_DELETED", "manual_journals", Some(&input.id))
            .by(actor)
            .summary(current.doc_no)
            .write(tx, &now)
    })
}

/// Why a journal can't go forward (balance, control accounts); None when it can.
fn blocker(j: &JournalDetail) -> Option<String> {
    if j.total_debit != j.total_credit {
        return Some(format!(
            "Debits ({}) and credits ({}) must be equal.",
            j.total_debit, j.total_credit
        ));
    }
    if j.total_debit == 0 {
        return Some("The journal has no amounts.".into());
    }
    if j.uses_restricted && !j.allow_restricted {
        return Some(
            "This journal posts to a control account (receivables, payables or tax). An owner must allow it first."
                .into(),
        );
    }
    None
}

fn transition(
    ctx: &mut Ctx<'_>,
    input: &JournalActionInput,
    action: &str,
    f: impl FnOnce(&rusqlite::Transaction<'_>, &JournalDetail, &Actor, &str) -> AppResult<()>,
) -> AppResult<JournalDetail> {
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let current = detail(tx, &input.id)?;
        check_current(&current, input.expected_updated_at.as_ref())?;
        f(tx, &current, actor, &now)?;
        AuditEntry::new(action, "manual_journals", Some(&input.id))
            .by(actor)
            .summary(match input.note.as_deref() {
                Some(n) if !n.trim().is_empty() => format!("{}: {}", current.doc_no, n.trim()),
                _ => current.doc_no.clone(),
            })
            .write(tx, &now)
    })?;
    detail(ctx.conn, &input.id)
}

/// Draft → submitted: balanced, and control accounts allowed if used.
pub fn submit(ctx: &mut Ctx<'_>, input: &JournalActionInput) -> AppResult<JournalDetail> {
    ctx.require("journal.manage")?;
    transition(ctx, input, "JOURNAL_SUBMITTED", |tx, j, actor, now| {
        if j.status != "draft" {
            return Err(AppError::conflict("Only a draft can be submitted."));
        }
        if let Some(b) = blocker(j) {
            return Err(AppError::validation(b));
        }
        tx.execute(
            "UPDATE manual_journals SET status = 'submitted', submitted_at = ?1, submitted_by = ?2, review_note = NULL, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, j.id],
        )?;
        Ok(())
    })
}

/// Submitted or approved → draft, with the reviewer's note.
pub fn send_back(ctx: &mut Ctx<'_>, input: &JournalActionInput) -> AppResult<JournalDetail> {
    ctx.require("journal.approve")?;
    let note = required("note", input.note.as_deref().unwrap_or_default(), 300)?;
    transition(ctx, input, "JOURNAL_SENT_BACK", |tx, j, actor, now| {
        if !matches!(j.status.as_str(), "submitted" | "approved") {
            return Err(AppError::conflict(
                "Only a submitted or approved journal can be sent back.",
            ));
        }
        tx.execute(
            "UPDATE manual_journals SET status = 'draft', review_note = ?1, approved_at = NULL, approved_by = NULL, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
            params![note, now, actor.user_id, j.id],
        )?;
        Ok(())
    })
}

/// Submitted → approved. The approver is someone other than the writer, except an owner.
pub fn approve(ctx: &mut Ctx<'_>, input: &JournalActionInput) -> AppResult<JournalDetail> {
    ctx.require("journal.approve")?;
    transition(ctx, input, "JOURNAL_APPROVED", |tx, j, actor, now| {
        if j.status != "submitted" {
            return Err(AppError::conflict(
                "Only a submitted journal can be approved.",
            ));
        }
        if j.created_by.as_deref() == Some(actor.user_id.as_str()) && actor.role_code != "owner" {
            return Err(AppError::conflict(
                "Someone else must approve a journal you wrote.",
            ));
        }
        if let Some(b) = blocker(j) {
            return Err(AppError::validation(b));
        }
        tx.execute(
            "UPDATE manual_journals SET status = 'approved', approved_at = ?1, approved_by = ?2, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, j.id],
        )?;
        Ok(())
    })
}

/// An owner allows (or stops allowing) control accounts on this journal, with a reason.
pub fn allow_restricted(ctx: &mut Ctx<'_>, input: &JournalActionInput) -> AppResult<JournalDetail> {
    ctx.require("journal.restricted")?;
    let reason = required("note", input.note.as_deref().unwrap_or_default(), 300)?;
    transition(
        ctx,
        input,
        "JOURNAL_CONTROL_ACCOUNTS_ALLOWED",
        |tx, j, actor, now| {
            if !matches!(j.status.as_str(), "draft" | "submitted") {
                return Err(AppError::conflict(
                    "Control accounts can only be allowed before approval.",
                ));
            }
            tx.execute(
            "UPDATE manual_journals SET allow_restricted = 1, restricted_reason = ?1, restricted_by = ?2, updated_at = ?3, updated_by = ?2 WHERE id = ?4",
            params![reason, actor.user_id, now, j.id],
        )?;
            Ok(())
        },
    )
}

/// Approved → posted: the journal entry (and the parties' ledger for receivable and payable
/// lines) is written. A locked day needs `period.override`.
pub fn post(ctx: &mut Ctx<'_>, input: &JournalActionInput) -> AppResult<JournalDetail> {
    ctx.require("journal.manage")?;
    let settings = settings::load(ctx.conn)?;
    let current = detail(ctx.conn, &input.id)?;
    let today = settings.business_date(ctx.now);
    if current.business_date > today {
        return Err(AppError::conflict(
            "This journal is dated in the future; post it on or after its date.",
        ));
    }
    let lock = period::check(ctx, &settings, &current.business_date, "a journal")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let j = detail(tx, &input.id)?;
        check_current(&j, input.expected_updated_at.as_ref())?;
        if j.status != "approved" {
            return Err(AppError::conflict(
                "Only an approved journal can be posted.",
            ));
        }
        if let Some(b) = blocker(&j) {
            return Err(AppError::validation(b));
        }
        let lines: Vec<JLine> = j
            .lines
            .iter()
            .map(|l| JLine {
                account_id: l.account_id.clone(),
                party_id: l.party_id.clone(),
                amount: l.debit - l.credit,
                memo: l.memo.clone(),
            })
            .collect();
        let entry = Entry {
            business_date: &j.business_date,
            source_type: "manual_journal",
            source_id: &j.id,
            narration: format!("Manual journal {}: {}", j.doc_no, j.narration),
            lines,
            user_id: Some(&actor.user_id),
            now: &now,
        };
        let original = match &j.reversal_of {
            Some(r) => Some(detail(tx, &r.id)?),
            None => None,
        };
        let entry_id = match &original {
            Some(o) => {
                if o.status != "posted" {
                    return Err(AppError::conflict(format!(
                        "{} is no longer posted, so it can't be reversed.",
                        o.doc_no
                    )));
                }
                let original_entry: String = tx.query_row(
                    "SELECT journal_entry_id FROM manual_journals WHERE id = ?1",
                    [&o.id],
                    |r| r.get(0),
                )?;
                journal::post_reversal(tx, entry, &original_entry)?
            }
            None => journal::post(tx, entry)?
                .ok_or_else(|| AppError::validation("The journal has no amounts."))?,
        };
        for l in j.lines.iter().filter(|l| l.party_id.is_some()) {
            party_ledger::post(
                tx,
                Posting {
                    party_id: l.party_id.as_deref().unwrap_or_default(),
                    business_date: &j.business_date,
                    entry_type: "adjustment",
                    source_type: "manual_journal",
                    source_id: &j.id,
                    amount: l.debit - l.credit,
                    narration: format!("Journal {}: {}", j.doc_no, j.narration),
                    user_id: Some(&actor.user_id),
                    now: &now,
                },
            )?;
        }
        tx.execute(
            "UPDATE manual_journals SET status = 'posted', journal_entry_id = ?1, posted_at = ?2, posted_by = ?3, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
            params![entry_id, now, actor.user_id, j.id],
        )?;
        if let Some(o) = &original {
            tx.execute(
                "UPDATE manual_journals SET status = 'reversed', reversed_by_journal = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
                params![j.id, now, actor.user_id, o.id],
            )?;
        }
        period::audit(&lock, tx, actor, "manual_journals", &j.id, &now)?;
        AuditEntry::new("JOURNAL_POSTED", "manual_journals", Some(&j.id))
            .by(actor)
            .approved_by(None)
            .summary(format!(
                "{} {} (manual){}",
                j.doc_no,
                j.total_debit,
                original
                    .as_ref()
                    .map(|o| format!(", reverses {}", o.doc_no))
                    .unwrap_or_default()
            ))
            .write(tx, &now)
    })?;
    detail(ctx.conn, &input.id)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReverseJournalInput {
    pub id: String,
    pub idempotency_key: String,
    /// Default: today.
    pub business_date: Option<String>,
    pub reason: String,
}

/// Starts the reversal of a posted journal: a draft with every line swapped, which then goes
/// through submit, approve and post like any journal.
pub fn reverse(ctx: &mut Ctx<'_>, input: &ReverseJournalInput) -> AppResult<JournalDetail> {
    ctx.require("journal.manage")?;
    let settings = settings::load(ctx.conn)?;
    let reason = required("reason", &input.reason, 300)?;
    let day = journal_date(&settings, ctx, input.business_date.as_deref())?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "journals_reverse",
            actor,
            input,
            &now,
            || {
                let original = detail(tx, &input.id)?;
                if original.status != "posted" {
                    return Err(AppError::conflict("Only a posted journal can be reversed."));
                }
                if original.reversal_of.is_some() {
                    return Err(AppError::conflict(
                        "This journal is itself a reversal; post a new journal instead.",
                    ));
                }
                let pending: Option<String> = tx
                    .query_row(
                        "SELECT doc_no FROM manual_journals WHERE reversal_of = ?1 AND deleted_at IS NULL AND status <> 'posted'",
                        [&original.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(doc) = pending {
                    return Err(AppError::conflict(format!(
                        "{doc} already reverses this journal."
                    )));
                }
                let id = new_id();
                let (period, seq, doc_no) = next_number(tx, &settings, &day)?;
                // The owner's permission for control accounts carries over to the reversal.
                tx.execute(
                    "INSERT INTO manual_journals (id, doc_no, number_period, doc_seq, business_date, narration, status, reversal_of,
                        allow_restricted, restricted_reason, restricted_by, idempotency_key, created_at, updated_at, created_by, updated_by)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, 'draft', o.id, o.allow_restricted, o.restricted_reason, o.restricted_by, ?7, ?8, ?8, ?9, ?9
                     FROM manual_journals o WHERE o.id = ?10",
                    params![
                        id, doc_no, period, seq, day, format!("Reversal of {}: {reason}", original.doc_no),
                        input.idempotency_key, now, actor.user_id, original.id
                    ],
                )?;
                let swapped: Vec<CheckedLine> = original
                    .lines
                    .iter()
                    .map(|l| CheckedLine {
                        account_id: l.account_id.clone(),
                        party_id: l.party_id.clone(),
                        debit: l.credit,
                        credit: l.debit,
                        memo: l.memo.clone(),
                    })
                    .collect();
                replace_lines(tx, &id, &swapped, &now)?;
                AuditEntry::new("JOURNAL_REVERSAL_DRAFTED", "manual_journals", Some(&id))
                    .by(actor)
                    .summary(format!("{doc_no} reverses {}: {reason}", original.doc_no))
                    .write(tx, &now)?;
                Ok(id)
            },
        )
    })?;
    detail(ctx.conn, &id)
}
