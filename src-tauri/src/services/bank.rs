//! Bank reconciliation. A statement (CSV, columns mapped once per account) is imported as lines;
//! each line is matched to the book lines (journal lines on that bank account) it stands for,
//! automatically by amount, a date window and the reference, or by hand. Matched book lines are
//! cleared on the statement line's date; a book line can also be ticked as cleared by hand.
//!
//! The reconciliation statement for a day: the book balance, less money in not yet on the
//! statement, plus money out not yet on it, plus statement lines not in the books, should equal
//! the statement balance. Finishing a reconciliation keeps those figures as history.
//!
//! Statement lines: positive = money into the account. Book lines: debit (money in) positive.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::audit::AuditEntry;
use super::common::{date, optional};
use super::settings;
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::{csv, statement};
use crate::error::{AppError, AppResult};

/// The largest statement file accepted.
pub const MAX_CSV_BYTES: usize = 3 * 1024 * 1024;
const MAX_LINES: usize = 20_000;
const PREVIEW_ROWS: usize = 200;

fn bank_account(conn: &Connection, id: &str) -> AppResult<String> {
    conn.query_row(
        "SELECT name FROM accounts WHERE id = ?1 AND subtype = 'bank' AND deleted_at IS NULL",
        [id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::field("accountId", "Choose a bank account."))
}

// ------------------------------------------------------------------ import

/// Which column holds what (0-based), and how dates are written.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementMapping {
    /// Lines before the header (bank name, account number…).
    #[serde(default)]
    pub skip_rows: usize,
    #[serde(default = "yes")]
    pub has_header: bool,
    pub date_column: usize,
    /// One of `domain::statement::DATE_FORMATS`.
    pub date_format: String,
    pub description_column: usize,
    pub reference_column: Option<usize>,
    /// One signed column (positive = money in)…
    pub amount_column: Option<usize>,
    /// …or separate withdrawal and deposit columns.
    pub withdrawal_column: Option<usize>,
    pub deposit_column: Option<usize>,
    pub balance_column: Option<usize>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportInput {
    pub account_id: String,
    pub file_name: String,
    /// The file's text.
    pub csv: String,
    /// None: the mapping saved for this account (if any); without one only the sample comes back.
    pub mapping: Option<StatementMapping>,
    /// Remember this mapping for the account.
    #[serde(default)]
    pub save_mapping: bool,
    /// false: check and show what would be imported; true: import.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementPreviewRow {
    /// Line number in the file (1-based).
    pub row_no: usize,
    pub txn_date: Option<String>,
    pub description: String,
    pub reference: Option<String>,
    pub amount: Option<i64>,
    pub balance: Option<i64>,
    /// Already imported for this account.
    pub duplicate: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportResult {
    /// The first rows of the file as read (for choosing columns).
    pub sample: Vec<Vec<String>>,
    /// Column names (the header, or "Column 1"…).
    pub columns: Vec<String>,
    pub mapping: Option<StatementMapping>,
    pub date_formats: Vec<String>,
    /// The first rows as they would be imported.
    pub rows: Vec<StatementPreviewRow>,
    pub total_rows: usize,
    pub new_rows: usize,
    pub duplicates: usize,
    pub errors: usize,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub imported: bool,
    pub import_id: Option<String>,
}

struct Parsed {
    row_no: usize,
    date: String,
    description: String,
    reference: Option<String>,
    amount: i64,
    balance: Option<i64>,
}

fn cell(row: &[String], column: Option<usize>) -> &str {
    column.and_then(|c| row.get(c)).map_or("", |s| s.trim())
}

fn parse_row(
    row: &[String],
    row_no: usize,
    m: &StatementMapping,
    digits: u32,
) -> Result<Parsed, String> {
    let date =
        statement::parse_date(cell(row, Some(m.date_column)), &m.date_format).ok_or_else(|| {
            format!(
                "The date '{}' isn't {}.",
                cell(row, Some(m.date_column)),
                m.date_format
            )
        })?;
    let amount_of = |c: Option<usize>, what: &str| {
        statement::parse_amount(cell(row, c), digits)
            .map_err(|_| format!("The {what} '{}' isn't an amount.", cell(row, c)))
    };
    let amount = match m.amount_column {
        Some(c) => amount_of(Some(c), "amount")?,
        None => {
            let out = amount_of(m.withdrawal_column, "withdrawal")?.unwrap_or(0);
            let inn = amount_of(m.deposit_column, "deposit")?.unwrap_or(0);
            Some(inn - out.abs())
        }
    }
    .filter(|a| *a != 0)
    .ok_or_else(|| "The line has no amount.".to_string())?;
    let description: String = cell(row, Some(m.description_column))
        .chars()
        .take(300)
        .collect();
    let reference = Some(cell(row, m.reference_column))
        .filter(|r| !r.is_empty())
        .map(|r| r.chars().take(80).collect());
    let balance = if m.balance_column.is_some() {
        amount_of(m.balance_column, "balance")?
    } else {
        None
    };
    Ok(Parsed {
        row_no,
        date,
        description,
        reference,
        amount,
        balance,
    })
}

fn line_hash(account: &str, p: &Parsed, occurrence: usize) -> String {
    hex::encode(Sha256::digest(
        format!(
            "{account}|{}|{}|{}|{}|{}|{occurrence}",
            p.date,
            p.amount,
            p.description,
            p.reference.as_deref().unwrap_or_default(),
            p.balance.map(|b| b.to_string()).unwrap_or_default()
        )
        .as_bytes(),
    ))
}

/// Reads a statement with the mapping, shows what it would import, and imports it on `apply`.
/// Importing the same (or an overlapping) statement again adds only the new lines.
pub fn import(ctx: &mut Ctx<'_>, input: &StatementImportInput) -> AppResult<StatementImportResult> {
    ctx.require("bank.reconcile")?;
    let account_name = bank_account(ctx.conn, &input.account_id)?;
    if input.csv.len() > MAX_CSV_BYTES {
        return Err(AppError::field("csv", "Statements can be at most 3 MB."));
    }
    let settings = settings::load(ctx.conn)?;
    let digits = settings.business.currency_minor_digits;
    let rows = csv::parse(&input.csv, csv::guess_separator(&input.csv));
    if rows.len() > MAX_LINES {
        return Err(AppError::field(
            "csv",
            format!("A statement can have at most {MAX_LINES} lines; import it in parts."),
        ));
    }
    let saved: Option<StatementMapping> = ctx
        .conn
        .query_row(
            "SELECT mapping_json FROM bank_import_profiles WHERE account_id = ?1",
            [&input.account_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|j| serde_json::from_str(&j).ok());
    let mapping = input.mapping.clone().or(saved);
    let sample: Vec<Vec<String>> = rows.iter().take(8).cloned().collect();
    let width = rows.iter().take(50).map(Vec::len).max().unwrap_or(0);
    let skip = mapping.as_ref().map_or(0, |m| m.skip_rows);
    let columns: Vec<String> = match &mapping {
        Some(m) if m.has_header => {
            let header = rows.get(skip).cloned().unwrap_or_default();
            (0..width)
                .map(|i| {
                    header
                        .get(i)
                        .map(|h| h.trim().to_string())
                        .filter(|h| !h.is_empty())
                        .unwrap_or_else(|| format!("Column {}", i + 1))
                })
                .collect()
        }
        _ => (0..width).map(|i| format!("Column {}", i + 1)).collect(),
    };
    let mut result = StatementImportResult {
        sample,
        columns,
        mapping: mapping.clone(),
        date_formats: statement::DATE_FORMATS
            .iter()
            .map(|f| f.to_string())
            .collect(),
        rows: Vec::new(),
        total_rows: 0,
        new_rows: 0,
        duplicates: 0,
        errors: 0,
        from_date: None,
        to_date: None,
        imported: false,
        import_id: None,
    };
    let Some(m) = mapping else {
        return Ok(result);
    };
    if m.amount_column.is_none() && m.withdrawal_column.is_none() && m.deposit_column.is_none() {
        return Err(AppError::field(
            "mapping.amountColumn",
            "Choose the amount column, or the withdrawal and deposit columns.",
        ));
    }
    if !statement::DATE_FORMATS.contains(&m.date_format.as_str()) {
        return Err(AppError::field(
            "mapping.dateFormat",
            "Choose a date layout.",
        ));
    }
    let first_data = skip + usize::from(m.has_header);
    let mut parsed: Vec<Result<Parsed, (usize, String)>> = Vec::new();
    for (i, row) in rows.iter().enumerate().skip(first_data) {
        parsed.push(parse_row(row, i + 1, &m, digits).map_err(|e| (i + 1, e)));
    }
    // Identical lines (two ₹10 charges on one day) stay apart by their position among equals.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut new_lines: Vec<(Parsed, String)> = Vec::new();
    for p in parsed {
        result.total_rows += 1;
        match p {
            Err((row_no, error)) => {
                result.errors += 1;
                if result.rows.len() < PREVIEW_ROWS {
                    result.rows.push(StatementPreviewRow {
                        row_no,
                        txn_date: None,
                        description: rows
                            .get(row_no - 1)
                            .map(|r| r.join(" · "))
                            .unwrap_or_default(),
                        reference: None,
                        amount: None,
                        balance: None,
                        duplicate: false,
                        error: Some(error),
                    });
                }
            }
            Ok(p) => {
                let base = line_hash(&input.account_id, &p, 0);
                let n = seen.entry(base).or_insert(0);
                let hash = line_hash(&input.account_id, &p, *n);
                *n += 1;
                let duplicate: bool = ctx.conn.query_row(
                    "SELECT EXISTS (SELECT 1 FROM bank_statement_lines WHERE account_id = ?1 AND line_hash = ?2)",
                    params![input.account_id, hash],
                    |r| r.get(0),
                )?;
                if duplicate {
                    result.duplicates += 1;
                }
                if result.rows.len() < PREVIEW_ROWS {
                    result.rows.push(StatementPreviewRow {
                        row_no: p.row_no,
                        txn_date: Some(p.date.clone()),
                        description: p.description.clone(),
                        reference: p.reference.clone(),
                        amount: Some(p.amount),
                        balance: p.balance,
                        duplicate,
                        error: None,
                    });
                }
                result.from_date = Some(match result.from_date.take() {
                    Some(d) if d <= p.date => d,
                    _ => p.date.clone(),
                });
                result.to_date = Some(match result.to_date.take() {
                    Some(d) if d >= p.date => d,
                    _ => p.date.clone(),
                });
                if !duplicate {
                    new_lines.push((p, hash));
                }
            }
        }
    }
    result.new_rows = new_lines.len();
    if !input.apply {
        return Ok(result);
    }
    if result.errors > 0 {
        return Err(AppError::validation(format!(
            "{} line(s) couldn't be read. Fix the columns or the file first.",
            result.errors
        )));
    }
    let file_name = super::common::required("fileName", &input.file_name, 200)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let import_id = new_id();
    write(ctx.conn, |tx| {
        tx.execute(
            "INSERT INTO bank_statement_imports (id, account_id, file_name, from_date, to_date, line_count, skipped, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![import_id, input.account_id, file_name, result.from_date, result.to_date, new_lines.len(), result.duplicates, now, actor.user_id],
        )?;
        for (p, hash) in &new_lines {
            tx.execute(
                "INSERT INTO bank_statement_lines (id, import_id, account_id, txn_date, description, reference, amount, balance, line_hash, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
                 ON CONFLICT (account_id, line_hash) DO NOTHING",
                params![new_id(), import_id, input.account_id, p.date, p.description, p.reference, p.amount, p.balance, hash, now],
            )?;
        }
        if input.save_mapping {
            tx.execute(
                "INSERT INTO bank_import_profiles (account_id, mapping_json, updated_at, updated_by) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (account_id) DO UPDATE SET mapping_json = excluded.mapping_json, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
                params![input.account_id, serde_json::to_string(&m)?, now, actor.user_id],
            )?;
        }
        AuditEntry::new(
            "BANK_STATEMENT_IMPORTED",
            "bank_statement_imports",
            Some(&import_id),
        )
        .by(actor)
        .summary(format!(
            "{account_name}: {file_name}, {} new line(s), {} already there",
            new_lines.len(),
            result.duplicates
        ))
        .write(tx, &now)
    })?;
    result.imported = true;
    result.import_id = Some(import_id);
    Ok(result)
}

// ------------------------------------------------------------------ the working view

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatementLineRow {
    pub id: String,
    pub txn_date: String,
    pub description: String,
    pub reference: Option<String>,
    pub amount: i64,
    pub balance: Option<i64>,
    /// unmatched, matched or ignored.
    pub status: String,
    pub ignore_note: Option<String>,
    /// The book lines it is matched to.
    pub journal_line_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BookLineRow {
    pub journal_line_id: String,
    pub business_date: String,
    pub narration: String,
    pub source_type: String,
    pub source_id: Option<String>,
    /// The payment's number and reference, when the line is a payment.
    pub doc_no: Option<String>,
    pub reference: Option<String>,
    /// Positive = money in.
    pub amount: i64,
    pub cleared_date: Option<String>,
    pub statement_line_id: Option<String>,
    /// auto, manual or ticked.
    pub method: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BankQuery {
    pub account_id: String,
    /// Cleared items and matched lines from this day (default: 60 days before `to`).
    pub from: Option<String>,
    /// Default: today.
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BankView {
    pub account_id: String,
    pub account_name: String,
    pub from: String,
    pub to: String,
    pub statement_lines: Vec<StatementLineRow>,
    pub book_lines: Vec<BookLineRow>,
    pub has_profile: bool,
    pub match_days: i64,
}

/// Book lines of a bank account. A voided document and its reversal leave the list together
/// unless one of them was cleared (a deposited cheque that bounced shows both).
const BOOK_LINES: &str = "
WITH lines AS (
  SELECT l.id, e.business_date, e.narration, e.source_type, e.source_id, l.debit - l.credit AS amount,
         e.status, e.reversal_of, coalesce(e.reversal_of, e.id) AS pair_root, e.created_at, l.line_no
  FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id
  WHERE l.account_id = ?1
)
SELECT x.id, x.business_date, x.narration, x.source_type, x.source_id, x.amount,
       p.doc_no, p.reference_no, m.cleared_date, m.statement_line_id, m.method
FROM lines x
LEFT JOIN payments p ON x.source_type = 'payment' AND p.id = x.source_id
LEFT JOIN bank_matches m ON m.journal_line_id = x.id AND m.deleted_at IS NULL
WHERE NOT (
  (x.status = 'reversed' OR x.reversal_of IS NOT NULL)
  AND NOT EXISTS (
    SELECT 1 FROM bank_matches bm JOIN lines y ON y.id = bm.journal_line_id
    WHERE y.pair_root = x.pair_root AND bm.deleted_at IS NULL
  )
)";

fn book_lines(
    conn: &Connection,
    account: &str,
    filter: &str,
    args: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<BookLineRow>> {
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&account];
    all.extend_from_slice(args);
    Ok(conn
        .prepare(&format!(
            "{BOOK_LINES} {filter} ORDER BY x.business_date, x.created_at, x.line_no"
        ))?
        .query_map(all.as_slice(), |r| {
            Ok(BookLineRow {
                journal_line_id: r.get(0)?,
                business_date: r.get(1)?,
                narration: r.get(2)?,
                source_type: r.get(3)?,
                source_id: r.get(4)?,
                amount: r.get(5)?,
                doc_no: r.get(6)?,
                reference: r.get(7)?,
                cleared_date: r.get(8)?,
                statement_line_id: r.get(9)?,
                method: r.get(10)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

fn statement_lines(
    conn: &Connection,
    account: &str,
    from: &str,
    to: &str,
) -> AppResult<Vec<StatementLineRow>> {
    let mut rows: Vec<StatementLineRow> = conn
        .prepare(
            "SELECT id, txn_date, description, reference, amount, balance, status, ignore_note FROM bank_statement_lines
             WHERE account_id = ?1 AND txn_date <= ?3 AND (status = 'unmatched' OR txn_date >= ?2)
             ORDER BY txn_date, rowid",
        )?
        .query_map(params![account, from, to], |r| {
            Ok(StatementLineRow {
                id: r.get(0)?,
                txn_date: r.get(1)?,
                description: r.get(2)?,
                reference: r.get(3)?,
                amount: r.get(4)?,
                balance: r.get(5)?,
                status: r.get(6)?,
                ignore_note: r.get(7)?,
                journal_line_ids: Vec::new(),
            })
        })?
        .collect::<Result<_, _>>()?;
    for row in rows.iter_mut().filter(|r| r.status == "matched") {
        row.journal_line_ids = conn
            .prepare("SELECT journal_line_id FROM bank_matches WHERE statement_line_id = ?1 AND deleted_at IS NULL")?
            .query_map([&row.id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
    }
    Ok(rows)
}

fn range(ctx: &Ctx<'_>, from: Option<&str>, to: Option<&str>) -> AppResult<(String, String)> {
    let settings = settings::load(ctx.conn)?;
    let to = match to {
        Some(d) => date("to", d)?,
        None => settings.business_date(ctx.now),
    };
    let from = match from {
        Some(d) => date("from", d)?,
        None => chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d")
            .map(|d| {
                (d - chrono::Duration::days(60))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|_| to.clone()),
    };
    if from > to {
        return Err(AppError::field("from", "The start is after the end."));
    }
    Ok((from, to))
}

/// Statement lines (unmatched ones of any date, the rest in the range) and book lines
/// (uncleared ones of any date up to `to`, cleared ones in the range).
pub fn view(ctx: &Ctx<'_>, q: &BankQuery) -> AppResult<BankView> {
    ctx.require("bank.reconcile")?;
    let account_name = bank_account(ctx.conn, &q.account_id)?;
    let (from, to) = range(ctx, q.from.as_deref(), q.to.as_deref())?;
    let settings = settings::load(ctx.conn)?;
    let book = book_lines(
        ctx.conn,
        &q.account_id,
        "AND x.business_date <= ?2 AND (m.id IS NULL OR m.cleared_date >= ?3)",
        &[&to, &from],
    )?;
    let has_profile: bool = ctx.conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM bank_import_profiles WHERE account_id = ?1)",
        [&q.account_id],
        |r| r.get(0),
    )?;
    Ok(BankView {
        account_id: q.account_id.clone(),
        account_name,
        statement_lines: statement_lines(ctx.conn, &q.account_id, &from, &to)?,
        book_lines: book,
        from,
        to,
        has_profile,
        match_days: settings.accounting.bank_match_days,
    })
}

// ------------------------------------------------------------------ matching

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutoMatchResult {
    pub matched: usize,
    /// Lines with more than one equally good candidate (left for a person to choose).
    pub ambiguous: usize,
    pub unmatched: usize,
}

fn days_between(a: &str, b: &str) -> i64 {
    let parse = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => (a - b).num_days().abs(),
        _ => i64::MAX,
    }
}

/// Whether the statement text names this book line (its reference or document number).
fn names(statement_text: &str, book: &BookLineRow) -> bool {
    let text = statement_text.to_ascii_lowercase();
    [book.reference.as_deref(), book.doc_no.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|r| r.len() >= 4)
        .any(|r| text.contains(&r.to_ascii_lowercase()))
}

/// Matches every unmatched statement line that has exactly one best candidate: an uncleared
/// book line of the same amount within the date window, the one the statement's reference or
/// text names first, then the nearest in date.
pub fn auto_match(ctx: &mut Ctx<'_>, input: &AccountInput) -> AppResult<AutoMatchResult> {
    ctx.require("bank.reconcile")?;
    let account_name = bank_account(ctx.conn, &input.account_id)?;
    let settings = settings::load(ctx.conn)?;
    let window = settings.accounting.bank_match_days;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let lines: Vec<StatementLineRow> =
            statement_lines(tx, &input.account_id, "0000-01-01", "9999-12-31")?
                .into_iter()
                .filter(|l| l.status == "unmatched")
                .collect();
        let mut book: Vec<BookLineRow> =
            book_lines(tx, &input.account_id, "AND m.id IS NULL", &[])?;
        let mut result = AutoMatchResult {
            matched: 0,
            ambiguous: 0,
            unmatched: 0,
        };
        for line in &lines {
            let text = format!(
                "{} {}",
                line.description,
                line.reference.as_deref().unwrap_or_default()
            );
            let mut candidates: Vec<(usize, bool, i64)> = book
                .iter()
                .enumerate()
                .filter(|(_, b)| b.amount == line.amount)
                .map(|(i, b)| {
                    (
                        i,
                        names(&text, b),
                        days_between(&b.business_date, &line.txn_date),
                    )
                })
                // A named line (cheque number, UTR) may clear later than the window, within reason.
                .filter(|(_, named, days)| (*named && *days <= 90) || *days <= window)
                .collect();
            // Best first: named by the statement, then nearest in date.
            candidates.sort_by_key(|(_, named, days)| (!named, *days));
            let pick = match candidates.as_slice() {
                [] => None,
                [only] => Some(only.0),
                [best, next, ..] => {
                    if (best.1, best.2) == (next.1, next.2) {
                        result.ambiguous += 1;
                        None
                    } else {
                        Some(best.0)
                    }
                }
            };
            let Some(i) = pick else {
                if candidates.is_empty() {
                    result.unmatched += 1;
                }
                continue;
            };
            let book_line = book.remove(i);
            tx.execute(
                "INSERT INTO bank_matches (id, account_id, journal_line_id, statement_line_id, cleared_date, method, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'auto', ?6, ?7)",
                params![new_id(), input.account_id, book_line.journal_line_id, line.id, line.txn_date, now, actor.user_id],
            )?;
            tx.execute(
                "UPDATE bank_statement_lines SET status = 'matched', updated_at = ?1 WHERE id = ?2",
                params![now, line.id],
            )?;
            result.matched += 1;
        }
        if result.matched > 0 {
            AuditEntry::new("BANK_AUTO_MATCHED", "accounts", Some(&input.account_id))
                .by(actor)
                .summary(format!(
                    "{account_name}: {} line(s) matched",
                    result.matched
                ))
                .write(tx, &now)?;
        }
        Ok(result)
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchInput {
    pub statement_line_id: String,
    /// One or more uncleared book lines whose amounts add up to the statement line's.
    pub journal_line_ids: Vec<String>,
}

fn statement_line(conn: &Connection, id: &str) -> AppResult<(String, String, i64, String)> {
    conn.query_row(
        "SELECT account_id, txn_date, amount, status FROM bank_statement_lines WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The statement line"))
}

/// Uncleared book lines of the account among `ids`, with their amounts.
fn uncleared(conn: &Connection, account: &str, ids: &[String]) -> AppResult<Vec<BookLineRow>> {
    let open = book_lines(conn, account, "AND m.id IS NULL", &[])?;
    ids.iter()
        .map(|id| {
            open.iter()
                .find(|b| &b.journal_line_id == id)
                .cloned()
                .ok_or_else(|| {
                    AppError::field(
                        "journalLineIds",
                        "A chosen book line is not an uncleared line of this account.",
                    )
                })
        })
        .collect()
}

pub fn match_lines(ctx: &mut Ctx<'_>, input: &MatchInput) -> AppResult<()> {
    ctx.require("bank.reconcile")?;
    if input.journal_line_ids.is_empty() || input.journal_line_ids.len() > 50 {
        return Err(AppError::field(
            "journalLineIds",
            "Choose the book lines (up to 50) this statement line pays.",
        ));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let (account, day, amount, status) = statement_line(tx, &input.statement_line_id)?;
        if status != "unmatched" {
            return Err(AppError::conflict(
                "This statement line is already matched or ignored.",
            ));
        }
        let mut ids = input.journal_line_ids.clone();
        ids.sort();
        ids.dedup();
        let lines = uncleared(tx, &account, &ids)?;
        let total: i64 = lines.iter().map(|l| l.amount).sum();
        if total != amount {
            return Err(AppError::validation(format!(
                "The chosen book lines add up to {total}, the statement line is {amount}."
            )));
        }
        for l in &lines {
            tx.execute(
                "INSERT INTO bank_matches (id, account_id, journal_line_id, statement_line_id, cleared_date, method, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'manual', ?6, ?7)",
                params![new_id(), account, l.journal_line_id, input.statement_line_id, day, now, actor.user_id],
            )?;
        }
        tx.execute(
            "UPDATE bank_statement_lines SET status = 'matched', updated_at = ?1 WHERE id = ?2",
            params![now, input.statement_line_id],
        )?;
        AuditEntry::new(
            "BANK_MATCHED",
            "bank_statement_lines",
            Some(&input.statement_line_id),
        )
        .by(actor)
        .summary(format!("{day} {amount}: {} book line(s)", lines.len()))
        .write(tx, &now)
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnmatchInput {
    /// Unmatch a statement line (all its book lines)…
    pub statement_line_id: Option<String>,
    /// …or clear the tick of one book line.
    pub journal_line_id: Option<String>,
}

pub fn unmatch(ctx: &mut Ctx<'_>, input: &UnmatchInput) -> AppResult<()> {
    ctx.require("bank.reconcile")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let statement = match (&input.statement_line_id, &input.journal_line_id) {
            (Some(s), _) => Some(s.clone()),
            (None, Some(j)) => tx
                .query_row(
                    "SELECT statement_line_id FROM bank_matches WHERE journal_line_id = ?1 AND deleted_at IS NULL",
                    [j],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()?
                .ok_or_else(|| AppError::conflict("That book line is not cleared."))?,
            (None, None) => {
                return Err(AppError::field("statementLineId", "Say what to unmatch."))
            }
        };
        let changed = match &statement {
            Some(s) => {
                tx.execute(
                    "UPDATE bank_statement_lines SET status = 'unmatched', updated_at = ?1 WHERE id = ?2 AND status = 'matched'",
                    params![now, s],
                )?;
                tx.execute(
                    "UPDATE bank_matches SET deleted_at = ?1, deleted_by = ?2 WHERE statement_line_id = ?3 AND deleted_at IS NULL",
                    params![now, actor.user_id, s],
                )?
            }
            None => tx.execute(
                "UPDATE bank_matches SET deleted_at = ?1, deleted_by = ?2 WHERE journal_line_id = ?3 AND deleted_at IS NULL",
                params![now, actor.user_id, input.journal_line_id],
            )?,
        };
        if changed == 0 {
            return Err(AppError::conflict("Nothing was matched there."));
        }
        AuditEntry::new(
            "BANK_UNMATCHED",
            "bank_statement_lines",
            statement.as_deref().or(input.journal_line_id.as_deref()),
        )
        .by(actor)
        .write(tx, &now)
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreInput {
    pub statement_line_id: String,
    /// A note ignores the line (not a real movement: an opening-balance row, a duplicate);
    /// none brings it back.
    pub note: Option<String>,
}

pub fn ignore(ctx: &mut Ctx<'_>, input: &IgnoreInput) -> AppResult<()> {
    ctx.require("bank.reconcile")?;
    let note = optional("note", input.note.as_deref(), 200)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let (_, _, _, status) = statement_line(tx, &input.statement_line_id)?;
        let (from, to) = if note.is_some() {
            ("unmatched", "ignored")
        } else {
            ("ignored", "unmatched")
        };
        if status != from {
            return Err(AppError::conflict(if note.is_some() {
                "Only an unmatched line can be ignored."
            } else {
                "This line is not ignored."
            }));
        }
        tx.execute(
            "UPDATE bank_statement_lines SET status = ?1, ignore_note = ?2, updated_at = ?3 WHERE id = ?4",
            params![to, note, now, input.statement_line_id],
        )?;
        AuditEntry::new(
            if note.is_some() {
                "BANK_LINE_IGNORED"
            } else {
                "BANK_LINE_RESTORED"
            },
            "bank_statement_lines",
            Some(&input.statement_line_id),
        )
        .by(actor)
        .summary(note.clone().unwrap_or_default())
        .write(tx, &now)
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TickInput {
    pub account_id: String,
    pub journal_line_ids: Vec<String>,
    /// The day the bank cleared them.
    pub cleared_date: String,
}

/// Marks book lines cleared without a statement line (a passbook or online banking check).
pub fn tick(ctx: &mut Ctx<'_>, input: &TickInput) -> AppResult<()> {
    ctx.require("bank.reconcile")?;
    bank_account(ctx.conn, &input.account_id)?;
    let day = date("clearedDate", &input.cleared_date)?;
    if input.journal_line_ids.is_empty() || input.journal_line_ids.len() > 200 {
        return Err(AppError::field(
            "journalLineIds",
            "Choose up to 200 book lines.",
        ));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let lines = uncleared(tx, &input.account_id, &input.journal_line_ids)?;
        for l in &lines {
            if l.business_date > day {
                return Err(AppError::field(
                    "clearedDate",
                    "A line can't clear before its own date.",
                ));
            }
            tx.execute(
                "INSERT INTO bank_matches (id, account_id, journal_line_id, statement_line_id, cleared_date, method, created_at, created_by)
                 VALUES (?1, ?2, ?3, NULL, ?4, 'ticked', ?5, ?6)",
                params![new_id(), input.account_id, l.journal_line_id, day, now, actor.user_id],
            )?;
        }
        AuditEntry::new("BANK_TICKED", "accounts", Some(&input.account_id))
            .by(actor)
            .summary(format!("{} line(s) cleared on {day}", lines.len()))
            .write(tx, &now)
    })
}

// ------------------------------------------------------------------ the statement

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileQuery {
    pub account_id: String,
    pub statement_date: String,
    /// The balance on the bank statement that day; default: the last imported line's balance.
    pub statement_balance: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Reconciliation {
    pub account_id: String,
    pub account_name: String,
    pub statement_date: String,
    pub book_balance: i64,
    /// Money in the books but not yet on the statement (deposits in transit).
    pub uncleared_in: Vec<BookLineRow>,
    pub uncleared_in_total: i64,
    /// Money out in the books but not yet on the statement (cheques not presented).
    pub uncleared_out: Vec<BookLineRow>,
    pub uncleared_out_total: i64,
    /// Statement lines not matched to the books (bank charges, interest…).
    pub not_in_books: Vec<StatementLineRow>,
    pub not_in_books_total: i64,
    /// What the statement should show: book − uncleared in + uncleared out + not in books.
    pub expected_statement_balance: i64,
    pub statement_balance: Option<i64>,
    /// Statement balance − expected (zero when reconciled).
    pub difference: Option<i64>,
}

fn reconciliation_of(conn: &Connection, q: &ReconcileQuery) -> AppResult<Reconciliation> {
    let account_name = bank_account(conn, &q.account_id)?;
    let day = date("statementDate", &q.statement_date)?;
    let book_balance: i64 = conn.query_row(
        "SELECT coalesce(sum(l.debit - l.credit), 0) FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id
         WHERE l.account_id = ?1 AND e.business_date <= ?2",
        params![q.account_id, day],
        |r| r.get(0),
    )?;
    let open = book_lines(
        conn,
        &q.account_id,
        "AND x.business_date <= ?2 AND (m.id IS NULL OR m.cleared_date > ?2)",
        &[&day],
    )?;
    let (uncleared_in, uncleared_out): (Vec<_>, Vec<_>) =
        open.into_iter().partition(|l| l.amount > 0);
    let not_in_books: Vec<StatementLineRow> = statement_lines(conn, &q.account_id, &day, &day)?
        .into_iter()
        .filter(|l| l.status == "unmatched")
        .collect();
    let in_total: i64 = uncleared_in.iter().map(|l| l.amount).sum();
    let out_total: i64 = uncleared_out.iter().map(|l| -l.amount).sum();
    let not_total: i64 = not_in_books.iter().map(|l| l.amount).sum();
    let expected = book_balance - in_total + out_total + not_total;
    let statement_balance = match q.statement_balance {
        Some(b) => Some(b),
        None => conn
            .query_row(
                "SELECT balance FROM bank_statement_lines WHERE account_id = ?1 AND txn_date <= ?2 AND balance IS NOT NULL
                 AND status <> 'ignored' ORDER BY txn_date DESC, rowid DESC LIMIT 1",
                params![q.account_id, day],
                |r| r.get(0),
            )
            .optional()?,
    };
    Ok(Reconciliation {
        account_id: q.account_id.clone(),
        account_name,
        statement_date: day,
        book_balance,
        uncleared_in_total: in_total,
        uncleared_in,
        uncleared_out_total: out_total,
        uncleared_out,
        not_in_books_total: not_total,
        not_in_books,
        expected_statement_balance: expected,
        difference: statement_balance.map(|b| b - expected),
        statement_balance,
    })
}

pub fn reconciliation(ctx: &Ctx<'_>, q: &ReconcileQuery) -> AppResult<Reconciliation> {
    ctx.require("bank.reconcile")?;
    reconciliation_of(ctx.conn, q)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinishInput {
    pub account_id: String,
    pub statement_date: String,
    pub statement_balance: i64,
    /// Required when a difference remains.
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationRecord {
    pub id: String,
    pub statement_date: String,
    pub statement_balance: i64,
    pub book_balance: i64,
    pub uncleared_in: i64,
    pub uncleared_out: i64,
    pub difference: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub created_by_name: Option<String>,
}

/// Keeps the reconciliation as it stands (history). A difference needs a note.
pub fn finish(ctx: &mut Ctx<'_>, input: &FinishInput) -> AppResult<ReconciliationRecord> {
    ctx.require("bank.reconcile")?;
    let notes = optional("notes", input.notes.as_deref(), 500)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = new_id();
    write(ctx.conn, |tx| {
        let r = reconciliation_of(
            tx,
            &ReconcileQuery {
                account_id: input.account_id.clone(),
                statement_date: input.statement_date.clone(),
                statement_balance: Some(input.statement_balance),
            },
        )?;
        let difference = r.difference.unwrap_or_default();
        if difference != 0 && notes.is_none() {
            return Err(AppError::field(
                "notes",
                "Say why the statement and the books still differ.",
            ));
        }
        tx.execute(
            "INSERT INTO bank_reconciliations (id, account_id, statement_date, statement_balance, book_balance, uncleared_in, uncleared_out, difference, notes, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, input.account_id, r.statement_date, input.statement_balance, r.book_balance, r.uncleared_in_total, r.uncleared_out_total, difference, notes, now, actor.user_id],
        )?;
        AuditEntry::new("BANK_RECONCILED", "bank_reconciliations", Some(&id))
            .by(actor)
            .summary(format!(
                "{} on {}: difference {difference}",
                r.account_name, r.statement_date
            ))
            .write(tx, &now)
    })?;
    history(
        ctx,
        &AccountInput {
            account_id: input.account_id.clone(),
        },
    )?
    .into_iter()
    .find(|h| h.id == id)
    .ok_or_else(|| AppError::not_found("The reconciliation"))
}

pub fn history(ctx: &Ctx<'_>, input: &AccountInput) -> AppResult<Vec<ReconciliationRecord>> {
    ctx.require("bank.reconcile")?;
    Ok(ctx
        .conn
        .prepare(
            "SELECT r.id, r.statement_date, r.statement_balance, r.book_balance, r.uncleared_in, r.uncleared_out, r.difference, r.notes, r.created_at, u.name
             FROM bank_reconciliations r LEFT JOIN users u ON u.id = r.created_by
             WHERE r.account_id = ?1 ORDER BY r.statement_date DESC, r.created_at DESC LIMIT 100",
        )?
        .query_map([&input.account_id], |r| {
            Ok(ReconciliationRecord {
                id: r.get(0)?,
                statement_date: r.get(1)?,
                statement_balance: r.get(2)?,
                book_balance: r.get(3)?,
                uncleared_in: r.get(4)?,
                uncleared_out: r.get(5)?,
                difference: r.get(6)?,
                notes: r.get(7)?,
                created_at: r.get(8)?,
                created_by_name: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}
