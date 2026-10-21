//! Lists and look-ups the screens need beyond each module's own commands: stock documents
//! (adjustments, transfers, counts), sales returns, goods receipts and purchase returns, plus
//! store (warehouse) maintenance.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::{date, like_pattern, required, Page, PageQuery};
use super::purchases::{GoodsReceiptDetail, PurchaseDocDetail, PurchaseDocLine};
use super::sales::ReturnDetail;
use super::stock::{StockDocLine, StockDocResult, Warehouse};
use super::users::IdInput;
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

/// Shared filters: date range and a search over the document number (and a name, where the
/// document has one).
struct Filters {
    sql: Vec<String>,
    args: Vec<SqlValue>,
}

impl Filters {
    fn new(base: &str) -> Self {
        Self {
            sql: vec![base.to_string()],
            args: Vec::new(),
        }
    }

    fn dates(&mut self, column: &str, from: Option<&str>, to: Option<&str>) -> AppResult<()> {
        if let Some(from) = from {
            self.sql.push(format!("{column} >= ?"));
            self.args.push(date("from", from)?.into());
        }
        if let Some(to) = to {
            self.sql.push(format!("{column} <= ?"));
            self.args.push(date("to", to)?.into());
        }
        Ok(())
    }

    fn search(&mut self, columns: &[&str], term: Option<&str>) {
        if let Some(term) = term.map(str::trim).filter(|t| !t.is_empty()) {
            let parts: Vec<String> = columns
                .iter()
                .map(|c| format!("lower(coalesce({c}, '')) LIKE lower(?) ESCAPE '\\'"))
                .collect();
            self.sql.push(format!("({})", parts.join(" OR ")));
            for _ in columns {
                self.args.push(like_pattern(term).into());
            }
        }
    }

    fn eq(&mut self, column: &str, value: Option<&str>) {
        if let Some(v) = value {
            self.sql.push(format!("{column} = ?"));
            self.args.push(v.to_string().into());
        }
    }

    fn where_sql(&self) -> String {
        self.sql.join(" AND ")
    }
}

// ------------------------------------------------------------------ stock documents

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockDocQuery {
    /// "adjustment", "transfer" or "count".
    pub kind: String,
    pub search: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockDocListItem {
    pub id: String,
    pub kind: String,
    pub doc_no: Option<String>,
    pub business_date: String,
    pub status: String,
    pub warehouse_name: String,
    /// Transfers: the receiving store.
    pub to_warehouse_name: Option<String>,
    /// Adjustment reason or count scope.
    pub note: Option<String>,
    pub line_count: i64,
}

pub fn list_stock_documents(
    ctx: &Ctx<'_>,
    query: &StockDocQuery,
) -> AppResult<Page<StockDocListItem>> {
    ctx.require("stock.view")?;
    let (from_sql, select, date_col) = match query.kind.as_str() {
        "adjustment" => (
            "stock_adjustments d JOIN warehouses w ON w.id = d.warehouse_id",
            "d.id, 'adjustment', d.doc_no, d.business_date, d.status, w.name, NULL, d.reason,
             (SELECT count(*) FROM stock_adjustment_items i WHERE i.adjustment_id = d.id)",
            "d.business_date",
        ),
        "transfer" => (
            "stock_transfers d JOIN warehouses w ON w.id = d.from_warehouse_id JOIN warehouses w2 ON w2.id = d.to_warehouse_id",
            "d.id, 'transfer', d.doc_no, d.business_date, d.status, w.name, w2.name, d.notes,
             (SELECT count(*) FROM stock_transfer_items i WHERE i.transfer_id = d.id)",
            "d.business_date",
        ),
        "count" => (
            "stock_counts d JOIN warehouses w ON w.id = d.warehouse_id",
            "d.id, 'count', d.doc_no, d.business_date, d.status, w.name, NULL, d.scope_note,
             (SELECT count(*) FROM stock_count_lines i WHERE i.count_id = d.id)",
            "d.business_date",
        ),
        _ => return Err(AppError::field("kind", "Use adjustment, transfer or count.")),
    };
    let mut f = Filters::new("d.deleted_at IS NULL");
    f.dates(date_col, query.from.as_deref(), query.to.as_deref())?;
    f.search(&["d.doc_no"], query.search.as_deref());
    let where_sql = f.where_sql();
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM {from_sql} WHERE {where_sql}"),
        params_from_iter(f.args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("docNo", "coalesce(d.doc_no, '')"),
            ("date", date_col),
            ("status", "d.status"),
            ("store", "lower(w.name)"),
        ],
        &format!("{date_col} DESC, d.created_at DESC, d.id"),
        "d.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    let mut args = f.args;
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT {select} FROM {from_sql} WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(StockDocListItem {
                id: r.get(0)?,
                kind: r.get(1)?,
                doc_no: r.get(2)?,
                business_date: r.get(3)?,
                status: r.get(4)?,
                warehouse_name: r.get(5)?,
                to_warehouse_name: r.get(6)?,
                note: r.get(7)?,
                line_count: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockDocRef {
    /// "adjustment" or "transfer" (counts: `stock_count_get`).
    pub kind: String,
    pub id: String,
}

pub fn get_stock_document(ctx: &Ctx<'_>, input: &StockDocRef) -> AppResult<StockDocResult> {
    ctx.require("stock.view")?;
    let (header, lines) = match input.kind.as_str() {
        "adjustment" => (
            "SELECT doc_no, business_date FROM stock_adjustments WHERE id = ?1 AND status <> 'draft'",
            "SELECT i.variant_id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, i.qty_change, i.unit_cost
             FROM stock_adjustment_items i JOIN product_variants v ON v.id = i.variant_id JOIN products p ON p.id = v.product_id
             WHERE i.adjustment_id = ?1 ORDER BY i.line_no",
        ),
        "transfer" => (
            "SELECT doc_no, business_date FROM stock_transfers WHERE id = ?1 AND status <> 'draft'",
            "SELECT i.variant_id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, i.qty, i.unit_cost
             FROM stock_transfer_items i JOIN product_variants v ON v.id = i.variant_id JOIN products p ON p.id = v.product_id
             WHERE i.transfer_id = ?1 ORDER BY i.line_no",
        ),
        _ => return Err(AppError::field("kind", "Use adjustment or transfer.")),
    };
    let (doc_no, business_date): (String, String) = ctx
        .conn
        .query_row(header, [&input.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("The stock document"))?;
    let lines = ctx
        .conn
        .prepare(lines)?
        .query_map([&input.id], |r| {
            let qty: i64 = r.get(3)?;
            let unit_cost: i64 = r.get(4)?;
            Ok(StockDocLine {
                variant_id: r.get(0)?,
                name: r.get(1)?,
                sku: r.get(2)?,
                qty,
                unit_cost,
                value: crate::domain::money::multiply_by_qty(
                    unit_cost,
                    qty,
                    crate::domain::money::RoundingMode::HalfUp,
                ),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(StockDocResult {
        id: input.id.clone(),
        doc_no,
        business_date,
        lines,
        warnings: Vec::new(),
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarehouseInput {
    pub id: Option<String>,
    pub code: String,
    pub name: String,
}

/// Adds or renames a store (stock location) of the main branch.
pub fn save_warehouse(ctx: &mut Ctx<'_>, input: &WarehouseInput) -> AppResult<Warehouse> {
    ctx.require("settings.manage")?;
    let code = required("code", &input.code, 20)?.to_uppercase();
    let name = required("name", &input.name, 80)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let id = match &input.id {
            Some(id) => {
                let changed = tx.execute(
                    "UPDATE warehouses SET code = ?1, name = ?2, updated_at = ?3, updated_by = ?4 WHERE id = ?5 AND deleted_at IS NULL",
                    params![code, name, now, actor.user_id, id],
                )?;
                if changed == 0 {
                    return Err(AppError::not_found("Store"));
                }
                id.clone()
            }
            None => {
                let (branch_id, _) = super::setup::default_location(tx)?;
                let id = new_id();
                tx.execute(
                    "INSERT INTO warehouses (id, branch_id, code, name, is_default, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5, ?6, ?6)",
                    params![id, branch_id, code, name, now, actor.user_id],
                )?;
                id
            }
        };
        AuditEntry::new("WAREHOUSE_SAVE", "warehouses", Some(&id))
            .by(actor)
            .summary(format!("{code} {name}"))
            .write(tx, &now)?;
        Ok(tx.query_row(
            "SELECT id, branch_id, code, name, is_default FROM warehouses WHERE id = ?1",
            [&id],
            |r| {
                Ok(Warehouse {
                    id: r.get(0)?,
                    branch_id: r.get(1)?,
                    code: r.get(2)?,
                    name: r.get(3)?,
                    is_default: r.get(4)?,
                })
            },
        )?)
    })
}

// ------------------------------------------------------------------ sales returns

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnListQuery {
    pub search: Option<String>,
    pub party_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnListItem {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub status: String,
    pub customer_name: String,
    pub invoice_doc_no: Option<String>,
    pub total: i64,
    pub amount_refunded: i64,
}

pub fn list_sales_returns(
    ctx: &Ctx<'_>,
    query: &ReturnListQuery,
) -> AppResult<Page<ReturnListItem>> {
    ctx.require("sales.view")?;
    let mut f = Filters::new("r.status <> 'draft' AND r.deleted_at IS NULL");
    f.eq("r.party_id", query.party_id.as_deref());
    f.dates(
        "r.business_date",
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    f.search(
        &["r.doc_no", "r.customer_name", "i.doc_no"],
        query.search.as_deref(),
    );
    let where_sql = f.where_sql();
    let from_sql = "sales_returns r LEFT JOIN sales_invoices i ON i.id = r.original_invoice_id";
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM {from_sql} WHERE {where_sql}"),
        params_from_iter(f.args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("docNo", "r.doc_no"),
            ("date", "r.business_date"),
            ("status", "r.status"),
            ("customer", "lower(r.customer_name)"),
            ("invoice", "coalesce(i.doc_no, '')"),
            ("total", "r.total"),
            ("refunded", "r.amount_refunded"),
        ],
        "r.business_date DESC, r.created_at DESC, r.id",
        "r.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    let mut args = f.args;
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT r.id, r.doc_no, r.business_date, r.status, r.customer_name, i.doc_no, r.total, r.amount_refunded
             FROM {from_sql} WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(ReturnListItem {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                status: r.get(3)?,
                customer_name: r.get(4)?,
                invoice_doc_no: r.get(5)?,
                total: r.get(6)?,
                amount_refunded: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

/// The returns already made against one invoice (for the return screen).
pub fn returns_of_invoice(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<Vec<ReturnDetail>> {
    ctx.require("sales.view")?;
    let ids: Vec<String> = ctx
        .conn
        .prepare("SELECT id FROM sales_returns WHERE original_invoice_id = ?1 AND status <> 'draft' ORDER BY created_at")?
        .query_map([&input.id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    ids.iter()
        .map(|id| super::sales::return_detail(ctx.conn, id))
        .collect()
}

// ------------------------------------------------------------------ goods receipts, purchase returns

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptListQuery {
    pub search: Option<String>,
    pub party_id: Option<String>,
    pub purchase_order_id: Option<String>,
    /// Only receipts not yet covered by a supplier bill.
    #[serde(default)]
    pub unbilled_only: bool,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptListItem {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub status: String,
    pub party_id: String,
    pub supplier_name: String,
    pub supplier_reference: Option<String>,
    pub order_doc_no: Option<String>,
    pub bill_doc_no: Option<String>,
    pub value: i64,
}

pub fn list_goods_receipts(
    ctx: &Ctx<'_>,
    query: &ReceiptListQuery,
) -> AppResult<Page<ReceiptListItem>> {
    ctx.require("purchases.view")?;
    let mut f = Filters::new("g.status <> 'draft' AND g.deleted_at IS NULL");
    f.eq("g.party_id", query.party_id.as_deref());
    f.eq("g.purchase_order_id", query.purchase_order_id.as_deref());
    if query.unbilled_only {
        f.sql.push(
            "NOT EXISTS (SELECT 1 FROM purchase_bills b WHERE b.goods_receipt_id = g.id AND b.status <> 'voided')".into(),
        );
    }
    f.dates(
        "g.business_date",
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    f.search(
        &["g.doc_no", "p.name", "g.supplier_reference"],
        query.search.as_deref(),
    );
    let where_sql = f.where_sql();
    let from_sql = "goods_receipts g JOIN parties p ON p.id = g.party_id LEFT JOIN purchase_orders o ON o.id = g.purchase_order_id";
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM {from_sql} WHERE {where_sql}"),
        params_from_iter(f.args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("docNo", "g.doc_no"),
            ("date", "g.business_date"),
            ("status", "g.status"),
            ("supplier", "lower(p.name)"),
            ("reference", "coalesce(g.supplier_reference, '')"),
            ("order", "coalesce(o.doc_no, '')"),
        ],
        "g.business_date DESC, g.created_at DESC, g.id",
        "g.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    let mut args = f.args;
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT g.id, g.doc_no, g.business_date, g.status, g.party_id, p.name, g.supplier_reference, o.doc_no,
                (SELECT b.doc_no FROM purchase_bills b WHERE b.goods_receipt_id = g.id AND b.status <> 'voided' LIMIT 1),
                (SELECT coalesce(sum(value), 0) FROM stock_movements m WHERE m.source_type = 'goods_receipt' AND m.source_id = g.id)
             FROM {from_sql} WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(ReceiptListItem {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                status: r.get(3)?,
                party_id: r.get(4)?,
                supplier_name: r.get(5)?,
                supplier_reference: r.get(6)?,
                order_doc_no: r.get(7)?,
                bill_doc_no: r.get(8)?,
                value: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

pub fn get_goods_receipt(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<GoodsReceiptDetail> {
    ctx.require("purchases.view")?;
    super::purchases::receipt_detail(ctx.conn, &input.id)
}

pub fn get_purchase_return(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.view")?;
    let mut doc = ctx
        .conn
        .query_row(
            "SELECT id, doc_no, business_date, status, party_id, supplier_name, taxable, tax_total, round_off, total,
                amount_refunded, reason
             FROM purchase_returns WHERE id = ?1 AND status <> 'draft' AND deleted_at IS NULL",
            [&input.id],
            |r| {
                Ok(PurchaseDocDetail {
                    id: r.get(0)?,
                    kind: "purchase_return".into(),
                    doc_no: r.get(1)?,
                    business_date: r.get(2)?,
                    status: r.get(3)?,
                    party_id: r.get(4)?,
                    supplier_name: r.get(5)?,
                    supplier_invoice_no: None,
                    taxable: r.get(6)?,
                    tax_total: r.get(7)?,
                    charges_total: 0,
                    round_off: r.get(8)?,
                    total: r.get(9)?,
                    amount_paid: r.get(10)?,
                    notes: r.get(11)?,
                    lines: Vec::new(),
                    warnings: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The purchase return"))?;
    doc.lines = ctx
        .conn
        .prepare(
            "SELECT id, variant_id, item_name, qty, unit_cost, taxable, tax_total, line_total, 0, unit_id
             FROM purchase_return_items WHERE return_id = ?1 ORDER BY line_no",
        )?
        .query_map([&input.id], |r| {
            Ok(PurchaseDocLine {
                id: r.get(0)?,
                variant_id: r.get(1)?,
                item_name: r.get(2)?,
                qty: r.get(3)?,
                unit_cost: r.get(4)?,
                taxable: r.get(5)?,
                tax_total: r.get(6)?,
                line_total: r.get(7)?,
                progress_qty: r.get(8)?,
                unit_id: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(doc)
}
