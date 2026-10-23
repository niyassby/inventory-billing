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

pub const REPORTS: [&str; 15] = [
    "sales_summary",
    "sales_invoices",
    "sales_by_item",
    "purchases",
    "stock_valuation",
    "stock_movement",
    "expiry",
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
    /// How rows are grouped (the report's `groupOptions`; default: the first).
    #[serde(default)]
    pub group_by: Option<String>,
    /// Drill-down filters: one customer or supplier ("walk_in" = bills without one)…
    #[serde(default)]
    pub party_id: Option<String>,
    /// …one cashier…
    #[serde(default)]
    pub user_id: Option<String>,
    /// …one category (product category, or expense category on the expenses report)…
    #[serde(default)]
    pub category_id: Option<String>,
    /// …one item.
    #[serde(default)]
    pub variant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupOption {
    pub key: String,
    pub label: String,
}

/// Where a row leads: another report (with these parameters) or a document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RowLink {
    /// "report", or a document: "sales_invoice", "sales_return", "purchase_bill",
    /// "goods_receipt", "purchase_return", "payment", "manual_journal".
    pub kind: String,
    pub id: Option<String>,
    pub report: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub group_by: Option<String>,
    pub party_id: Option<String>,
    pub user_id: Option<String>,
    pub category_id: Option<String>,
    pub variant_id: Option<String>,
}

impl RowLink {
    fn doc(kind: &str, id: &Value) -> Option<Self> {
        id.as_str().map(|id| Self {
            kind: kind.into(),
            id: Some(id.into()),
            report: None,
            from: None,
            to: None,
            group_by: None,
            party_id: None,
            user_id: None,
            category_id: None,
            variant_id: None,
        })
    }

    fn report(report: &str, range: (&str, &str)) -> Self {
        Self {
            kind: "report".into(),
            id: None,
            report: Some(report.into()),
            from: Some(range.0.into()),
            to: Some(range.1.into()),
            group_by: None,
            party_id: None,
            user_id: None,
            category_id: None,
            variant_id: None,
        }
    }
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
    /// The grouping used, and the ones this report offers.
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub group_options: Vec<GroupOption>,
    /// Where each row leads (same order as `rows`; empty when rows lead nowhere).
    #[serde(default)]
    pub links: Vec<Option<RowLink>>,
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
        group_by: None,
        group_options: Vec::new(),
        links: Vec::new(),
    }
}

/// The groupings each report offers (first = default).
fn group_options(report: &str) -> &'static [(&'static str, &'static str)] {
    match report {
        "sales_summary" => &[
            ("day", "Day"),
            ("month", "Month"),
            ("customer", "Customer"),
            ("user", "Cashier"),
            ("item", "Item"),
            ("category", "Category"),
        ],
        "sales_by_item" => &[("item", "Item"), ("category", "Category")],
        "purchases" => &[
            ("bill", "Bill"),
            ("supplier", "Supplier"),
            ("month", "Month"),
        ],
        "expenses" => &[("category", "Category"), ("month", "Month")],
        _ => &[],
    }
}

/// Adds the grouping and the row links (cut to the rows kept) to a finished table.
fn grouped(mut table: ReportTable, group: &str, links: Vec<Option<RowLink>>) -> ReportTable {
    let options = group_options(&table.report);
    table.group_by = (!options.is_empty()).then(|| group.to_string());
    table.group_options = options
        .iter()
        .map(|(key, label)| GroupOption {
            key: (*key).into(),
            label: (*label).into(),
        })
        .collect();
    let mut links = links;
    links.truncate(table.rows.len());
    if links.iter().any(Option::is_some) {
        table.links = links;
    }
    table
}

/// First and last day of a "YYYY-MM" month, kept inside the report's range.
fn month_range(month: &str, range: (&str, &str)) -> (String, String) {
    let start = format!("{month}-01");
    let end = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_add_months(chrono::Months::new(1)))
        .and_then(|d| d.pred_opt())
        .map_or_else(|| range.1.to_string(), |d| d.format("%Y-%m-%d").to_string());
    (start.max(range.0.to_string()), end.min(range.1.to_string()))
}

/// SQL conditions (and their arguments, numbered after `first`) for the drill-down filters on
/// sales invoices `i` (and their lines `l` / products `pr` when the query joins them).
fn invoice_filters(q: &ReportQuery, first: usize, lines: bool) -> (String, Vec<Sql>) {
    let mut sql = String::new();
    let mut args: Vec<Sql> = Vec::new();
    let next = |cond: &str, value: Option<String>, sql: &mut String, args: &mut Vec<Sql>| {
        let n = first + args.len();
        match value {
            Some(v) => {
                sql.push_str(&format!(" AND {}", cond.replace('?', &format!("?{n}"))));
                args.push(v.into());
            }
            None => sql.push_str(&format!(" AND {cond}")),
        }
    };
    match q.party_id.as_deref().filter(|p| !p.is_empty()) {
        Some("walk_in") => next("i.party_id IS NULL", None, &mut sql, &mut args),
        Some(p) => next("i.party_id = ?", Some(p.into()), &mut sql, &mut args),
        None => {}
    }
    if let Some(u) = q.user_id.as_deref().filter(|u| !u.is_empty()) {
        next("i.created_by = ?", Some(u.into()), &mut sql, &mut args);
    }
    if let Some(v) = q.variant_id.as_deref().filter(|v| !v.is_empty()) {
        let cond = if lines {
            "l.variant_id = ?"
        } else {
            "EXISTS (SELECT 1 FROM sales_invoice_items x WHERE x.invoice_id = i.id AND x.variant_id = ?)"
        };
        next(cond, Some(v.into()), &mut sql, &mut args);
    }
    if let Some(c) = q.category_id.as_deref().filter(|c| !c.is_empty()) {
        let cond = if lines {
            "l.variant_id IN (SELECT v.id FROM product_variants v JOIN products p ON p.id = v.product_id WHERE p.category_id = ?)"
        } else {
            "EXISTS (SELECT 1 FROM sales_invoice_items x JOIN product_variants v ON v.id = x.variant_id JOIN products p ON p.id = v.product_id
                     WHERE x.invoice_id = i.id AND p.category_id = ?)"
        };
        next(cond, Some(c.into()), &mut sql, &mut args);
    }
    (sql, args)
}

/// Every finalized invoice in the range, with the drill-down filters; each row opens the bill.
fn sales_invoices(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
) -> AppResult<ReportTable> {
    let (filters, extra) = invoice_filters(q, 3, false);
    let mut args: Vec<Sql> = vec![range.0.to_string().into(), range.1.to_string().into()];
    args.extend(extra);
    let raw = query_rows(
        conn,
        &format!(
            "SELECT i.id, i.business_date, i.doc_no, i.customer_name, u.name, i.taxable, i.tax_total, i.total, i.total - i.amount_paid
             FROM sales_invoices i LEFT JOIN users u ON u.id = i.created_by
             WHERE i.status = 'finalized' AND i.business_date BETWEEN ?1 AND ?2{filters}
             ORDER BY i.business_date, i.doc_no LIMIT 5001"
        ),
        &args,
        9,
    )?;
    let links = raw
        .iter()
        .map(|r| RowLink::doc("sales_invoice", &r[0]))
        .collect();
    let rows = raw.into_iter().map(|r| r[1..].to_vec()).collect();
    let table = finish(
        q,
        "Sales invoices",
        range,
        vec![
            col("date", "Date", "date"),
            col("docNo", "Invoice", "text"),
            col("customer", "Customer", "text"),
            col("user", "Cashier", "text"),
            col("taxable", "Taxable value", "money"),
            col("tax", "Tax", "money"),
            col("total", "Total", "money"),
            col("due", "Balance due", "money"),
        ],
        rows,
        &[4, 5, 6, 7],
    );
    Ok(grouped(table, "", links))
}

/// Sales summary grouped by customer, cashier, item or category; each row opens its invoices.
fn sales_summary_by(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    group: &str,
) -> AppResult<ReportTable> {
    let args: Vec<Sql> = vec![range.0.to_string().into(), range.1.to_string().into()];
    let (label, rows, links): (&str, Vec<Vec<Value>>, Vec<Option<RowLink>>) = match group {
        "customer" | "user" => {
            let (key, name, ret_key) = if group == "customer" {
                (
                    "coalesce(i.party_id, 'walk_in')",
                    "CASE WHEN i.party_id IS NULL THEN 'Walk-in customers' ELSE max(i.customer_name) END",
                    "coalesce(s.party_id, 'walk_in')",
                )
            } else {
                (
                    "i.created_by",
                    "coalesce(max(u.name), '(unknown)')",
                    "s.created_by",
                )
            };
            let sales = query_rows(
                conn,
                &format!(
                    "SELECT {key}, {name}, count(*), sum(i.taxable), sum(i.tax_total), sum(i.total)
                     FROM sales_invoices i LEFT JOIN users u ON u.id = i.created_by
                     WHERE i.status = 'finalized' AND i.business_date BETWEEN ?1 AND ?2
                     GROUP BY {key} ORDER BY sum(i.total) DESC"
                ),
                &args,
                6,
            )?;
            let returns: BTreeMap<String, i64> = query_rows(
                conn,
                &format!(
                    "SELECT {ret_key}, sum(s.total) FROM sales_returns s
                     WHERE s.status = 'finalized' AND s.business_date BETWEEN ?1 AND ?2 GROUP BY {ret_key}"
                ),
                &args,
                2,
            )?
            .into_iter()
            .filter_map(|r| Some((r[0].as_str()?.to_string(), r[1].as_i64().unwrap_or(0))))
            .collect();
            let mut links = Vec::new();
            let rows = sales
                .into_iter()
                .map(|r| {
                    let k = r[0].as_str().unwrap_or_default().to_string();
                    let total = r[5].as_i64().unwrap_or(0);
                    let back = returns.get(&k).copied().unwrap_or(0);
                    let mut link = RowLink::report("sales_invoices", range);
                    if group == "customer" {
                        link.party_id = Some(k);
                    } else {
                        link.user_id = Some(k);
                    }
                    links.push(Some(link));
                    vec![
                        r[1].clone(),
                        r[2].clone(),
                        r[3].clone(),
                        r[4].clone(),
                        json!(total),
                        json!(back),
                        json!(total - back),
                    ]
                })
                .collect();
            (
                if group == "customer" {
                    "Customer"
                } else {
                    "Cashier"
                },
                rows,
                links,
            )
        }
        _ => {
            // item or category: from the invoice lines (returns are not split by item here).
            let (key, name) = if group == "item" {
                ("l.variant_id", "max(l.item_name)")
            } else {
                (
                    "coalesce(p.category_id, '')",
                    "coalesce(max(c.name), 'No category')",
                )
            };
            let raw = query_rows(
                conn,
                &format!(
                    "SELECT {key}, {name}, count(DISTINCT i.id), sum(l.taxable), sum(l.tax_total), sum(l.line_total)
                     FROM sales_invoice_items l JOIN sales_invoices i ON i.id = l.invoice_id
                     JOIN product_variants v ON v.id = l.variant_id JOIN products p ON p.id = v.product_id
                     LEFT JOIN categories c ON c.id = p.category_id
                     WHERE i.status = 'finalized' AND i.business_date BETWEEN ?1 AND ?2
                     GROUP BY {key} ORDER BY sum(l.line_total) DESC"
                ),
                &args,
                6,
            )?;
            let mut links = Vec::new();
            let rows = raw
                .into_iter()
                .map(|r| {
                    let k = r[0].as_str().unwrap_or_default().to_string();
                    let mut link = RowLink::report("sales_invoices", range);
                    if group == "item" {
                        link.variant_id = Some(k);
                    } else {
                        link.category_id = Some(k).filter(|c| !c.is_empty());
                    }
                    links.push(Some(link));
                    let total = r[5].as_i64().unwrap_or(0);
                    vec![
                        r[1].clone(),
                        r[2].clone(),
                        r[3].clone(),
                        r[4].clone(),
                        json!(total),
                        Value::Null,
                        json!(total),
                    ]
                })
                .collect();
            (
                if group == "item" { "Item" } else { "Category" },
                rows,
                links,
            )
        }
    };
    let table = finish(
        q,
        "Sales summary",
        range,
        vec![
            col("group", label, "text"),
            col("bills", "Bills", "count"),
            col("taxable", "Taxable value", "money"),
            col("tax", "Tax", "money"),
            col("total", "Sales", "money"),
            col("returns", "Returns", "money"),
            col("net", "Net sales", "money"),
        ],
        rows,
        &[2, 3, 4, 5, 6],
    );
    Ok(grouped(table, group, links))
}

/// The report's grouping: the one asked for (checked) or its default.
fn group_of(q: &ReportQuery) -> AppResult<String> {
    let options = group_options(&q.report);
    match q.group_by.as_deref().filter(|g| !g.is_empty()) {
        None => Ok(options.first().map_or("", |o| o.0).to_string()),
        Some(g) if options.iter().any(|o| o.0 == g) => Ok(g.to_string()),
        Some(_) => Err(AppError::field(
            "groupBy",
            "This report can't be grouped that way.",
        )),
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
    let settings = super::settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    run_on(ctx.conn, &settings, &today, q)
}

/// Runs a report (no permission check: `run` and the scheduled exports check first).
pub fn run_on(
    conn: &Connection,
    settings: &super::settings::Settings,
    today: &str,
    q: &ReportQuery,
) -> AppResult<ReportTable> {
    if !REPORTS.contains(&q.report.as_str()) {
        return Err(AppError::field("report", "Unknown report."));
    }
    let today = today.to_string();
    let group = group_of(q)?;
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
    let range = (from.as_str(), to.as_str());
    let dates: Vec<Sql> = vec![from.clone().into(), to.clone().into()];
    match q.report.as_str() {
        "sales_summary" if matches!(group.as_str(), "day" | "month") => {
            sales_summary(conn, q, range, &group)
        }
        "sales_summary" => sales_summary_by(conn, q, range, &group),
        "sales_invoices" => sales_invoices(conn, q, range),
        "sales_by_item" => sales_by_item(conn, q, range, &dates, &group),
        "purchases" => purchases(conn, q, range, &dates, &group),
        "stock_valuation" => stock_valuation(conn, q, range),
        "expiry" => expiry(conn, q, range, settings, &today),
        "stock_movement" => stock_movement(conn, q, range, &dates),
        "party_balances" => party_balances(conn, q, range),
        "tax_summary" => tax_summary(conn, q, range, &dates),
        "day_book" => day_book(conn, q, range, &dates),
        "profit_loss" => profit_loss(conn, q, range, &dates),
        "trial_balance" => trial_balance(conn, q, range),
        "cash_book" | "bank_book" => books(conn, q, range),
        "expenses" => expenses(conn, q, range, &dates, &group),
        _ => unreachable!("checked above"),
    }
}

fn sales_summary(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    group: &str,
) -> AppResult<ReportTable> {
    let args: Vec<Sql> = vec![range.0.to_string().into(), range.1.to_string().into()];
    let period = if group == "month" {
        "substr(business_date, 1, 7)"
    } else {
        "business_date"
    };
    // day → [bills, taxable, tax, round_off, total, returns]
    let mut days: BTreeMap<String, [i64; 6]> = BTreeMap::new();
    for r in query_rows(
        conn,
        &format!(
            "SELECT {period}, count(*), sum(taxable), sum(tax_total), sum(round_off), sum(total)
             FROM sales_invoices WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2 GROUP BY {period}"
        ),
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
        &format!(
            "SELECT {period}, sum(total) FROM sales_returns WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2 GROUP BY {period}"
        ),
        &args,
        2,
    )? {
        days.entry(r[0].as_str().unwrap_or_default().into()).or_default()[5] = r[1].as_i64().unwrap_or(0);
    }
    let links = days
        .keys()
        .map(|key| {
            let (from, to) = if group == "month" {
                month_range(key, range)
            } else {
                (key.clone(), key.clone())
            };
            Some(RowLink::report("sales_invoices", (&from, &to)))
        })
        .collect();
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
    let table = finish(
        q,
        "Sales summary",
        range,
        vec![
            if group == "month" {
                col("month", "Month", "text")
            } else {
                col("date", "Date", "date")
            },
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
    );
    Ok(grouped(table, group, links))
}

/// Item profit: what each item sold for (after discounts, before tax), what it cost, and the
/// difference. Returns of restocked items take their sale and cost back.
fn sales_by_item(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
    group: &str,
) -> AppResult<ReportTable> {
    let by_category = group == "category";
    let mut args = dates.to_vec();
    let category_filter = match q.category_id.as_deref().filter(|c| !c.is_empty()) {
        Some(c) => {
            args.push(c.to_string().into());
            " WHERE category = ?3"
        }
        None => "",
    };
    let (key, name, code) = if by_category {
        (
            "category",
            "coalesce(max(category_name), 'No category')",
            "''",
        )
    } else {
        ("variant", "max(item)", "max(sku)")
    };
    let raw = query_rows(
        conn,
        &format!(
            "SELECT {key}, {name}, {code}, sum(qty), sum(revenue), sum(cost) FROM (
            SELECT l.variant_id AS variant, coalesce(p.category_id, '') AS category, c.name AS category_name,
                   l.item_name AS item, l.sku AS sku, l.base_qty AS qty, l.taxable AS revenue,
                   coalesce(l.cost_value, (l.unit_cost * l.base_qty + 500) / 1000) AS cost
            FROM sales_invoice_items l JOIN sales_invoices i ON i.id = l.invoice_id
            JOIN product_variants v ON v.id = l.variant_id JOIN products p ON p.id = v.product_id
            LEFT JOIN categories c ON c.id = p.category_id
            WHERE i.status = 'finalized' AND i.business_date BETWEEN ?1 AND ?2
            UNION ALL
            SELECT r.variant_id, coalesce(p.category_id, ''), c.name, r.item_name, r.sku, -r.base_qty, -r.taxable,
                   -CASE WHEN r.restock = 1 THEN (r.unit_cost * r.base_qty + 500) / 1000 ELSE 0 END
            FROM sales_return_items r JOIN sales_returns s ON s.id = r.return_id
            JOIN product_variants v ON v.id = r.variant_id JOIN products p ON p.id = v.product_id
            LEFT JOIN categories c ON c.id = p.category_id
            WHERE s.status = 'finalized' AND s.business_date BETWEEN ?1 AND ?2
         ){category_filter} GROUP BY {key} ORDER BY sum(revenue) DESC"
        ),
        &args,
        6,
    )?;
    let links: Vec<Option<RowLink>> = raw
        .iter()
        .map(|r| {
            let k = r[0].as_str().unwrap_or_default().to_string();
            if by_category {
                let mut link = RowLink::report("sales_by_item", range);
                link.group_by = Some("item".into());
                link.category_id = Some(k);
                Some(link)
            } else {
                let mut link = RowLink::report("sales_invoices", range);
                link.variant_id = Some(k);
                Some(link)
            }
        })
        .collect();
    let raw: Vec<Vec<Value>> = raw.into_iter().map(|r| r[1..].to_vec()).collect();
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
    let method = super::stock_posting::valuation(conn)?;
    let mut table = finish(
        q,
        match method {
            super::stock_posting::Valuation::Fifo => "Item profit (FIFO cost)",
            super::stock_posting::Valuation::WeightedAverage => "Item profit (average cost)",
        },
        range,
        vec![
            if by_category {
                col("category", "Category", "text")
            } else {
                col("item", "Item", "text")
            },
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
    Ok(grouped(table, group, links))
}

fn purchases(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    dates: &[Sql],
    group: &str,
) -> AppResult<ReportTable> {
    if group != "bill" {
        let (key, name, label) = if group == "supplier" {
            ("party_id", "max(supplier_name)", "Supplier")
        } else {
            (
                "substr(business_date, 1, 7)",
                "substr(business_date, 1, 7)",
                "Month",
            )
        };
        let raw = query_rows(
            conn,
            &format!(
                "SELECT {key}, {name}, count(*), sum(taxable), sum(tax_total), sum(total), sum(amount_paid)
                 FROM purchase_bills WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2
                 GROUP BY {key} ORDER BY {}",
                if group == "supplier" { "sum(total) DESC" } else { "1" }
            ),
            dates,
            7,
        )?;
        let links = raw
            .iter()
            .map(|r| {
                let k = r[0].as_str().unwrap_or_default();
                let mut link = if group == "supplier" {
                    let mut l = RowLink::report("purchases", range);
                    l.party_id = Some(k.into());
                    l
                } else {
                    let (from, to) = month_range(k, range);
                    RowLink::report("purchases", (&from, &to))
                };
                link.group_by = Some("bill".into());
                Some(link)
            })
            .collect();
        let rows = raw.into_iter().map(|r| r[1..].to_vec()).collect();
        let table = finish(
            q,
            "Purchases",
            range,
            vec![
                col("group", label, "text"),
                col("bills", "Bills", "count"),
                col("taxable", "Taxable value", "money"),
                col("tax", "Tax", "money"),
                col("total", "Total", "money"),
                col("paid", "Paid", "money"),
            ],
            rows,
            &[1, 2, 3, 4, 5],
        );
        return Ok(grouped(table, group, links));
    }
    let mut args = dates.to_vec();
    let party = match q.party_id.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => {
            args.push(p.to_string().into());
            " AND party_id = ?3"
        }
        None => "",
    };
    let raw = query_rows(
        conn,
        &format!(
            "SELECT id, business_date, doc_no, supplier_name, supplier_invoice_no, taxable, tax_total, total, amount_paid
             FROM purchase_bills WHERE status = 'finalized' AND business_date BETWEEN ?1 AND ?2{party}
             ORDER BY business_date, doc_no"
        ),
        &args,
        9,
    )?;
    let links = raw
        .iter()
        .map(|r| RowLink::doc("purchase_bill", &r[0]))
        .collect();
    let rows = raw.into_iter().map(|r| r[1..].to_vec()).collect();
    let table = finish(
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
    );
    Ok(grouped(table, group, links))
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
    // Under FIFO the value is what the open cost layers hold (always equal to the ledger); the
    // oldest layer shows how long the stock has been on the shelf.
    let fifo = super::stock_posting::valuation(conn)? == super::stock_posting::Valuation::Fifo;
    let layer_filter = if q.warehouse_id.is_some() {
        "AND cl.warehouse_id = ?1"
    } else {
        ""
    };
    let rows = query_rows(
        conn,
        &format!(
            "SELECT p.name || coalesce(' — ' || v.variant_name, ''), v.sku, coalesce(c.name, ''), sum(l.qty), sum(l.value),
                    (SELECT min(cl.business_date) FROM cost_layers cl WHERE cl.variant_id = v.id AND cl.qty_left > 0 {layer_filter})
             FROM stock_levels l JOIN product_variants v ON v.id = l.variant_id JOIN products p ON p.id = v.product_id
             LEFT JOIN categories c ON c.id = p.category_id
             WHERE p.is_service = 0 {filter} GROUP BY v.id HAVING sum(l.qty) <> 0 OR sum(l.value) <> 0
             ORDER BY lower(p.name)"
        ),
        &args,
        6,
    )?;
    let rows = rows
        .into_iter()
        .map(|r| {
            let (qty, value) = (r[3].as_i64().unwrap_or(0), r[4].as_i64().unwrap_or(0));
            let avg = if qty > 0 { value * 1000 / qty } else { 0 };
            let mut row = vec![
                r[0].clone(),
                r[1].clone(),
                r[2].clone(),
                json!(qty),
                json!(avg),
                json!(value),
            ];
            if fifo {
                row.push(r[5].clone());
            }
            row
        })
        .collect();
    let mut columns = vec![
        col("item", "Item", "text"),
        col("sku", "SKU", "text"),
        col("category", "Category", "text"),
        col("qty", "In stock", "qty"),
        col(
            "avgCost",
            if fifo { "Unit cost" } else { "Average cost" },
            "money",
        ),
        col("value", "Value", "money"),
    ];
    if fifo {
        columns.push(col("oldest", "Oldest stock from", "date"));
    }
    let mut table = finish(
        q,
        if fifo {
            "Stock valuation (FIFO)"
        } else {
            "Stock valuation (weighted average cost)"
        },
        range,
        columns,
        rows,
        &[5],
    );
    table.from = range.1.into();
    Ok(table)
}

/// Batches that have expired or expire within the alert window, with what they're worth.
fn expiry(
    conn: &Connection,
    q: &ReportQuery,
    range: (&str, &str),
    settings: &super::settings::Settings,
    today: &str,
) -> AppResult<ReportTable> {
    let horizon = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map(|d| {
            (d + chrono::Duration::days(settings.inventory.expiry_alert_days))
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| today.to_string());
    let mut args: Vec<Sql> = vec![horizon.into(), today.to_string().into()];
    let filter = match &q.warehouse_id {
        Some(w) => {
            args.push(w.clone().into());
            "AND l.warehouse_id = ?3"
        }
        None => "",
    };
    let rows = query_rows(
        conn,
        &format!(
            "SELECT p.name || coalesce(' — ' || v.variant_name, ''), v.sku, b.batch_no, b.expiry_date,
                    CAST(julianday(b.expiry_date) - julianday(?2) AS INTEGER), w.name, l.qty, l.value,
                    CASE WHEN b.expiry_date < ?2 THEN 'Expired' ELSE 'Near expiry' END
             FROM stock_levels l JOIN batches b ON b.id = l.batch_id JOIN product_variants v ON v.id = l.variant_id
             JOIN products p ON p.id = v.product_id JOIN warehouses w ON w.id = l.warehouse_id
             WHERE l.qty > 0 AND b.expiry_date IS NOT NULL AND b.expiry_date <= ?1 {filter}
             ORDER BY b.expiry_date, lower(p.name)"
        ),
        &args,
        9,
    )?;
    let mut table = finish(
        q,
        &format!(
            "Expired and near-expiry stock (next {} days)",
            settings.inventory.expiry_alert_days
        ),
        range,
        vec![
            col("item", "Item", "text"),
            col("sku", "SKU", "text"),
            col("batch", "Batch", "text"),
            col("expiry", "Expires", "date"),
            col("daysLeft", "Days left", "count"),
            col("store", "Store", "text"),
            col("qty", "In stock", "qty"),
            col("value", "Value", "money"),
            col("status", "Status", "text"),
        ],
        rows,
        &[6, 7],
    );
    table.from = today.into();
    table.to = today.into();
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
    let raw = query_rows(
        conn,
        "SELECT e.source_type, e.source_id, e.business_date,
                CASE e.source_type WHEN 'manual_journal' THEN 'manual journal' WHEN 'year_close' THEN 'year close'
                  ELSE replace(e.source_type, '_', ' ') END,
                e.narration, sum(l.debit),
                CASE WHEN e.reversal_of IS NOT NULL THEN 'reversal' WHEN e.status = 'reversed' THEN 'reversed' ELSE 'posted' END
         FROM journal_entries e JOIN journal_lines l ON l.entry_id = e.id
         WHERE e.business_date BETWEEN ?1 AND ?2 GROUP BY e.id ORDER BY e.business_date, e.created_at LIMIT 5001",
        dates,
        7,
    )?;
    let links = raw
        .iter()
        .map(|r| match r[0].as_str() {
            Some(
                kind @ ("sales_invoice" | "sales_return" | "purchase_bill" | "goods_receipt"
                | "purchase_return" | "payment" | "manual_journal"),
            ) => RowLink::doc(kind, &r[1]),
            _ => None,
        })
        .collect();
    let rows = raw.into_iter().map(|r| r[2..].to_vec()).collect();
    let table = finish(
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
    );
    Ok(grouped(table, "", links))
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
             WHERE e.business_date BETWEEN ?1 AND ?2 AND a.account_type = ?3 AND e.source_type <> 'year_close'
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
    group: &str,
) -> AppResult<ReportTable> {
    let by_month = group == "month";
    let mut args = dates.to_vec();
    let category = match q.category_id.as_deref().filter(|c| !c.is_empty()) {
        Some(c) => {
            args.push(c.to_string().into());
            " AND e.category_id = ?3"
        }
        None => "",
    };
    let (key, name, order) = if by_month {
        (
            "substr(e.business_date, 1, 7)",
            "substr(e.business_date, 1, 7)",
            "1",
        )
    } else {
        ("c.id", "c.name", "sum(e.total) DESC")
    };
    let raw = query_rows(
        conn,
        &format!(
            "SELECT {key}, {name}, count(*), sum(e.amount), sum(e.tax_total), sum(e.total) FROM expenses e
             JOIN expense_categories c ON c.id = e.category_id
             WHERE e.status = 'finalized' AND e.business_date BETWEEN ?1 AND ?2{category} GROUP BY {key} ORDER BY {order}"
        ),
        &args,
        6,
    )?;
    let links = raw
        .iter()
        .map(|r| {
            let k = r[0].as_str().unwrap_or_default();
            if by_month {
                let (from, to) = month_range(k, range);
                let mut l = RowLink::report("expenses", (&from, &to));
                l.group_by = Some("category".into());
                Some(l)
            } else {
                let mut l = RowLink::report("expenses", range);
                l.group_by = Some("month".into());
                l.category_id = Some(k.into());
                Some(l)
            }
        })
        .collect();
    let rows = raw.into_iter().map(|r| r[1..].to_vec()).collect();
    let table = finish(
        q,
        if by_month {
            "Expenses by month"
        } else {
            "Expenses by category"
        },
        range,
        vec![
            if by_month {
                col("month", "Month", "text")
            } else {
                col("category", "Category", "text")
            },
            col("count", "Entries", "count"),
            col("amount", "Amount", "money"),
            col("tax", "Tax", "money"),
            col("total", "Total", "money"),
        ],
        rows,
        &[1, 2, 3, 4],
    );
    Ok(grouped(table, group, links))
}
