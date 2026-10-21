//! Reports (Master Prompt §4). Every report is one table: typed columns, rows of plain values and
//! optional totals, so a single screen shows, prints and exports all of them. Money is minor units,
//! quantities thousandths, rates ppm; the screen formats them by column kind.
//!
//! Sales and purchase figures come from finalized documents (voids reverse everything, so voided
//! documents are left out); profit and loss, trial balance and the books come from the journal.

use std::collections::BTreeMap;

use rusqlite::types::Value as Sql;
use rusqlite::{params_from_iter, Connection};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::common::date;
use super::Ctx;
use crate::error::{AppError, AppResult};

/// Reports stop at this many rows (and say so) rather than freezing the screen.
const MAX_ROWS: usize = 5_000;

pub const REPORTS: [&str; 13] = [
    "sales_summary",
    "sales_by_item",
    "purchases",
    "stock_valuation",
    "stock_movement",
    "party_balances",
    "tax_summary",
    "day_book",
    "profit_loss",
    "trial_balance",
    "cash_book",
    "bank_book",
    "expenses",
];

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportQuery {
    pub report: String,
    /// First day (YYYY-MM-DD); default: the start of the current month.
    pub from: Option<String>,
    /// Last day; default: today.
    pub to: Option<String>,
    /// Stock reports: one store only.
    pub warehouse_id: Option<String>,
    /// Party balances: "customer" or "supplier".
    pub party_type: Option<String>,
    /// Cash and bank book: one account (default: all cash, or all bank, accounts).
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub key: String,
    pub label: String,
    /// "text", "date", "money", "qty", "rate", "count" or "percent" (ppm).
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportTable {
    pub report: String,
    pub title: String,
    pub from: String,
    pub to: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    /// One value per column (null where a total makes no sense).
    pub totals: Option<Vec<Value>>,
    /// More rows existed than are shown.
    pub truncated: bool,
}

fn col(key: &str, label: &str, kind: &str) -> Column {
    Column {
        key: key.into(),
        label: label.into(),
        kind: kind.into(),
    }
}

/// Sum of an integer column of the rows (the totals line).
fn total_of(rows: &[Vec<Value>], index: usize) -> Value {
    json!(rows.iter().filter_map(|r| r[index].as_i64()).sum::<i64>())
}

fn finish(
    q: &ReportQuery,
    title: &str,
    range: (&str, &str),
    columns: Vec<Column>,
    mut rows: Vec<Vec<Value>>,
    total_columns: &[usize],
) -> ReportTable {
    let truncated = rows.len() > MAX_ROWS;
    rows.truncate(MAX_ROWS);
    let totals = (!total_columns.is_empty() && !rows.is_empty()).then(|| {
        let mut t = vec![Value::Null; columns.len()];
        t[0] = json!("Total");
        for &i in total_columns {
            t[i] = total_of(&rows, i);
        }
        t
    });
    ReportTable {
        report: q.report.clone(),
        title: title.into(),
        from: range.0.into(),
        to: range.1.into(),
        columns,
        rows,
        totals,
        truncated,
    }
}

fn query_rows(
    conn: &Connection,
    sql: &str,
    args: &[Sql],
    width: usize,
) -> AppResult<Vec<Vec<Value>>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            let mut out = Vec::with_capacity(width);
            for i in 0..width {
                out.push(match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Real(f) => json!(f),
                    rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    rusqlite::types::ValueRef::Blob(_) => Value::Null,
                });
            }
            Ok(out)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn run(ctx: &Ctx<'_>, q: &ReportQuery) -> AppResult<ReportTable> {
    ctx.require("reports.view")?;
    if !REPORTS.contains(&q.report.as_str()) {
        return Err(AppError::field("report", "Unknown report."));
    }
    let settings = super::settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let to = match &q.to {
        Some(d) => date("to", d)?,
        None => today.clone(),
    };
    let from = match &q.from {
        Some(d) => date("from", d)?,
        None => format!("{}-01", &today[..7]),
    };
    if from > to {
        return Err(AppError::field("from", "The start is after the end."));
    }
    let conn: &Connection = ctx.conn;
    let range = (from.as_str(), to.as_str());
    let dates: Vec<Sql> = vec![from.clone().into(), to.clone().into()];
    match q.report.as_str() {
        "sales_summary" => sales_summary(conn, q, range),
        "sales_by_item" => sales_by_item(conn, q, range, &dates),
        "purchases" => purchases(conn, q, range, &dates),
        "stock_valuation" => stock_valuation(conn, q, range),
        "stock_movement" => stock_movement(conn, q, range, &dates),
        "party_balances" => party_balances(conn, q, range),
        "tax_summary" => tax_summary(conn, q, range, &dates),
        "day_book" => day_book(conn, q, range, &dates),
        "profit_loss" => profit_loss(conn, q, range, &dates),
        "trial_balance" => trial_balance(conn, q, range),
        "cash_book" | "bank_book" => books(conn, q, range),
        "expenses" => expenses(conn, q, range, &dates),
        _ => unreachable!("checked above"),
    }
}

fn sales_summary(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
) -> AppResult<ReportTable> {
    let args: Vec<Sql> = vec![range.0.to_string().into(), range.1.to_string().into()];
    // day → [bills, taxable, tax, round_off, total, returns]
    let mut days: BTreeMap<String, [i64; 6]> = BTreeMap::new();
    for r in query_rows(
        conn,
        "SELECT business_date, count(*), sum(taxable), sum(tax_total), sum(round_off), sum(total)
         FROM sales_invoices WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2 GROUP BY business_date",
        &args,
        6,
    )? {
        let d = days.entry(r[0].as_str().unwrap_or_default().into()).or_default();
        for i in 0..5 {
            d[i] = r[i + 1].as_i64().unwrap_or(0);
        }
    }
    for r in query_rows(
        conn,
        "SELECT business_date, sum(total) FROM sales_returns WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2 GROUP BY business_date",
        &args,
        2,
    )? {
        days.entry(r[0].as_str().unwrap_or_default().into()).or_default()[5] = r[1].as_i64().unwrap_or(0);
    }
    let rows = days
        .into_iter()
        .map(|(day, d)| {
            vec![
                json!(day),
                json!(d[0]),
                json!(d[1]),
                json!(d[2]),
                json!(d[3]),
                json!(d[4]),
                json!(d[5]),
                json!(d[4] - d[5]),
            ]
        })
        .collect();
    Ok(finish(
        q,
        "Sales summary",
        range,
        vec![
            col("date", "Date", "date"),
            col("bills", "Bills", "count"),
            col("taxable", "Taxable value", "money"),
            col("tax", "Tax", "money"),
            col("roundOff", "Round off", "money"),
            col("total", "Sales", "money"),
            col("returns", "Returns", "money"),
            col("net", "Net sales", "money"),
        ],
        rows,
        &[1, 2, 3, 4, 5, 6, 7],
    ))
}

/// Item profit: what each item sold for (after discounts, before tax), what it cost, and the
/// difference. Returns of restocked items take their sale and cost back.
fn sales_by_item(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    let raw = query_rows(
        conn,
        "SELECT item, sku, sum(qty), sum(revenue), sum(cost) FROM (
            SELECT l.item_name AS item, l.sku AS sku, l.base_qty AS qty, l.taxable AS revenue,
                   (l.unit_cost * l.base_qty + 500) / 1000 AS cost
            FROM sales_invoice_items l JOIN sales_invoices i ON i.id = l.invoice_id
            WHERE i.status = 'finalized' AND i.business_date BETWEEN ?1 AND ?2
            UNION ALL
            SELECT r.item_name, r.sku, -r.base_qty, -r.taxable,
                   -CASE WHEN r.restock = 1 THEN (r.unit_cost * r.base_qty + 500) / 1000 ELSE 0 END
            FROM sales_return_items r JOIN sales_returns s ON s.id = r.return_id
            WHERE s.status = 'finalized' AND s.business_date BETWEEN ?1 AND ?2
         ) GROUP BY item, sku ORDER BY sum(revenue) DESC",
        dates,
        5,
    )?;
    let rows = raw
        .into_iter()
        .map(|r| {
            let (revenue, cost) = (r[3].as_i64().unwrap_or(0), r[4].as_i64().unwrap_or(0));
            let margin = if revenue > 0 {
                (revenue - cost) * 1_000_000 / revenue
            } else {
                0
            };
            vec![
                r[0].clone(),
                r[1].clone(),
                r[2].clone(),
                json!(revenue),
                json!(cost),
                json!(revenue - cost),
                json!(margin),
            ]
        })
        .collect();
    let mut table = finish(
        q,
        "Item profit",
        range,
        vec![
            col("item", "Item", "text"),
            col("sku", "SKU", "text"),
            col("qty", "Qty sold", "qty"),
            col("revenue", "Sales (before tax)", "money"),
            col("cost", "Cost", "money"),
            col("profit", "Profit", "money"),
            col("margin", "Margin", "percent"),
        ],
        rows,
        &[2, 3, 4, 5],
    );
    if let Some(t) = table.totals.as_mut() {
        let (rev, profit) = (t[3].as_i64().unwrap_or(0), t[5].as_i64().unwrap_or(0));
        t[6] = json!(if rev > 0 { profit * 1_000_000 / rev } else { 0 });
    }
    Ok(table)
}

fn purchases(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    let rows = query_rows(
        conn,
        "SELECT business_date, doc_no, supplier_name, supplier_invoice_no, taxable, tax_total, total, amount_paid
         FROM purchase_bills WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2
         ORDER BY business_date, doc_no",
        dates,
        8,
    )?;
    Ok(finish(
        q,
        "Purchases",
        range,
        vec![
            col("date", "Date", "date"),
            col("docNo", "Bill", "text"),
            col("supplier", "Supplier", "text"),
            col("theirNo", "Their bill no.", "text"),
            col("taxable", "Taxable value", "money"),
            col("tax", "Tax", "money"),
            col("total", "Total", "money"),
            col("paid", "Paid", "money"),
        ],
        rows,
        &[4, 5, 6, 7],
    ))
}

fn stock_valuation(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
) -> AppResult<ReportTable> {
    let mut args: Vec<Sql> = Vec::new();
    let filter = match &q.warehouse_id {
        Some(w) => {
            args.push(w.clone().into());
            "AND l.warehouse_id = ?1"
        }
        None => "",
    };
    let rows = query_rows(
        conn,
        &format!(
            "SELECT p.name || coalesce(' — ' || v.variant_name, ''), v.sku, coalesce(c.name, ''), sum(l.qty), sum(l.value)
             FROM stock_levels l JOIN product_variants v ON v.id = l.variant_id JOIN products p ON p.id = v.product_id
             LEFT JOIN categories c ON c.id = p.category_id
             WHERE p.is_service = 0 {filter} GROUP BY v.id HAVING sum(l.qty) <> 0 OR sum(l.value) <> 0
             ORDER BY lower(p.name)"
        ),
        &args,
        5,
    )?;
    let rows = rows
        .into_iter()
        .map(|r| {
            let (qty, value) = (r[3].as_i64().unwrap_or(0), r[4].as_i64().unwrap_or(0));
            let avg = if qty > 0 { value * 1000 / qty } else { 0 };
            vec![
                r[0].clone(),
                r[1].clone(),
                r[2].clone(),
                json!(qty),
                json!(avg),
                json!(value),
            ]
        })
        .collect();
    let mut table = finish(
        q,
        "Stock valuation (weighted average cost)",
        range,
        vec![
            col("item", "Item", "text"),
            col("sku", "SKU", "text"),
            col("category", "Category", "text"),
            col("qty", "In stock", "qty"),
            col("avgCost", "Average cost", "money"),
            col("value", "Value", "money"),
        ],
        rows,
        &[5],
    );
    table.from = range.1.into();
    Ok(table)
}

fn stock_movement(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    let mut args: Vec<Sql> = dates.to_vec();
    let filter = match &q.warehouse_id {
        Some(w) => {
            args.push(w.clone().into());
            "AND m.warehouse_id = ?3"
        }
        None => "",
    };
    // Add up the movements per variant first (one pass over the index), then name and sort only
    // the variants that moved.
    let rows = query_rows(
        conn,
        &format!(
            "SELECT p.name || coalesce(' — ' || v.variant_name, ''), v.sku, t.opening, t.qty_in, t.qty_out, t.closing
             FROM (
               SELECT m.variant_id,
                 coalesce(sum(CASE WHEN m.business_date < ?1 THEN m.qty END), 0) AS opening,
                 coalesce(sum(CASE WHEN m.business_date >= ?1 AND m.qty > 0 THEN m.qty END), 0) AS qty_in,
                 coalesce(sum(CASE WHEN m.business_date >= ?1 AND m.qty < 0 THEN -m.qty END), 0) AS qty_out,
                 sum(m.qty) AS closing
               FROM stock_movements m WHERE m.business_date <= ?2 {filter} GROUP BY m.variant_id
             ) t
             JOIN product_variants v ON v.id = t.variant_id JOIN products p ON p.id = v.product_id
             WHERE t.opening <> 0 OR t.qty_in <> 0 OR t.qty_out <> 0 OR t.closing <> 0
             ORDER BY lower(p.name)"
        ),
        &args,
        6,
    )?;
    Ok(finish(
        q,
        "Stock movement",
        range,
        vec![
            col("item", "Item", "text"),
            col("sku", "SKU", "text"),
            col("opening", "Opening", "qty"),
            col("in", "In", "qty"),
            col("out", "Out", "qty"),
            col("closing", "Closing", "qty"),
        ],
        rows,
        &[2, 3, 4, 5],
    ))
}

fn party_balances(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
) -> AppResult<ReportTable> {
    let supplier = q.party_type.as_deref() == Some("supplier");
    // A party that is both a customer and a supplier has one balance (what they owe us less what
    // we owe them), so it is listed once, on the side the balance falls on.
    let (kinds, title, sign) = if supplier {
        (
            "(party_type = 'supplier' OR (party_type = 'both' AND balance < 0))",
            "Payables: what you owe suppliers",
            -1,
        )
    } else {
        (
            "(party_type = 'customer' OR (party_type = 'both' AND balance > 0))",
            "Receivables: what customers owe you",
            1,
        )
    };
    let rows = query_rows(
        conn,
        &format!(
            "SELECT name, coalesce(phone, ''), {sign} * balance, coalesce(credit_limit, 0),
                (SELECT max(business_date) FROM party_ledger l WHERE l.party_id = parties.id)
             FROM parties WHERE {kinds} AND deleted_at IS NULL AND balance <> 0
             ORDER BY {sign} * balance DESC"
        ),
        &[],
        5,
    )?;
    let mut table = finish(
        q,
        title,
        range,
        vec![
            col("name", "Name", "text"),
            col("phone", "Phone", "text"),
            col(
                "balance",
                if supplier { "You owe" } else { "They owe" },
                "money",
            ),
            col("limit", "Credit limit", "money"),
            col("last", "Last entry", "date"),
        ],
        rows,
        &[2],
    );
    table.from = range.1.into();
    Ok(table)
}

fn tax_summary(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    // direction, rate, taxable, cgst, sgst, igst, cess + other: credit notes and purchase returns
    // are subtracted so the figures are what is payable or claimable.
    let rows = query_rows(
        conn,
        "SELECT direction, tax_rate, sum(taxable), sum(cgst), sum(sgst), sum(igst), sum(cess + other_tax) FROM (
            SELECT 'Output (sales)' AS direction, l.tax_rate, l.taxable, l.cgst, l.sgst, l.igst, l.cess, l.other_tax
              FROM sales_invoice_items l JOIN sales_invoices i ON i.id = l.invoice_id
              WHERE i.status = 'finalized' AND i.business_date BETWEEN ?1 AND ?2
            UNION ALL
            SELECT 'Output (sales)', r.tax_rate, -r.taxable, -r.cgst, -r.sgst, -r.igst, -r.cess, -r.other_tax
              FROM sales_return_items r JOIN sales_returns s ON s.id = r.return_id
              WHERE s.status = 'finalized' AND s.business_date BETWEEN ?1 AND ?2
            UNION ALL
            SELECT 'Input (purchases)', l.tax_rate, l.taxable, l.cgst, l.sgst, l.igst, l.cess, l.other_tax
              FROM purchase_bill_items l JOIN purchase_bills b ON b.id = l.bill_id
              WHERE b.status = 'finalized' AND b.business_date BETWEEN ?1 AND ?2
            UNION ALL
            SELECT 'Input (purchases)', r.tax_rate, -r.taxable, -r.cgst, -r.sgst, -r.igst, -r.cess, -r.other_tax
              FROM purchase_return_items r JOIN purchase_returns s ON s.id = r.return_id
              WHERE s.status = 'finalized' AND s.business_date BETWEEN ?1 AND ?2
         ) GROUP BY direction, tax_rate ORDER BY direction DESC, tax_rate",
        dates,
        7,
    )?;
    let mut rows: Vec<Vec<Value>> = rows
        .into_iter()
        .map(|r| {
            let tax: i64 = (3..7).map(|i| r[i].as_i64().unwrap_or(0)).sum();
            let mut row = r;
            row.push(json!(tax));
            row
        })
        .collect();
    // Net payable: output tax minus input tax.
    let sum = |dir: &str, i: usize| -> i64 {
        rows.iter()
            .filter(|r| r[0].as_str() == Some(dir))
            .filter_map(|r| r[i].as_i64())
            .sum()
    };
    if !rows.is_empty() {
        let net = |i: usize| json!(sum("Output (sales)", i) - sum("Input (purchases)", i));
        rows.push(vec![
            json!("Net tax payable"),
            Value::Null,
            net(2),
            net(3),
            net(4),
            net(5),
            net(6),
            net(7),
        ]);
    }
    let mut table = finish(
        q,
        "Tax summary",
        range,
        vec![
            col("direction", "Tax on", "text"),
            col("rate", "Rate", "rate"),
            col("taxable", "Taxable value", "money"),
            col("cgst", "CGST", "money"),
            col("sgst", "SGST", "money"),
            col("igst", "IGST", "money"),
            col("other", "Cess and other", "money"),
            col("tax", "Total tax", "money"),
        ],
        rows,
        &[],
    );
    table.totals = None;
    Ok(table)
}

fn day_book(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    let rows = query_rows(
        conn,
        "SELECT e.business_date, e.source_type, e.narration, sum(l.debit),
                CASE WHEN e.reversal_of IS NOT NULL THEN 'reversal' WHEN e.status = 'reversed' THEN 'reversed' ELSE 'posted' END
         FROM journal_entries e JOIN journal_lines l ON l.entry_id = e.id
         WHERE e.business_date BETWEEN ?1 AND ?2 GROUP BY e.id ORDER BY e.business_date, e.created_at LIMIT 5001",
        dates,
        5,
    )?;
    Ok(finish(
        q,
        "Day book",
        range,
        vec![
            col("date", "Date", "date"),
            col("source", "Document", "text"),
            col("narration", "Narration", "text"),
            col("amount", "Amount", "money"),
            col("status", "Status", "text"),
        ],
        rows,
        &[],
    ))
}

/// Net balance of accounts of one type in the range: (code, name, subtype, signed amount), where
/// income is credit − debit and expense is debit − credit.
fn account_balances(
    conn: &Connection,
    account_type: &str,
    dates: &[Sql],
) -> AppResult<Vec<(String, String, String, i64)>> {
    let sign = if account_type == "income" {
        "l.credit - l.debit"
    } else {
        "l.debit - l.credit"
    };
    let mut args = dates.to_vec();
    args.push(account_type.to_string().into());
    let rows = query_rows(
        conn,
        &format!(
            "SELECT a.code, a.name, a.subtype, sum({sign}) FROM journal_lines l
             JOIN journal_entries e ON e.id = l.entry_id JOIN accounts a ON a.id = l.account_id
             WHERE e.business_date BETWEEN ?1 AND ?2 AND a.account_type = ?3
             GROUP BY a.id HAVING sum({sign}) <> 0 ORDER BY a.code"
        ),
        &args,
        4,
    )?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r[0].as_str().unwrap_or_default().into(),
                r[1].as_str().unwrap_or_default().into(),
                r[2].as_str().unwrap_or_default().into(),
                r[3].as_i64().unwrap_or(0),
            )
        })
        .collect())
}

fn profit_loss(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    let income = account_balances(conn, "income", dates)?;
    let expense = account_balances(conn, "expense", dates)?;
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let line = |section: &str, code: &str, name: &str, amount: i64| {
        vec![json!(section), json!(code), json!(name), json!(amount)]
    };
    let total_income: i64 = income.iter().map(|a| a.3).sum();
    for (code, name, _, amount) in &income {
        rows.push(line("Income", code, name, *amount));
    }
    rows.push(line("Income", "", "Total income", total_income));
    let cogs: i64 = expense.iter().filter(|a| a.2 == "cogs").map(|a| a.3).sum();
    for (code, name, _, amount) in expense.iter().filter(|a| a.2 == "cogs") {
        rows.push(line("Cost of goods sold", code, name, *amount));
    }
    rows.push(line(
        "Gross profit",
        "",
        "Gross profit",
        total_income - cogs,
    ));
    let others: i64 = expense.iter().filter(|a| a.2 != "cogs").map(|a| a.3).sum();
    for (code, name, _, amount) in expense.iter().filter(|a| a.2 != "cogs") {
        rows.push(line("Expenses", code, name, *amount));
    }
    rows.push(line("Expenses", "", "Total expenses", others));
    rows.push(line(
        "Net profit",
        "",
        "Net profit",
        total_income - cogs - others,
    ));
    Ok(finish(
        q,
        "Profit and loss",
        range,
        vec![
            col("section", "Section", "text"),
            col("code", "Code", "text"),
            col("account", "Account", "text"),
            col("amount", "Amount", "money"),
        ],
        rows,
        &[],
    ))
}

fn trial_balance(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
) -> AppResult<ReportTable> {
    let args: Vec<Sql> = vec![range.1.to_string().into()];
    let rows = query_rows(
        conn,
        "SELECT a.code, a.name, a.account_type,
                CASE WHEN sum(l.debit - l.credit) > 0 THEN sum(l.debit - l.credit) ELSE 0 END,
                CASE WHEN sum(l.debit - l.credit) < 0 THEN -sum(l.debit - l.credit) ELSE 0 END
         FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id JOIN accounts a ON a.id = l.account_id
         WHERE e.business_date <= ?1 GROUP BY a.id HAVING sum(l.debit - l.credit) <> 0 ORDER BY a.code",
        &args,
        5,
    )?;
    let mut table = finish(
        q,
        "Trial balance",
        range,
        vec![
            col("code", "Code", "text"),
            col("account", "Account", "text"),
            col("type", "Type", "text"),
            col("debit", "Debit", "money"),
            col("credit", "Credit", "money"),
        ],
        rows,
        &[3, 4],
    );
    table.from = range.1.into();
    Ok(table)
}

/// Cash book or bank book: every posting to the cash (or bank) accounts with a running balance.
fn books(conn: &Connection, q: &ReportQuery, range: (&str, &str)) -> AppResult<ReportTable> {
    let subtype = if q.report == "cash_book" {
        "cash"
    } else {
        "bank"
    };
    let mut args: Vec<Sql> = vec![subtype.to_string().into()];
    let account = match &q.account_id {
        Some(a) => {
            args.push(a.clone().into());
            "AND a.id = ?2"
        }
        None => "",
    };
    let n = args.len();
    let opening: i64 = {
        let mut a = args.clone();
        a.push(range.0.to_string().into());
        conn.query_row(
            &format!(
                "SELECT coalesce(sum(l.debit - l.credit), 0) FROM journal_lines l
                 JOIN journal_entries e ON e.id = l.entry_id JOIN accounts a ON a.id = l.account_id
                 WHERE a.subtype = ?1 {account} AND e.business_date < ?{}",
                n + 1
            ),
            params_from_iter(a.iter()),
            |r| r.get(0),
        )?
    };
    args.push(range.0.to_string().into());
    args.push(range.1.to_string().into());
    let raw = query_rows(
        conn,
        &format!(
            "SELECT e.business_date, a.name, e.narration, l.debit, l.credit
             FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id JOIN accounts a ON a.id = l.account_id
             WHERE a.subtype = ?1 {account} AND e.business_date BETWEEN ?{} AND ?{}
             ORDER BY e.business_date, e.created_at, l.line_no",
            n + 1,
            n + 2
        ),
        &args,
        5,
    )?;
    let mut balance = opening;
    let mut rows = vec![vec![
        json!(range.0),
        Value::Null,
        json!("Opening balance"),
        Value::Null,
        Value::Null,
        json!(opening),
    ]];
    for r in raw {
        let (debit, credit) = (r[3].as_i64().unwrap_or(0), r[4].as_i64().unwrap_or(0));
        balance += debit - credit;
        rows.push(vec![
            r[0].clone(),
            r[1].clone(),
            r[2].clone(),
            json!(debit),
            json!(credit),
            json!(balance),
        ]);
    }
    let title = if subtype == "cash" {
        "Cash book"
    } else {
        "Bank book"
    };
    let mut table = finish(
        q,
        title,
        range,
        vec![
            col("date", "Date", "date"),
            col("account", "Account", "text"),
            col("narration", "Narration", "text"),
            col("in", "Received", "money"),
            col("out", "Paid", "money"),
            col("balance", "Balance", "money"),
        ],
        rows,
        &[3, 4],
    );
    if let Some(t) = table.totals.as_mut() {
        t[5] = json!(balance);
    }
    Ok(table)
}

fn expenses(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
) -> AppResult<ReportTable> {
    let rows = query_rows(
        conn,
        "SELECT c.name, count(*), sum(e.amount), sum(e.tax_total), sum(e.total) FROM expenses e
         JOIN expense_categories c ON c.id = e.category_id
         WHERE e.status = 'finalized' AND e.business_date BETWEEN ?1 AND ?2 GROUP BY c.id ORDER BY sum(e.total) DESC",
        dates,
        5,
    )?;
    Ok(finish(
        q,
        "Expenses by category",
        range,
        vec![
            col("category", "Category", "text"),
            col("count", "Entries", "count"),
            col("amount", "Amount", "money"),
            col("tax", "Tax", "money"),
            col("total", "Total", "money"),
        ],
        rows,
        &[1, 2, 3, 4],
    ))
}
