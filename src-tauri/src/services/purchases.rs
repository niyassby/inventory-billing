//! Purchasing: orders, goods receipts (stock in), supplier bills (the payable, with landed costs)
//! and purchase returns / debit notes.
//!
//! Inventory accounting: a goods receipt debits Inventory and credits "Goods received not
//! invoiced"; the bill then clears that account (or debits Inventory directly for a bill that
//! receives the goods itself). The Inventory account therefore always equals stock value.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::common::{date, like_pattern, optional, required, Page, PageQuery};
use super::idempotency;
use super::journal::{self, Entry, JLine};
use super::numbering;
use super::party_ledger::{self, PartyRef, Posting};
use super::sales::{
    branch_of, record_payment, resolve_line, tax_columns, tax_components, validate_payments,
    void_payment, PaymentInput, ResolvedLine,
};
use super::settings::{self, Settings};
use super::stock_posting::{self, Move, StockWarning};
use super::users::IdInput;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::domain::pricing::{self, Discount, DocumentInput, DocumentResult, LineInput};
use crate::error::{AppError, AppResult, ErrorCode};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseLineInput {
    pub variant_id: String,
    pub unit_id: Option<String>,
    /// Thousandths of the purchase unit.
    pub qty: i64,
    /// Cost per purchase unit (before tax unless the bill's prices include tax).
    pub unit_cost: i64,
    pub discount: Option<Discount>,
    pub batch_no: Option<String>,
    pub expiry_date: Option<String>,
    pub mfg_date: Option<String>,
    /// Purchase-order line being fulfilled.
    pub po_item_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChargeInput {
    pub description: String,
    pub amount: i64,
    /// "value" or "qty" (added to stock cost), or "none" (expensed).
    pub allocation: String,
}

struct PricedPurchase {
    lines: Vec<ResolvedLine>,
    result: DocumentResult,
    inter_state: bool,
    prices_include_tax: bool,
}

fn price_purchase(
    conn: &Connection,
    settings: &Settings,
    lines: &[PurchaseLineInput],
    bill_discount: Option<&Discount>,
    include: bool,
    supplier_state: Option<&str>,
    own_state: Option<&str>,
) -> AppResult<PricedPurchase> {
    if lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one item."));
    }
    let inter_state = matches!((supplier_state, own_state), (Some(a), Some(b)) if a != b);
    let mut resolved = Vec::new();
    let mut inputs = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.unit_cost < 0 {
            return Err(AppError::field(
                &format!("lines.{index}.unitCost"),
                "Cost can't be negative.",
            ));
        }
        let mut r = resolve_line(
            conn,
            index,
            &line.variant_id,
            line.unit_id.as_deref(),
            line.qty,
            None,
        )?;
        r.unit_price = line.unit_cost;
        inputs.push(LineInput {
            unit_price: line.unit_cost,
            qty: r.qty,
            discount: line.discount.clone(),
            taxes: tax_components(
                settings.tax.regime,
                r.tax_type.as_deref(),
                r.tax_rate,
                r.cess_rate,
                inter_state,
            )?,
        });
        resolved.push(r);
    }
    let result = pricing::price_document(&DocumentInput {
        lines: inputs,
        prices_include_tax: include,
        bill_discount: bill_discount.cloned(),
        round_total_to: None,
        rounding: Some(settings.tax.rounding.into()),
    })
    .map_err(|e| AppError::validation(e.0))?;
    Ok(PricedPurchase {
        lines: resolved,
        result,
        inter_state,
        prices_include_tax: include,
    })
}

/// Finds or creates the batch for a received line.
fn batch_for(
    tx: &Connection,
    variant_id: &str,
    line: &PurchaseLineInput,
    index: usize,
    now: &str,
    user: &str,
) -> AppResult<Option<String>> {
    let Some(batch_no) = optional(
        &format!("lines.{index}.batchNo"),
        line.batch_no.as_deref(),
        40,
    )?
    else {
        return Ok(None);
    };
    let expiry = line
        .expiry_date
        .as_deref()
        .map(|d| date(&format!("lines.{index}.expiryDate"), d))
        .transpose()?;
    let mfg = line
        .mfg_date
        .as_deref()
        .map(|d| date(&format!("lines.{index}.mfgDate"), d))
        .transpose()?;
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM batches WHERE variant_id = ?1 AND lower(batch_no) = lower(?2) AND deleted_at IS NULL",
            params![variant_id, batch_no],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(Some(id));
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO batches (id, variant_id, batch_no, mfg_date, expiry_date, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7)",
        params![id, variant_id, batch_no, mfg, expiry, now, user],
    )?;
    Ok(Some(id))
}

/// Cost per base unit from an amount for `base_qty` (thousandths).
fn per_base_unit(amount: i64, base_qty: i64) -> AppResult<i64> {
    i64::try_from(money::div_round(
        i128::from(amount) * 1000,
        i128::from(base_qty),
        RoundingMode::HalfUp,
    ))
    .map_err(AppError::internal)
}

// ------------------------------------------------------------------ purchase orders

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseOrderInput {
    pub id: Option<String>,
    pub party_id: String,
    pub warehouse_id: Option<String>,
    pub expected_date: Option<String>,
    pub lines: Vec<PurchaseLineInput>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDocLine {
    pub id: String,
    pub variant_id: String,
    pub item_name: String,
    pub qty: i64,
    pub unit_cost: i64,
    pub taxable: i64,
    pub tax_total: i64,
    pub line_total: i64,
    /// Received so far (orders) or returned so far (bills).
    pub progress_qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDocDetail {
    pub id: String,
    pub kind: String,
    pub doc_no: String,
    pub business_date: String,
    pub status: String,
    pub party_id: String,
    pub supplier_name: String,
    pub supplier_invoice_no: Option<String>,
    pub taxable: i64,
    pub tax_total: i64,
    pub charges_total: i64,
    pub round_off: i64,
    pub total: i64,
    pub amount_paid: i64,
    pub notes: Option<String>,
    pub lines: Vec<PurchaseDocLine>,
    #[serde(default)]
    pub warnings: Vec<StockWarning>,
}

fn order_detail(conn: &Connection, id: &str) -> AppResult<PurchaseDocDetail> {
    let mut doc = conn
        .query_row(
            "SELECT id, doc_no, business_date, status, party_id, supplier_name, taxable, tax_total, total, notes FROM purchase_orders WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| {
                Ok(PurchaseDocDetail {
                    id: r.get(0)?,
                    kind: "purchase_order".into(),
                    doc_no: r.get(1)?,
                    business_date: r.get(2)?,
                    status: r.get(3)?,
                    party_id: r.get(4)?,
                    supplier_name: r.get(5)?,
                    supplier_invoice_no: None,
                    taxable: r.get(6)?,
                    tax_total: r.get(7)?,
                    charges_total: 0,
                    round_off: 0,
                    total: r.get(8)?,
                    amount_paid: 0,
                    notes: r.get(9)?,
                    lines: Vec::new(),
                    warnings: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The purchase order"))?;
    doc.lines = conn
        .prepare("SELECT id, variant_id, item_name, qty, unit_cost, taxable, tax_total, line_total, received_qty FROM purchase_order_items WHERE purchase_order_id = ?1 ORDER BY line_no")?
        .query_map([id], map_doc_line)?
        .collect::<Result<_, _>>()?;
    Ok(doc)
}

fn map_doc_line(r: &rusqlite::Row<'_>) -> rusqlite::Result<PurchaseDocLine> {
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
    })
}

pub fn get_order(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.view")?;
    order_detail(ctx.conn, &input.id)
}

fn supplier_and_states(
    conn: &Connection,
    settings: &Settings,
    party_id: &str,
    warehouse_id: Option<&str>,
) -> AppResult<(PartyRef, String, String, Option<String>)> {
    let party = party_ledger::load_party(conn, party_id, "supplier")?;
    let (branch_id, warehouse_id, branch_state) = branch_of(conn, warehouse_id)?;
    Ok((
        party,
        branch_id,
        warehouse_id,
        branch_state.or_else(|| settings.business.state_code.clone()),
    ))
}

pub fn save_order(ctx: &mut Ctx<'_>, input: &PurchaseOrderInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.manage")?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let (party, branch_id, warehouse_id, own_state) = supplier_and_states(
        ctx.conn,
        &settings,
        &input.party_id,
        input.warehouse_id.as_deref(),
    )?;
    let priced = price_purchase(
        ctx.conn,
        &settings,
        &input.lines,
        None,
        false,
        party.state_code.as_deref(),
        own_state.as_deref(),
    )?;
    let expected = input
        .expected_date
        .as_deref()
        .map(|d| date("expectedDate", d))
        .transpose()?;
    let notes = optional("notes", input.notes.as_deref(), 1000)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let r = &priced.result;
    let id = write(ctx.conn, |tx| {
        let id = match &input.id {
            Some(id) => {
                let existing = order_detail(tx, id)?;
                if existing.status != "open" || existing.lines.iter().any(|l| l.progress_qty > 0) {
                    return Err(AppError::conflict(
                        "Only open orders with nothing received can be edited.",
                    ));
                }
                tx.execute(
                    "UPDATE purchase_orders SET party_id = ?1, supplier_name = ?2, warehouse_id = ?3, expected_date = ?4, gross = ?5, discount = ?6,
                       taxable = ?7, tax_total = ?8, total = ?9, notes = ?10, updated_at = ?11, updated_by = ?12 WHERE id = ?13",
                    params![party.id, party.name, warehouse_id, expected, r.gross, r.line_discount + r.bill_discount, r.taxable, r.tax, r.taxable + r.tax, notes, now, actor.user_id, id],
                )?;
                tx.execute(
                    "DELETE FROM purchase_order_items WHERE purchase_order_id = ?1",
                    [id],
                )?;
                id.clone()
            }
            None => {
                let number =
                    numbering::next(tx, &settings, &branch_id, "purchase_order", &today, &now)?;
                let id = new_id();
                tx.execute(
                    "INSERT INTO purchase_orders (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, expected_date,
                        status, party_id, supplier_name, gross, discount, taxable, tax_total, total, notes, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'open', ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18, ?19, ?19)",
                    params![
                        id, branch_id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, today, expected, party.id, party.name,
                        r.gross, r.line_discount + r.bill_discount, r.taxable, r.tax, r.taxable + r.tax, notes, now, actor.user_id
                    ],
                )?;
                id
            }
        };
        for (index, (line, lr)) in priced.lines.iter().zip(&r.lines).enumerate() {
            tx.execute(
                "INSERT INTO purchase_order_items (id, purchase_order_id, line_no, variant_id, unit_id, unit_factor_num, unit_factor_den, item_name, sku,
                    qty, base_qty, unit_cost, discount, taxable, tax_rate, tax_total, line_total, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)",
                params![
                    new_id(), id, index + 1, line.variant_id, line.unit_id, line.factor_num, line.factor_den, line.name, line.sku, line.qty,
                    line.base_qty, line.unit_price, lr.line_discount + lr.bill_discount, lr.taxable, line.tax_rate + line.cess_rate, lr.tax, lr.total, now
                ],
            )?;
        }
        AuditEntry::new("PURCHASE_ORDER_SAVED", "purchase_orders", Some(&id))
            .by(actor)
            .write(tx, &now)?;
        Ok(id)
    })?;
    order_detail(ctx.conn, &id)
}

pub fn cancel_order(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let order = order_detail(tx, &input.id)?;
        if !matches!(order.status.as_str(), "open" | "partially_received") {
            return Err(AppError::conflict("This order is already closed."));
        }
        let status = if order.lines.iter().any(|l| l.progress_qty > 0) {
            "closed"
        } else {
            "cancelled"
        };
        tx.execute("UPDATE purchase_orders SET status = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4", params![status, now, actor.user_id, input.id])?;
        AuditEntry::new("PURCHASE_ORDER_CLOSED", "purchase_orders", Some(&input.id))
            .by(actor)
            .summary(status)
            .write(tx, &now)?;
        order_detail(tx, &input.id)
    })
}

/// Adds received quantities to a PO's lines and updates its status.
fn progress_order(
    tx: &Connection,
    po_id: &str,
    lines: &[(Option<String>, i64)],
    now: &str,
) -> AppResult<()> {
    for (po_item, qty) in lines {
        if let Some(item) = po_item {
            let changed = tx.execute(
                "UPDATE purchase_order_items SET received_qty = received_qty + ?1, updated_at = ?2 WHERE id = ?3 AND purchase_order_id = ?4",
                params![qty, now, item, po_id],
            )?;
            if changed == 0 {
                return Err(AppError::field(
                    "poItemId",
                    "A line doesn't belong to that purchase order.",
                ));
            }
        }
    }
    let (open_lines, received_any): (i64, i64) = tx.query_row(
        "SELECT count(*) FILTER (WHERE received_qty < qty), count(*) FILTER (WHERE received_qty > 0) FROM purchase_order_items WHERE purchase_order_id = ?1",
        [po_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let status = if open_lines == 0 {
        "received"
    } else if received_any > 0 {
        "partially_received"
    } else {
        "open"
    };
    tx.execute(
        "UPDATE purchase_orders SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now, po_id],
    )?;
    Ok(())
}

// ------------------------------------------------------------------ goods receipts

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GoodsReceiptInput {
    pub idempotency_key: String,
    pub party_id: String,
    pub warehouse_id: Option<String>,
    pub purchase_order_id: Option<String>,
    pub supplier_reference: Option<String>,
    pub lines: Vec<PurchaseLineInput>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GoodsReceiptDetail {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub party_id: String,
    pub purchase_order_id: Option<String>,
    pub value: i64,
    pub lines: Vec<PurchaseDocLine>,
}

fn receipt_detail(conn: &Connection, id: &str) -> AppResult<GoodsReceiptDetail> {
    let (doc_no, business_date, party_id, purchase_order_id): (String, String, String, Option<String>) = conn
        .query_row("SELECT doc_no, business_date, party_id, purchase_order_id FROM goods_receipts WHERE id = ?1 AND status <> 'draft'", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("The goods receipt"))?;
    let lines: Vec<PurchaseDocLine> = conn
        .prepare(
            "SELECT id, variant_id, item_name, qty, unit_cost, 0, 0, 0, 0 FROM goods_receipt_items WHERE receipt_id = ?1 ORDER BY line_no",
        )?
        .query_map([id], map_doc_line)?
        .collect::<Result<_, _>>()?;
    let value: i64 = conn.query_row("SELECT coalesce(sum(value), 0) FROM stock_movements WHERE source_type = 'goods_receipt' AND source_id = ?1", [id], |r| r.get(0))?;
    Ok(GoodsReceiptDetail {
        id: id.into(),
        doc_no,
        business_date,
        party_id,
        purchase_order_id,
        value,
        lines,
    })
}

/// Brings goods into stock before (or without) the supplier's bill.
pub fn receive_goods(
    ctx: &mut Ctx<'_>,
    input: &GoodsReceiptInput,
) -> AppResult<GoodsReceiptDetail> {
    ctx.require("purchases.receive")?;
    if input.lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one item."));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let (party, branch_id, warehouse_id, _) = supplier_and_states(
        ctx.conn,
        &settings,
        &input.party_id,
        input.warehouse_id.as_deref(),
    )?;
    let reference = optional("supplierReference", input.supplier_reference.as_deref(), 60)?;
    let notes = optional("notes", input.notes.as_deref(), 1000)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "purchases_receive_goods",
            actor,
            input,
            &now,
            || {
                if let Some(po) = &input.purchase_order_id {
                    let order = order_detail(tx, po)?;
                    if order.party_id != party.id
                        || !matches!(order.status.as_str(), "open" | "partially_received")
                    {
                        return Err(AppError::field(
                            "purchaseOrderId",
                            "That order is closed or belongs to another supplier.",
                        ));
                    }
                }
                let number =
                    numbering::next(tx, &settings, &branch_id, "goods_receipt", &today, &now)?;
                let id = new_id();
                tx.execute(
                "INSERT INTO goods_receipts (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, status, party_id,
                    purchase_order_id, supplier_reference, notes, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'draft', ?9, ?10, ?11, ?12, ?13, ?13, ?14, ?14)",
                params![id, branch_id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, today, party.id, input.purchase_order_id, reference, notes, now, actor.user_id],
            )?;
                let mut value = 0;
                let mut progress = Vec::new();
                for (index, line) in input.lines.iter().enumerate() {
                    let r = resolve_line(
                        tx,
                        index,
                        &line.variant_id,
                        line.unit_id.as_deref(),
                        line.qty,
                        None,
                    )?;
                    if r.is_service {
                        return Err(AppError::field(
                            &format!("lines.{index}.variantId"),
                            "Services aren't received into stock.",
                        ));
                    }
                    let amount =
                        money::multiply_by_qty(line.unit_cost, r.qty, RoundingMode::HalfUp);
                    let unit_cost = per_base_unit(amount, r.base_qty)?;
                    let batch = batch_for(tx, &r.variant_id, line, index, &now, &actor.user_id)?;
                    let line_id = new_id();
                    tx.execute(
                    "INSERT INTO goods_receipt_items (id, receipt_id, line_no, po_item_id, variant_id, batch_id, unit_id, unit_factor_num, unit_factor_den,
                        item_name, qty, base_qty, unit_cost, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                    params![line_id, id, index + 1, line.po_item_id, r.variant_id, batch, r.unit_id, r.factor_num, r.factor_den, r.name, r.qty, r.base_qty, unit_cost, now],
                )?;
                    stock_posting::post(
                        tx,
                        settings.inventory.negative_stock_policy,
                        &today,
                        &now,
                        Some(&actor.user_id),
                        &Move {
                            warehouse_id: &warehouse_id,
                            variant_id: &r.variant_id,
                            batch_id: batch.as_deref(),
                            qty: r.base_qty,
                            unit_cost,
                            kind: "purchase",
                            source_type: "goods_receipt",
                            source_id: &id,
                            source_line_id: Some(&line_id),
                            reversal_of: None,
                            note: None,
                            update_average_cost: true,
                        },
                    )?;
                    value += money::multiply_by_qty(unit_cost, r.base_qty, RoundingMode::HalfUp);
                    progress.push((line.po_item_id.clone(), r.qty));
                }
                if let Some(po) = &input.purchase_order_id {
                    progress_order(tx, po, &progress, &now)?;
                }
                tx.execute("UPDATE goods_receipts SET status = 'finalized', finalized_at = ?1, finalized_by = ?2 WHERE id = ?3", params![now, actor.user_id, id])?;
                journal::post(
                    tx,
                    Entry {
                        business_date: &today,
                        source_type: "goods_receipt",
                        source_id: &id,
                        narration: format!("Goods received {}", number.doc_no),
                        lines: vec![
                            JLine::debit(accounts::INVENTORY, value),
                            JLine::credit(accounts::GOODS_RECEIVED_NOT_INVOICED, value),
                        ],
                        user_id: Some(&actor.user_id),
                        now: &now,
                    },
                )?;
                AuditEntry::new("GOODS_RECEIVED", "goods_receipts", Some(&id))
                    .by(actor)
                    .summary(format!("{} from {}", number.doc_no, party.name))
                    .write(tx, &now)?;
                receipt_detail(tx, &id)
            },
        )
    })
}

// ------------------------------------------------------------------ purchase bills

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseBillInput {
    pub idempotency_key: String,
    pub party_id: String,
    pub supplier_invoice_no: Option<String>,
    pub supplier_invoice_date: Option<String>,
    pub business_date: Option<String>,
    pub due_date: Option<String>,
    pub warehouse_id: Option<String>,
    pub prices_include_tax: Option<bool>,
    pub purchase_order_id: Option<String>,
    /// Goods already received with this receipt (the bill then doesn't move stock again).
    pub goods_receipt_id: Option<String>,
    pub lines: Vec<PurchaseLineInput>,
    pub bill_discount: Option<Discount>,
    #[serde(default)]
    pub charges: Vec<ChargeInput>,
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
    pub expected_total: Option<i64>,
    pub notes: Option<String>,
}

fn bill_detail(conn: &Connection, id: &str) -> AppResult<PurchaseDocDetail> {
    let mut doc = conn
        .query_row(
            "SELECT id, doc_no, business_date, status, party_id, supplier_name, supplier_invoice_no, taxable, tax_total, charges_total, round_off, total,
                    amount_paid, notes FROM purchase_bills WHERE id = ?1 AND status <> 'draft'",
            [id],
            |r| {
                Ok(PurchaseDocDetail {
                    id: r.get(0)?,
                    kind: "purchase_bill".into(),
                    doc_no: r.get(1)?,
                    business_date: r.get(2)?,
                    status: r.get(3)?,
                    party_id: r.get(4)?,
                    supplier_name: r.get(5)?,
                    supplier_invoice_no: r.get(6)?,
                    taxable: r.get(7)?,
                    tax_total: r.get(8)?,
                    charges_total: r.get(9)?,
                    round_off: r.get(10)?,
                    total: r.get(11)?,
                    amount_paid: r.get(12)?,
                    notes: r.get(13)?,
                    lines: Vec::new(),
                    warnings: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The purchase bill"))?;
    doc.lines = conn
        .prepare(
            "SELECT l.id, l.variant_id, l.item_name, l.qty, l.unit_cost, l.taxable, l.tax_total, l.line_total,
               coalesce((SELECT sum(ri.qty) FROM purchase_return_items ri JOIN purchase_returns pr ON pr.id = ri.return_id
                         WHERE ri.original_item_id = l.id AND pr.status = 'finalized'), 0)
             FROM purchase_bill_items l WHERE l.bill_id = ?1 ORDER BY l.line_no",
        )?
        .query_map([id], map_doc_line)?
        .collect::<Result<_, _>>()?;
    Ok(doc)
}

pub fn get_bill(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.view")?;
    bill_detail(ctx.conn, &input.id)
}

/// Records a supplier bill: the payable, input tax, landed costs, and (unless a goods receipt is
/// linked) the stock coming in.
pub fn create_bill(ctx: &mut Ctx<'_>, input: &PurchaseBillInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.manage")?;
    let receives_stock = input.goods_receipt_id.is_none();
    if receives_stock {
        ctx.require("purchases.receive")?;
    }
    idempotency::validate_key(&input.idempotency_key)?;
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
            "A bill can't be dated in the future.",
        ));
    }
    let (party, branch_id, warehouse_id, own_state) = supplier_and_states(
        ctx.conn,
        &settings,
        &input.party_id,
        input.warehouse_id.as_deref(),
    )?;
    let include = input.prices_include_tax.unwrap_or(false);
    let priced = price_purchase(
        ctx.conn,
        &settings,
        &input.lines,
        input.bill_discount.as_ref(),
        include,
        party.state_code.as_deref(),
        own_state.as_deref(),
    )?;
    let supplier_invoice_no = optional(
        "supplierInvoiceNo",
        input.supplier_invoice_no.as_deref(),
        40,
    )?;
    let supplier_invoice_date = input
        .supplier_invoice_date
        .as_deref()
        .map(|d| date("supplierInvoiceDate", d))
        .transpose()?;
    let due_date = input
        .due_date
        .as_deref()
        .map(|d| date("dueDate", d))
        .transpose()?;
    let notes = optional("notes", input.notes.as_deref(), 1000)?;

    let mut capitalised = Vec::with_capacity(input.charges.len());
    for (i, c) in input.charges.iter().enumerate() {
        let description = required(&format!("charges.{i}.description"), &c.description, 120)?;
        if c.amount < 0 {
            return Err(AppError::field(
                &format!("charges.{i}.amount"),
                "A charge can't be negative.",
            ));
        }
        if !matches!(c.allocation.as_str(), "value" | "qty" | "none") {
            return Err(AppError::field(
                &format!("charges.{i}.allocation"),
                "Use value, qty or none.",
            ));
        }
        capitalised.push((description, c.amount, c.allocation.clone()));
    }
    let r = &priced.result;
    let charges_total: i64 = capitalised.iter().map(|c| c.1).sum();
    let before_rounding = r.taxable + r.tax + charges_total;
    let total = if settings.tax.round_total_to > 0 {
        i64::try_from(
            money::div_round(
                i128::from(before_rounding),
                i128::from(settings.tax.round_total_to),
                RoundingMode::HalfUp,
            ) * i128::from(settings.tax.round_total_to),
        )
        .map_err(AppError::internal)?
    } else {
        before_rounding
    };
    let round_off = total - before_rounding;
    if let Some(expected) = input.expected_total {
        if expected != total {
            return Err(AppError::new(
                ErrorCode::TotalsMismatch,
                "The bill total differs from the one on screen.",
            )
            .with_details(json!({ "expected": expected, "actual": total })));
        }
    }
    // Landed cost: spread capitalised charges over stocked lines by value or by quantity.
    let stocked: Vec<usize> = (0..priced.lines.len())
        .filter(|i| !priced.lines[*i].is_service)
        .collect();
    let mut shares = vec![0i64; priced.lines.len()];
    for (_, amount, allocation) in capitalised.iter().filter(|c| c.2 != "none") {
        if stocked.is_empty() {
            return Err(AppError::field(
                "charges",
                "There are no stocked items to add these charges to.",
            ));
        }
        let weights: Vec<i64> = stocked
            .iter()
            .map(|i| {
                if allocation == "qty" {
                    priced.lines[*i].base_qty
                } else {
                    r.lines[*i].taxable
                }
            })
            .collect();
        let weights = if weights.iter().all(|w| *w == 0) {
            vec![1; weights.len()]
        } else {
            weights
        };
        for (slot, part) in stocked
            .iter()
            .zip(money::allocate(*amount, &weights).map_err(AppError::internal)?)
        {
            shares[*slot] += part;
        }
    }
    let expensed_charges: i64 = capitalised
        .iter()
        .filter(|c| c.2 == "none")
        .map(|c| c.1)
        .sum();
    let payments = validate_payments(ctx.conn, &input.payments)?;
    if payments.iter().map(|p| p.amount).sum::<i64>() > total {
        return Err(AppError::field(
            "payments",
            "Payments are more than the bill.",
        ));
    }

    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "purchases_create_bill",
            actor,
            input,
            &now,
            || {
                let grn_value = match &input.goods_receipt_id {
                    Some(grn) => {
                        let receipt = receipt_detail(tx, grn)?;
                        if receipt.party_id != party.id {
                            return Err(AppError::field(
                                "goodsReceiptId",
                                "That receipt is from another supplier.",
                            ));
                        }
                        let billed: i64 = tx.query_row("SELECT count(*) FROM purchase_bills WHERE goods_receipt_id = ?1 AND status = 'finalized'", [grn], |r| r.get(0))?;
                        if billed > 0 {
                            return Err(AppError::conflict(
                                "That goods receipt is already billed.",
                            ));
                        }
                        receipt.value
                    }
                    None => 0,
                };
                let number = numbering::next(
                    tx,
                    &settings,
                    &branch_id,
                    "purchase_bill",
                    &business_date,
                    &now,
                )?;
                let id = new_id();
                tx.execute(
                "INSERT INTO purchase_bills (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, supplier_invoice_no, supplier_invoice_date,
                    business_date, due_date, status, party_id, supplier_name, supplier_gstin, place_of_supply, is_inter_state, prices_include_tax,
                    receives_stock, purchase_order_id, goods_receipt_id, gross, discount, taxable, tax_total, charges_total, round_off, total, notes,
                    idempotency_key, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'draft', ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                    ?26, ?27, ?28, ?29, ?30, ?30, ?31, ?31)",
                params![
                    id, branch_id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, supplier_invoice_no, supplier_invoice_date,
                    business_date, due_date, party.id, party.name, party.gstin, party.state_code, priced.inter_state, priced.prices_include_tax,
                    receives_stock, input.purchase_order_id, input.goods_receipt_id, r.gross, r.line_discount + r.bill_discount, r.taxable, r.tax,
                    charges_total, round_off, total, notes, input.idempotency_key, now, actor.user_id
                ],
            )?;
                let mut warnings = Vec::new();
                let mut inventory_value = 0;
                let mut service_value = 0;
                let mut progress = Vec::new();
                for (index, ((line, lr), raw)) in priced
                    .lines
                    .iter()
                    .zip(&r.lines)
                    .zip(&input.lines)
                    .enumerate()
                {
                    let cols = tax_columns(&lr.taxes);
                    let landed = per_base_unit(lr.taxable + shares[index], line.base_qty)?;
                    let batch = if receives_stock && !line.is_service {
                        batch_for(tx, &line.variant_id, raw, index, &now, &actor.user_id)?
                    } else {
                        None
                    };
                    let line_id = new_id();
                    tx.execute(
                    "INSERT INTO purchase_bill_items (id, bill_id, line_no, po_item_id, variant_id, batch_id, unit_id, unit_factor_num, unit_factor_den, item_name,
                        sku, hsn_sac, qty, base_qty, unit_cost, discount, taxable, tax_rate, cgst, sgst, igst, other_tax, cess, tax_total, line_total,
                        landed_unit_cost, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?27)",
                    params![
                        line_id, id, index + 1, raw.po_item_id, line.variant_id, batch, line.unit_id, line.factor_num, line.factor_den, line.name, line.sku,
                        line.hsn_sac, line.qty, line.base_qty, line.unit_price, lr.line_discount + lr.bill_discount, lr.taxable, line.tax_rate + line.cess_rate,
                        cols.cgst, cols.sgst, cols.igst, cols.other, cols.cess, lr.tax, lr.total, landed, now
                    ],
                )?;
                    if line.is_service {
                        service_value += lr.taxable + shares[index];
                    } else if receives_stock {
                        warnings.extend(stock_posting::post(
                            tx,
                            settings.inventory.negative_stock_policy,
                            &business_date,
                            &now,
                            Some(&actor.user_id),
                            &Move {
                                warehouse_id: &warehouse_id,
                                variant_id: &line.variant_id,
                                batch_id: batch.as_deref(),
                                qty: line.base_qty,
                                unit_cost: landed,
                                kind: "purchase",
                                source_type: "purchase_bill",
                                source_id: &id,
                                source_line_id: Some(&line_id),
                                reversal_of: None,
                                note: None,
                                update_average_cost: true,
                            },
                        )?);
                        inventory_value +=
                            money::multiply_by_qty(landed, line.base_qty, RoundingMode::HalfUp);
                        progress.push((raw.po_item_id.clone(), line.qty));
                    }
                }
                for (description, amount, allocation) in &capitalised {
                    tx.execute(
                    "INSERT INTO purchase_bill_charges (id, bill_id, description, amount, allocation, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![new_id(), id, description, amount, allocation, now],
                )?;
                }
                if receives_stock {
                    if let Some(po) = &input.purchase_order_id {
                        progress_order(tx, po, &progress, &now)?;
                    }
                }
                tx.execute("UPDATE purchase_bills SET status = 'finalized', finalized_at = ?1, finalized_by = ?2 WHERE id = ?3", params![now, actor.user_id, id])?;
                party_ledger::post(
                    tx,
                    Posting {
                        party_id: &party.id,
                        business_date: &business_date,
                        entry_type: "purchase_bill",
                        source_type: "purchase_bill",
                        source_id: &id,
                        amount: -total,
                        narration: format!(
                            "Bill {}{}",
                            number.doc_no,
                            supplier_invoice_no
                                .as_ref()
                                .map(|n| format!(" ({n})"))
                                .unwrap_or_default()
                        ),
                        user_id: Some(&actor.user_id),
                        now: &now,
                    },
                )?;
                let goods_cost = r.taxable + shares.iter().sum::<i64>() - service_value;
                let mut lines = vec![
                    JLine::debit(accounts::INPUT_TAX, r.tax),
                    JLine::debit(accounts::GENERAL_EXPENSES, expensed_charges + service_value),
                    JLine::debit(accounts::ROUND_OFF, round_off),
                    JLine::credit(accounts::PAYABLE, total).party(Some(&party.id)),
                ];
                if receives_stock {
                    // Rounding between line costs and stock values goes to round-off.
                    lines.push(JLine::debit(accounts::INVENTORY, inventory_value));
                    lines.push(JLine::debit(
                        accounts::ROUND_OFF,
                        goods_cost - inventory_value,
                    ));
                } else {
                    // Clear the receipt at its stock value; any price difference is a cost variance.
                    lines.push(JLine::debit(
                        accounts::GOODS_RECEIVED_NOT_INVOICED,
                        grn_value,
                    ));
                    lines.push(JLine::debit(
                        accounts::STOCK_ADJUSTMENTS,
                        goods_cost - grn_value,
                    ));
                }
                journal::post(
                    tx,
                    Entry {
                        business_date: &business_date,
                        source_type: "purchase_bill",
                        source_id: &id,
                        narration: format!("Purchase {}", number.doc_no),
                        lines,
                        user_id: Some(&actor.user_id),
                        now: &now,
                    },
                )?;
                for payment in &payments {
                    record_payment(
                        tx,
                        &settings,
                        &branch_id,
                        &business_date,
                        "out",
                        Some(&party.id),
                        payment,
                        Some(("purchase_bill", &id)),
                        accounts::PAYABLE,
                        None,
                        actor,
                        &now,
                    )?;
                }
                AuditEntry::new("PURCHASE_BILL", "purchase_bills", Some(&id))
                    .by(actor)
                    .summary(format!("{} {} total {total}", number.doc_no, party.name))
                    .write(tx, &now)?;
                let mut detail = bill_detail(tx, &id)?;
                detail.warnings = warnings;
                Ok(detail)
            },
        )
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidBillInput {
    pub id: String,
    pub reason: String,
}

pub fn void_bill(ctx: &mut Ctx<'_>, input: &VoidBillInput) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.manage")?;
    let approved_by = ctx.approve("approvals.grant", "void a purchase bill")?;
    let reason = required("reason", &input.reason, 300)?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let bill = bill_detail(tx, &input.id)?;
        if bill.status != "finalized" {
            return Err(AppError::conflict("Only a finalized bill can be voided."));
        }
        if bill.lines.iter().any(|l| l.progress_qty > 0) {
            return Err(AppError::conflict(
                "Goods from this bill were returned. Use a debit note instead.",
            ));
        }
        let payments: Vec<String> = tx
            .prepare("SELECT DISTINCT a.payment_id FROM payment_allocations a JOIN payments p ON p.id = a.payment_id WHERE a.target_type = 'purchase_bill' AND a.target_id = ?1 AND p.status = 'finalized'")?
            .query_map([&bill.id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for payment in payments {
            let elsewhere: i64 = tx.query_row(
                "SELECT count(*) FROM payment_allocations WHERE payment_id = ?1 AND NOT (target_type = 'purchase_bill' AND target_id = ?2)",
                params![payment, bill.id],
                |r| r.get(0),
            )?;
            if elsewhere > 0 {
                return Err(AppError::conflict(
                    "A payment on this bill also pays other bills. Void it separately first.",
                ));
            }
            void_payment(
                tx,
                &payment,
                &today,
                &format!("Bill {} voided", bill.doc_no),
                actor,
                &now,
            )?;
        }
        stock_posting::reverse_source(
            tx,
            "purchase_bill",
            &bill.id,
            &today,
            &now,
            Some(&actor.user_id),
        )?;
        party_ledger::reverse_source(
            tx,
            "purchase_bill",
            &bill.id,
            &today,
            &format!("Bill {} voided", bill.doc_no),
            Some(&actor.user_id),
            &now,
        )?;
        journal::reverse_source(
            tx,
            "purchase_bill",
            &bill.id,
            &today,
            &format!("Void {}", bill.doc_no),
            Some(&actor.user_id),
            &now,
        )?;
        tx.execute(
            "UPDATE purchase_bills SET status = 'voided', voided_at = ?1, voided_by = ?2, void_reason = ?3, updated_at = ?1, updated_by = ?2 WHERE id = ?4",
            params![now, actor.user_id, reason, bill.id],
        )?;
        AuditEntry::new("PURCHASE_BILL_VOIDED", "purchase_bills", Some(&bill.id))
            .by(actor)
            .approved_by(approved_by.as_deref())
            .summary(reason.clone())
            .write(tx, &now)?;
        bill_detail(tx, &bill.id)
    })
}

// ------------------------------------------------------------------ purchase returns

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseReturnLineInput {
    pub original_item_id: Option<String>,
    pub variant_id: Option<String>,
    /// Thousandths of the bill line's unit (or base unit without a bill).
    pub qty: i64,
    /// Without a bill: what the supplier credits per base unit (default: current cost).
    pub unit_cost: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseReturnInput {
    pub idempotency_key: String,
    pub original_bill_id: Option<String>,
    pub party_id: String,
    pub warehouse_id: Option<String>,
    pub reason: String,
    pub lines: Vec<PurchaseReturnLineInput>,
    /// Money the supplier pays back now.
    pub refund: Option<PaymentInput>,
}

/// Goods sent back to a supplier (debit note).
pub fn create_return(
    ctx: &mut Ctx<'_>,
    input: &PurchaseReturnInput,
) -> AppResult<PurchaseDocDetail> {
    ctx.require("purchases.manage")?;
    let reason = required("reason", &input.reason, 300)?;
    if input.lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one item."));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let (party, branch_id, warehouse_id, _) = supplier_and_states(
        ctx.conn,
        &settings,
        &input.party_id,
        input.warehouse_id.as_deref(),
    )?;
    let refund = input
        .refund
        .as_ref()
        .map(|r| validate_payments(ctx.conn, std::slice::from_ref(r)))
        .transpose()?
        .map(|mut v| v.remove(0));
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "purchases_create_return",
            actor,
            input,
            &now,
            || {
                let bill = input
                    .original_bill_id
                    .as_deref()
                    .map(|b| bill_detail(tx, b))
                    .transpose()?;
                if let Some(b) = &bill {
                    if b.party_id != party.id || b.status != "finalized" {
                        return Err(AppError::field(
                            "originalBillId",
                            "That bill is voided or from another supplier.",
                        ));
                    }
                }
                struct Out {
                    original_item_id: Option<String>,
                    variant_id: String,
                    unit_id: String,
                    num: i64,
                    den: i64,
                    name: String,
                    qty: i64,
                    base_qty: i64,
                    unit_cost: i64,
                    stock_cost: i64,
                    taxable: i64,
                    tax_rate: i64,
                    cols: super::sales::TaxColumns,
                    tax: i64,
                }
                let mut outs = Vec::new();
                for (index, line) in input.lines.iter().enumerate() {
                    if line.qty <= 0 {
                        return Err(AppError::field(
                            &format!("lines.{index}.qty"),
                            "Quantity must be greater than zero.",
                        ));
                    }
                    match (&bill, &line.original_item_id) {
                        (Some(b), Some(item)) => {
                            let orig = b.lines.iter().find(|l| l.id == *item).ok_or_else(|| {
                                AppError::field(
                                    &format!("lines.{index}.originalItemId"),
                                    "That line isn't on this bill.",
                                )
                            })?;
                            if line.qty > orig.qty - orig.progress_qty {
                                return Err(AppError::field(
                                    &format!("lines.{index}.qty"),
                                    "That's more than is left on the bill.",
                                ));
                            }
                            #[allow(clippy::type_complexity)]
                        let (variant_id, unit_id, num, den, base_qty, cgst, sgst, igst, other, cess, tax_rate): (String, String, i64, i64, i64, i64, i64, i64, i64, i64, i64) = tx.query_row(
                            "SELECT variant_id, unit_id, unit_factor_num, unit_factor_den, base_qty, cgst, sgst, igst, other_tax, cess, tax_rate FROM purchase_bill_items WHERE id = ?1",
                            [item],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)),
                        )?;
                            let part = |v: i64| {
                                i64::try_from(money::div_round(
                                    i128::from(v) * i128::from(line.qty),
                                    i128::from(orig.qty),
                                    RoundingMode::HalfUp,
                                ))
                                .unwrap_or(0)
                            };
                            let cols = super::sales::TaxColumns {
                                cgst: part(cgst),
                                sgst: part(sgst),
                                igst: part(igst),
                                other: part(other),
                                cess: part(cess),
                            };
                            let stock_cost: i64 = tx.query_row(
                                "SELECT cost_price FROM product_variants WHERE id = ?1",
                                [&variant_id],
                                |r| r.get(0),
                            )?;
                            outs.push(Out {
                                original_item_id: Some(item.clone()),
                                variant_id,
                                unit_id,
                                num,
                                den,
                                name: orig.item_name.clone(),
                                qty: line.qty,
                                base_qty: part(base_qty),
                                unit_cost: orig.unit_cost,
                                stock_cost,
                                taxable: part(orig.taxable),
                                tax_rate,
                                tax: cols.cgst + cols.sgst + cols.igst + cols.other + cols.cess,
                                cols,
                            });
                        }
                        (Some(_), None) => {
                            return Err(AppError::field(
                                &format!("lines.{index}.originalItemId"),
                                "Choose the bill line being returned.",
                            ))
                        }
                        (None, _) => {
                            let variant = line.variant_id.as_deref().ok_or_else(|| {
                                AppError::field(
                                    &format!("lines.{index}.variantId"),
                                    "Choose the product being returned.",
                                )
                            })?;
                            let r = resolve_line(tx, index, variant, None, line.qty, None)?;
                            let unit_cost = line.unit_cost.unwrap_or(r.cost);
                            let taxable =
                                money::multiply_by_qty(unit_cost, r.base_qty, RoundingMode::HalfUp);
                            let taxes = tax_components(
                                settings.tax.regime,
                                r.tax_type.as_deref(),
                                r.tax_rate,
                                r.cess_rate,
                                false,
                            )?;
                            let amounts: Vec<pricing::TaxAmount> = taxes
                                .iter()
                                .map(|t| pricing::TaxAmount {
                                    code: t.code.clone(),
                                    rate: t.rate,
                                    amount: money::apply_rate(
                                        taxable,
                                        t.rate,
                                        RoundingMode::HalfUp,
                                    ),
                                })
                                .collect();
                            let cols = tax_columns(&amounts);
                            outs.push(Out {
                                original_item_id: None,
                                variant_id: r.variant_id.clone(),
                                unit_id: r.unit_id.clone(),
                                num: 1,
                                den: 1,
                                name: r.name.clone(),
                                qty: r.qty,
                                base_qty: r.base_qty,
                                unit_cost,
                                stock_cost: r.cost,
                                taxable,
                                tax_rate: r.tax_rate + r.cess_rate,
                                tax: cols.cgst + cols.sgst + cols.igst + cols.other + cols.cess,
                                cols,
                            });
                        }
                    }
                }
                let taxable: i64 = outs.iter().map(|o| o.taxable).sum();
                let tax: i64 = outs.iter().map(|o| o.tax).sum();
                let total = taxable + tax;
                if refund.as_ref().is_some_and(|r| r.amount > total) {
                    return Err(AppError::field(
                        "refund.amount",
                        "The refund is more than the return's value.",
                    ));
                }
                let number =
                    numbering::next(tx, &settings, &branch_id, "purchase_return", &today, &now)?;
                let id = new_id();
                tx.execute(
                "INSERT INTO purchase_returns (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, status, original_bill_id,
                    party_id, supplier_name, supplier_gstin, reason, gross, taxable, tax_total, total, idempotency_key, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'draft', ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?16, ?17, ?18, ?18, ?19, ?19)",
                params![id, branch_id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, today, input.original_bill_id, party.id, party.name, party.gstin, reason, taxable, tax, total, input.idempotency_key, now, actor.user_id],
            )?;
                let mut stock_value = 0;
                for (index, o) in outs.iter().enumerate() {
                    tx.execute(
                    "INSERT INTO purchase_return_items (id, return_id, line_no, original_item_id, variant_id, unit_id, unit_factor_num, unit_factor_den, item_name,
                        qty, base_qty, unit_cost, taxable, tax_rate, cgst, sgst, igst, other_tax, cess, tax_total, line_total, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?22)",
                    params![new_id(), id, index + 1, o.original_item_id, o.variant_id, o.unit_id, o.num, o.den, o.name, o.qty, o.base_qty, o.unit_cost, o.taxable, o.tax_rate, o.cols.cgst, o.cols.sgst, o.cols.igst, o.cols.other, o.cols.cess, o.tax, o.taxable + o.tax, now],
                )?;
                    stock_posting::post(
                        tx,
                        settings.inventory.negative_stock_policy,
                        &today,
                        &now,
                        Some(&actor.user_id),
                        &Move {
                            warehouse_id: &warehouse_id,
                            variant_id: &o.variant_id,
                            batch_id: None,
                            qty: -o.base_qty,
                            unit_cost: o.stock_cost,
                            kind: "purchase_return",
                            source_type: "purchase_return",
                            source_id: &id,
                            source_line_id: None,
                            reversal_of: None,
                            note: Some(&reason),
                            update_average_cost: false,
                        },
                    )?;
                    stock_value +=
                        money::multiply_by_qty(o.stock_cost, o.base_qty, RoundingMode::HalfUp);
                }
                tx.execute("UPDATE purchase_returns SET status = 'finalized', finalized_at = ?1, finalized_by = ?2 WHERE id = ?3", params![now, actor.user_id, id])?;
                party_ledger::post(
                    tx,
                    Posting {
                        party_id: &party.id,
                        business_date: &today,
                        entry_type: "purchase_return",
                        source_type: "purchase_return",
                        source_id: &id,
                        amount: total,
                        narration: format!("Debit note {}", number.doc_no),
                        user_id: Some(&actor.user_id),
                        now: &now,
                    },
                )?;
                journal::post(
                    tx,
                    Entry {
                        business_date: &today,
                        source_type: "purchase_return",
                        source_id: &id,
                        narration: format!("Purchase return {}", number.doc_no),
                        lines: vec![
                            JLine::debit(accounts::PAYABLE, total).party(Some(&party.id)),
                            JLine::credit(accounts::INPUT_TAX, tax),
                            JLine::credit(accounts::INVENTORY, stock_value),
                            JLine::credit(accounts::STOCK_ADJUSTMENTS, taxable - stock_value),
                        ],
                        user_id: Some(&actor.user_id),
                        now: &now,
                    },
                )?;
                if let Some(refund) = &refund {
                    record_payment(
                        tx,
                        &settings,
                        &branch_id,
                        &today,
                        "in",
                        Some(&party.id),
                        refund,
                        Some(("purchase_return", &id)),
                        accounts::PAYABLE,
                        None,
                        actor,
                        &now,
                    )?;
                }
                AuditEntry::new("PURCHASE_RETURN", "purchase_returns", Some(&id))
                    .by(actor)
                    .summary(format!("{}: {reason}", number.doc_no))
                    .write(tx, &now)?;
                let detail = PurchaseDocDetail {
                    id: id.clone(),
                    kind: "purchase_return".into(),
                    doc_no: number.doc_no.clone(),
                    business_date: today.clone(),
                    status: "finalized".into(),
                    party_id: party.id.clone(),
                    supplier_name: party.name.clone(),
                    supplier_invoice_no: None,
                    taxable,
                    tax_total: tax,
                    charges_total: 0,
                    round_off: 0,
                    total,
                    amount_paid: refund.as_ref().map_or(0, |r| r.amount),
                    notes: Some(reason.clone()),
                    lines: outs
                        .iter()
                        .map(|o| PurchaseDocLine {
                            id: String::new(),
                            variant_id: o.variant_id.clone(),
                            item_name: o.name.clone(),
                            qty: o.qty,
                            unit_cost: o.unit_cost,
                            taxable: o.taxable,
                            tax_total: o.tax,
                            line_total: o.taxable + o.tax,
                            progress_qty: 0,
                        })
                        .collect(),
                    warnings: Vec::new(),
                };
                Ok(detail)
            },
        )
    })
}

// ------------------------------------------------------------------ lists

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseListQuery {
    /// "purchase_order", "purchase_bill" or "purchase_return".
    pub kind: String,
    pub party_id: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseListItem {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub supplier_name: String,
    pub supplier_invoice_no: Option<String>,
    pub total: i64,
    pub amount_paid: i64,
    pub status: String,
}

pub fn list(ctx: &Ctx<'_>, query: &PurchaseListQuery) -> AppResult<Page<PurchaseListItem>> {
    ctx.require("purchases.view")?;
    let (table, invoice_no, paid) = match query.kind.as_str() {
        "purchase_order" => ("purchase_orders", "NULL", "0"),
        "purchase_bill" => ("purchase_bills", "supplier_invoice_no", "amount_paid"),
        "purchase_return" => ("purchase_returns", "NULL", "amount_refunded"),
        _ => {
            return Err(AppError::field(
                "kind",
                "Use purchase_order, purchase_bill or purchase_return.",
            ))
        }
    };
    let mut filters = vec!["status <> 'draft'".to_string()];
    let mut args: Vec<SqlValue> = Vec::new();
    if let Some(p) = &query.party_id {
        filters.push("party_id = ?".into());
        args.push(p.clone().into());
    }
    if let Some(s) = &query.status {
        filters.push("status = ?".into());
        args.push(s.clone().into());
    }
    if let Some(from) = &query.from {
        filters.push("business_date >= ?".into());
        args.push(date("from", from)?.into());
    }
    if let Some(to) = &query.to {
        filters.push("business_date <= ?".into());
        args.push(date("to", to)?.into());
    }
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        filters.push(format!("(doc_no LIKE ? ESCAPE '\\' OR lower(supplier_name) LIKE lower(?) ESCAPE '\\' OR coalesce({invoice_no}, '') LIKE ? ESCAPE '\\')"));
        let p = like_pattern(search);
        args.extend([p.clone().into(), p.clone().into(), p.into()]);
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT id, doc_no, business_date, supplier_name, {invoice_no}, total, {paid}, status FROM {table} WHERE {where_sql}
             ORDER BY business_date DESC, created_at DESC LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(PurchaseListItem {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                supplier_name: r.get(3)?,
                supplier_invoice_no: r.get(4)?,
                total: r.get(5)?,
                amount_paid: r.get(6)?,
                status: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}
