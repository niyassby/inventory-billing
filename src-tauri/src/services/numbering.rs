//! Gapless document numbers per series and financial year. The counter is incremented inside the
//! caller's write transaction, so a rolled-back sale gives its number back.

use rusqlite::{params, Connection, OptionalExtension};

use super::settings::Settings;
use crate::domain::ids::new_id;
use crate::domain::numbering::{financial_year, format_doc_number};
use crate::error::AppResult;

pub struct Assigned {
    pub series_id: String,
    pub period: String,
    pub seq: i64,
    pub doc_no: String,
}

/// Default prefix per document type (template "{prefix}/{fy}/{seq}", 5 digits: max 15 chars,
/// within the 16-character GST limit).
pub fn default_prefix(doc_type: &str) -> &'static str {
    match doc_type {
        "sales_invoice" => "INV",
        "sales_return" => "CN",
        "quotation" => "QT",
        "sales_order" => "SO",
        "delivery_challan" => "DC",
        "credit_note" => "CRN",
        "purchase_order" => "PO",
        "goods_receipt" => "GRN",
        "purchase_bill" => "PB",
        "purchase_return" => "DN",
        "payment_in" => "RCT",
        "payment_out" => "PMT",
        "expense" => "EXP",
        "stock_transfer" => "TRF",
        "stock_adjustment" => "ADJ",
        "stock_count" => "CNT",
        _ => "DOC",
    }
}

pub const DOC_TYPES: [&str; 16] = [
    "sales_invoice",
    "sales_return",
    "quotation",
    "sales_order",
    "delivery_challan",
    "credit_note",
    "purchase_order",
    "goods_receipt",
    "purchase_bill",
    "purchase_return",
    "payment_in",
    "payment_out",
    "expense",
    "stock_transfer",
    "stock_adjustment",
    "stock_count",
];

/// Creates the default series for a branch and document type if it doesn't exist yet.
pub fn ensure_default_series(
    conn: &Connection,
    branch_id: &str,
    doc_type: &str,
    now: &str,
) -> AppResult<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM number_series WHERE branch_id = ?1 AND doc_type = ?2 AND is_default = 1
               AND device_id IS NULL AND deleted_at IS NULL",
            params![branch_id, doc_type],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO number_series (id, branch_id, doc_type, name, prefix, template, padding, reset_policy,
            is_default, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'Default', ?4, '{prefix}/{fy}/{seq}', 5, 'yearly', 1, ?5, ?5)",
        params![id, branch_id, doc_type, default_prefix(doc_type), now],
    )?;
    Ok(id)
}

/// Takes the next number of the branch's default series for `doc_type`.
pub fn next(
    conn: &Connection,
    settings: &Settings,
    branch_id: &str,
    doc_type: &str,
    business_date: &str,
    now: &str,
) -> AppResult<Assigned> {
    let series_id = ensure_default_series(conn, branch_id, doc_type, now)?;
    let (prefix, template, padding, reset_policy, branch_code): (
        String,
        String,
        i64,
        String,
        String,
    ) = conn.query_row(
        "SELECT s.prefix, s.template, s.padding, s.reset_policy, b.code
         FROM number_series s JOIN branches b ON b.id = s.branch_id WHERE s.id = ?1",
        [&series_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    let fy = financial_year(business_date, settings.business.fy_start_month);
    let period = if reset_policy == "yearly" {
        fy.clone()
    } else {
        "all".to_string()
    };
    let seq: i64 = conn.query_row(
        "INSERT INTO number_series_counters (series_id, period, next_seq, updated_at) VALUES (?1, ?2, 2, ?3)
         ON CONFLICT (series_id, period) DO UPDATE SET next_seq = next_seq + 1, updated_at = excluded.updated_at
         RETURNING next_seq - 1",
        params![series_id, period, now],
        |r| r.get(0),
    )?;
    let doc_no = format_doc_number(
        &template,
        &prefix,
        &fy,
        &branch_code,
        seq,
        usize::try_from(padding).unwrap_or(5),
    );
    Ok(Assigned {
        series_id,
        period,
        seq,
        doc_no,
    })
}
