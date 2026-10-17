//! Receivable / payable postings. Debit = the party owes us more (sale, refund paid out to a
//! customer, payment to a supplier); credit = we owe them more (purchase, payment received,
//! return). `parties.balance` follows by trigger.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

pub struct Posting<'a> {
    pub party_id: &'a str,
    pub business_date: &'a str,
    pub entry_type: &'a str,
    pub source_type: &'a str,
    pub source_id: &'a str,
    /// Positive = debit, negative = credit.
    pub amount: i64,
    pub narration: String,
    pub user_id: Option<&'a str>,
    pub now: &'a str,
}

pub fn post(conn: &Connection, p: Posting<'_>) -> AppResult<()> {
    if p.amount == 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO party_ledger (id, party_id, business_date, entry_type, source_type, source_id, debit, credit,
            narration, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            new_id(),
            p.party_id,
            p.business_date,
            p.entry_type,
            p.source_type,
            p.source_id,
            p.amount.max(0),
            (-p.amount).max(0),
            p.narration,
            p.now,
            p.user_id
        ],
    )?;
    Ok(())
}

/// Posts the opposite of every ledger row of a source document (used by voids).
pub fn reverse_source(
    conn: &Connection,
    source_type: &str,
    source_id: &str,
    business_date: &str,
    narration: &str,
    user_id: Option<&str>,
    now: &str,
) -> AppResult<()> {
    let rows: Vec<(String, i64, i64)> = conn
        .prepare("SELECT party_id, debit, credit FROM party_ledger WHERE source_type = ?1 AND source_id = ?2 AND entry_type <> 'void_reversal'")?
        .query_map(params![source_type, source_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for (party_id, debit, credit) in rows {
        post(
            conn,
            Posting {
                party_id: &party_id,
                business_date,
                entry_type: "void_reversal",
                source_type,
                source_id,
                amount: credit - debit,
                narration: narration.to_string(),
                user_id,
                now,
            },
        )?;
    }
    Ok(())
}

pub struct PartyRef {
    pub id: String,
    pub party_type: String,
    pub name: String,
    pub phone: Option<String>,
    pub gstin: Option<String>,
    pub state_code: Option<String>,
    pub billing_address: Option<String>,
    pub shipping_address: Option<String>,
    pub credit_limit: Option<i64>,
    pub balance: i64,
}

/// Loads a live, active party and checks it can act as `role` ("customer" or "supplier").
pub fn load_party(conn: &Connection, party_id: &str, role: &str) -> AppResult<PartyRef> {
    let party = conn
        .query_row(
            "SELECT id, party_type, name, phone, gstin, state_code, billing_address, shipping_address, credit_limit, balance
             FROM parties WHERE id = ?1 AND deleted_at IS NULL AND is_active = 1",
            [party_id],
            |r| {
                Ok(PartyRef {
                    id: r.get(0)?,
                    party_type: r.get(1)?,
                    name: r.get(2)?,
                    phone: r.get(3)?,
                    gstin: r.get(4)?,
                    state_code: r.get(5)?,
                    billing_address: r.get(6)?,
                    shipping_address: r.get(7)?,
                    credit_limit: r.get(8)?,
                    balance: r.get(9)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The customer or supplier"))?;
    if party.party_type != role && party.party_type != "both" {
        return Err(AppError::field(
            "partyId",
            format!("{} is not set up as a {role}.", party.name),
        ));
    }
    Ok(party)
}
