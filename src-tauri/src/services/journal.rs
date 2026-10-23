//! Double-entry posting. Every financial document posts a balanced journal entry in the same
//! transaction; an unbalanced entry is a bug and aborts the whole operation.

use rusqlite::{params, Connection};

use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct JLine {
    pub account_id: String,
    pub party_id: Option<String>,
    /// Positive = debit, negative = credit.
    pub amount: i64,
    pub memo: Option<String>,
}

impl JLine {
    pub fn debit(account_id: &str, amount: i64) -> Self {
        Self {
            account_id: account_id.into(),
            party_id: None,
            amount,
            memo: None,
        }
    }

    pub fn credit(account_id: &str, amount: i64) -> Self {
        Self {
            account_id: account_id.into(),
            party_id: None,
            amount: -amount,
            memo: None,
        }
    }

    pub fn party(mut self, party_id: Option<&str>) -> Self {
        self.party_id = party_id.map(String::from);
        self
    }
}

pub struct Entry<'a> {
    pub business_date: &'a str,
    pub source_type: &'a str,
    pub source_id: &'a str,
    pub narration: String,
    pub lines: Vec<JLine>,
    pub user_id: Option<&'a str>,
    pub now: &'a str,
}

/// Posts the entry; zero lines are dropped. Returns the entry id (None if nothing to post).
pub fn post(conn: &Connection, entry: Entry<'_>) -> AppResult<Option<String>> {
    insert(conn, entry, None)
}

fn insert(
    conn: &Connection,
    entry: Entry<'_>,
    reversal_of: Option<&str>,
) -> AppResult<Option<String>> {
    let lines: Vec<JLine> = entry.lines.into_iter().filter(|l| l.amount != 0).collect();
    if lines.is_empty() {
        return Ok(None);
    }
    let balance: i64 = lines.iter().map(|l| l.amount).sum();
    if balance != 0 {
        return Err(AppError::internal(format!(
            "unbalanced journal for {} {} (difference {balance})",
            entry.source_type, entry.source_id
        )));
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO journal_entries (id, business_date, source_type, source_id, narration, reversal_of, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, entry.business_date, entry.source_type, entry.source_id, entry.narration, reversal_of, entry.now, entry.user_id],
    )?;
    for (index, line) in lines.iter().enumerate() {
        conn.execute(
            "INSERT INTO journal_lines (id, entry_id, line_no, account_id, party_id, debit, credit, memo, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                new_id(),
                id,
                index + 1,
                line.account_id,
                line.party_id,
                line.amount.max(0),
                (-line.amount).max(0),
                line.memo,
                entry.now
            ],
        )?;
    }
    Ok(Some(id))
}

/// Posts `entry` as the reversal of journal entry `original` (its own lines, which must undo the
/// original's) and marks the original reversed. Used by manual journals, whose reversal is a
/// reviewed document of its own.
pub fn post_reversal(conn: &Connection, entry: Entry<'_>, original: &str) -> AppResult<String> {
    let id = insert(conn, entry, Some(original))?
        .ok_or_else(|| AppError::internal("a reversal with no lines"))?;
    conn.execute(
        "UPDATE journal_entries SET status = 'reversed' WHERE id = ?1 AND status = 'posted'",
        [original],
    )?;
    Ok(id)
}

/// Reverses every posted entry of a source document (swapped debits/credits) and marks the
/// originals reversed.
pub fn reverse_source(
    conn: &Connection,
    source_type: &str,
    source_id: &str,
    business_date: &str,
    narration: &str,
    user_id: Option<&str>,
    now: &str,
) -> AppResult<()> {
    let entries: Vec<String> = conn
        .prepare("SELECT id FROM journal_entries WHERE source_type = ?1 AND source_id = ?2 AND status = 'posted' AND reversal_of IS NULL")?
        .query_map(params![source_type, source_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for original in entries {
        let lines: Vec<JLine> = conn
            .prepare("SELECT account_id, party_id, debit, credit FROM journal_lines WHERE entry_id = ?1 ORDER BY line_no")?
            .query_map([&original], |r| {
                let (debit, credit): (i64, i64) = (r.get(2)?, r.get(3)?);
                Ok(JLine { account_id: r.get(0)?, party_id: r.get(1)?, amount: credit - debit, memo: None })
            })?
            .collect::<Result<_, _>>()?;
        insert(
            conn,
            Entry {
                business_date,
                source_type,
                source_id,
                narration: narration.to_string(),
                lines,
                user_id,
                now,
            },
            Some(&original),
        )?;
        conn.execute(
            "UPDATE journal_entries SET status = 'reversed' WHERE id = ?1",
            [&original],
        )?;
    }
    Ok(())
}
