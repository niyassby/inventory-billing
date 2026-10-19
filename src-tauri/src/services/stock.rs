//! Stock documents: adjustments, transfers between stores, physical counts (stock-take with a
//! variance report), and the stock card. Every change goes through `stock_posting`.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::{date, optional, positive, required};
use super::idempotency;
use super::journal::{self, Entry, JLine};
use super::numbering;
use super::settings::{self, Settings};
use super::setup::default_location;
use super::stock_posting::{self, Move, StockWarning};
use super::users::IdInput;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Warehouse {
    pub id: String,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_warehouses(ctx: &Ctx<'_>) -> AppResult<Vec<Warehouse>> {
    ctx.require("stock.view")?;
    let mut stmt = ctx
        .conn
        .prepare("SELECT id, branch_id, code, name, is_default FROM warehouses WHERE deleted_at IS NULL ORDER BY is_default DESC, name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Warehouse {
                id: r.get(0)?,
                branch_id: r.get(1)?,
                code: r.get(2)?,
                name: r.get(3)?,
                is_default: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn warehouse(conn: &Connection, id: Option<&str>) -> AppResult<String> {
    match id {
        None => Ok(default_location(conn)?.1),
        Some(id) => {
            let found: i64 = conn.query_row(
                "SELECT count(*) FROM warehouses WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                |r| r.get(0),
            )?;
            if found == 0 {
                return Err(AppError::field("warehouseId", "Choose an existing store."));
            }
            Ok(id.to_string())
        }
    }
}

struct VariantInfo {
    name: String,
    sku: String,
    cost: i64,
    is_service: bool,
}

fn variant(conn: &Connection, id: &str) -> AppResult<VariantInfo> {
    conn.query_row(
        "SELECT p.name || coalesce(' — ' || v.variant_name, ''), v.sku, v.cost_price, p.is_service
         FROM product_variants v JOIN products p ON p.id = v.product_id
         WHERE v.id = ?1 AND v.deleted_at IS NULL AND p.deleted_at IS NULL",
        [id],
        |r| {
            Ok(VariantInfo {
                name: r.get(0)?,
                sku: r.get(1)?,
                cost: r.get(2)?,
                is_service: r.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::field("variantId", "A product in the list no longer exists."))
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdjustLine {
    pub variant_id: String,
    pub batch_id: Option<String>,
    /// Signed change in thousandths of the base unit (+ found / received, − damaged / lost).
    pub qty_change: i64,
    /// Cost per base unit for stock added (default: current average cost).
    pub unit_cost: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdjustInput {
    pub idempotency_key: String,
    pub warehouse_id: Option<String>,
    pub reason: String,
    pub notes: Option<String>,
    pub lines: Vec<AdjustLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockDocLine {
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub qty: i64,
    pub unit_cost: i64,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockDocResult {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub lines: Vec<StockDocLine>,
    pub warnings: Vec<StockWarning>,
}

#[allow(
    clippy::too_many_arguments,
    reason = "the adjustment journal needs every document field"
)]
fn post_adjustment_journal(
    tx: &Connection,
    today: &str,
    source_type: &str,
    source_id: &str,
    doc_no: &str,
    net_value: i64,
    user: &str,
    now: &str,
) -> AppResult<()> {
    journal::post(
        tx,
        Entry {
            business_date: today,
            source_type,
            source_id,
            narration: format!("Stock adjustment {doc_no}"),
            lines: vec![
                JLine::debit(accounts::INVENTORY, net_value),
                JLine::credit(accounts::STOCK_ADJUSTMENTS, net_value),
            ],
            user_id: Some(user),
            now,
        },
    )?;
    Ok(())
}

pub fn adjust(ctx: &mut Ctx<'_>, input: &AdjustInput) -> AppResult<StockDocResult> {
    ctx.require("stock.adjust")?;
    let reason = required("reason", &input.reason, 200)?;
    let notes = optional("notes", input.notes.as_deref(), 1000)?;
    if input.lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one product."));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "stock_adjust",
            actor,
            input,
            &now,
            || {
                let warehouse_id = warehouse(tx, input.warehouse_id.as_deref())?;
                let (branch_id,): (String,) = tx.query_row(
                    "SELECT branch_id FROM warehouses WHERE id = ?1",
                    [&warehouse_id],
                    |r| Ok((r.get(0)?,)),
                )?;
                let number =
                    numbering::next(tx, &settings, &branch_id, "stock_adjustment", &today, &now)?;
                let id = new_id();
                tx.execute(
                "INSERT INTO stock_adjustments (id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, reason, notes,
                    status, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'draft', ?10, ?10, ?11, ?11)",
                params![id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, today, reason, notes, now, actor.user_id],
            )?;
                let mut lines = Vec::new();
                let mut warnings = Vec::new();
                let mut net_value = 0;
                for (index, line) in input.lines.iter().enumerate() {
                    if line.qty_change == 0 {
                        return Err(AppError::field(
                            &format!("lines.{index}.qtyChange"),
                            "Enter a non-zero quantity.",
                        ));
                    }
                    let info = variant(tx, &line.variant_id)?;
                    if info.is_service {
                        return Err(AppError::field(
                            &format!("lines.{index}.variantId"),
                            "Services don't carry stock.",
                        ));
                    }
                    let unit_cost = if line.qty_change > 0 {
                        line.unit_cost.unwrap_or(info.cost)
                    } else {
                        info.cost
                    };
                    let line_id = new_id();
                    tx.execute(
                    "INSERT INTO stock_adjustment_items (id, adjustment_id, line_no, variant_id, batch_id, qty_change, unit_cost, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![line_id, id, index + 1, line.variant_id, line.batch_id, line.qty_change, unit_cost, now],
                )?;
                    let warning = stock_posting::post(
                        tx,
                        settings.inventory.negative_stock_policy,
                        &today,
                        &now,
                        Some(&actor.user_id),
                        &Move {
                            warehouse_id: &warehouse_id,
                            variant_id: &line.variant_id,
                            batch_id: line.batch_id.as_deref(),
                            qty: line.qty_change,
                            unit_cost,
                            kind: if line.qty_change > 0 {
                                "adjustment_in"
                            } else {
                                "adjustment_out"
                            },
                            source_type: "stock_adjustment",
                            source_id: &id,
                            source_line_id: Some(&line_id),
                            reversal_of: None,
                            note: Some(&reason),
                            update_average_cost: false,
                        },
                    )?;
                    warnings.extend(warning);
                    let value =
                        money::multiply_by_qty(unit_cost, line.qty_change, RoundingMode::HalfUp);
                    net_value += value;
                    lines.push(StockDocLine {
                        variant_id: line.variant_id.clone(),
                        name: info.name,
                        sku: info.sku,
                        qty: line.qty_change,
                        unit_cost,
                        value,
                    });
                }
                tx.execute(
                "UPDATE stock_adjustments SET status = 'finalized', finalized_at = ?1, finalized_by = ?2 WHERE id = ?3",
                params![now, actor.user_id, id],
            )?;
                post_adjustment_journal(
                    tx,
                    &today,
                    "stock_adjustment",
                    &id,
                    &number.doc_no,
                    net_value,
                    &actor.user_id,
                    &now,
                )?;
                AuditEntry::new("STOCK_ADJUSTMENT", "stock_adjustments", Some(&id))
                    .by(actor)
                    .summary(format!("{} — {reason}", number.doc_no))
                    .after(serde_json::to_value(&lines)?)
                    .write(tx, &now)?;
                Ok(StockDocResult {
                    id,
                    doc_no: number.doc_no,
                    business_date: today.clone(),
                    lines,
                    warnings,
                })
            },
        )
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransferLine {
    pub variant_id: String,
    pub batch_id: Option<String>,
    pub qty: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    pub idempotency_key: String,
    pub from_warehouse_id: String,
    pub to_warehouse_id: String,
    pub notes: Option<String>,
    pub lines: Vec<TransferLine>,
}

/// Moves stock between two stores in one step (dispatched and received together).
pub fn transfer(ctx: &mut Ctx<'_>, input: &TransferInput) -> AppResult<StockDocResult> {
    ctx.require("stock.transfer")?;
    if input.from_warehouse_id == input.to_warehouse_id {
        return Err(AppError::field(
            "toWarehouseId",
            "Choose two different stores.",
        ));
    }
    if input.lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one product."));
    }
    let notes = optional("notes", input.notes.as_deref(), 1000)?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "stock_transfer",
            actor,
            input,
            &now,
            || {
                let from = warehouse(tx, Some(&input.from_warehouse_id))?;
                let to = warehouse(tx, Some(&input.to_warehouse_id))?;
                let branch_id: String = tx.query_row(
                    "SELECT branch_id FROM warehouses WHERE id = ?1",
                    [&from],
                    |r| r.get(0),
                )?;
                let number =
                    numbering::next(tx, &settings, &branch_id, "stock_transfer", &today, &now)?;
                let id = new_id();
                tx.execute(
                "INSERT INTO stock_transfers (id, from_warehouse_id, to_warehouse_id, series_id, number_period, doc_seq, doc_no, business_date,
                    status, notes, dispatched_at, dispatched_by, received_at, received_by, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'received', ?9, ?10, ?11, ?10, ?11, ?10, ?10, ?11, ?11)",
                params![id, from, to, number.series_id, number.period, number.seq, number.doc_no, today, notes, now, actor.user_id],
            )?;
                let mut lines = Vec::new();
                let mut warnings = Vec::new();
                for (index, line) in input.lines.iter().enumerate() {
                    positive(&format!("lines.{index}.qty"), line.qty)?;
                    let info = variant(tx, &line.variant_id)?;
                    let line_id = new_id();
                    tx.execute(
                    "INSERT INTO stock_transfer_items (id, transfer_id, line_no, variant_id, batch_id, qty, unit_cost, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![line_id, id, index + 1, line.variant_id, line.batch_id, line.qty, info.cost, now],
                )?;
                    for (warehouse_id, qty, kind) in [
                        (&from, -line.qty, "transfer_out"),
                        (&to, line.qty, "transfer_in"),
                    ] {
                        warnings.extend(stock_posting::post(
                            tx,
                            settings.inventory.negative_stock_policy,
                            &today,
                            &now,
                            Some(&actor.user_id),
                            &Move {
                                warehouse_id,
                                variant_id: &line.variant_id,
                                batch_id: line.batch_id.as_deref(),
                                qty,
                                unit_cost: info.cost,
                                kind,
                                source_type: "stock_transfer",
                                source_id: &id,
                                source_line_id: Some(&line_id),
                                reversal_of: None,
                                note: None,
                                update_average_cost: false,
                            },
                        )?);
                    }
                    let value = money::multiply_by_qty(info.cost, line.qty, RoundingMode::HalfUp);
                    lines.push(StockDocLine {
                        variant_id: line.variant_id.clone(),
                        name: info.name,
                        sku: info.sku,
                        qty: line.qty,
                        unit_cost: info.cost,
                        value,
                    });
                }
                AuditEntry::new("STOCK_TRANSFER", "stock_transfers", Some(&id))
                    .by(actor)
                    .summary(number.doc_no.clone())
                    .write(tx, &now)?;
                Ok(StockDocResult {
                    id,
                    doc_no: number.doc_no,
                    business_date: today.clone(),
                    lines,
                    warnings,
                })
            },
        )
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockCardQuery {
    pub variant_id: String,
    pub warehouse_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockCardLine {
    pub business_date: String,
    pub movement_type: String,
    pub source_type: String,
    pub source_id: String,
    pub doc_no: Option<String>,
    pub warehouse_name: String,
    pub qty: i64,
    pub unit_cost: i64,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockCard {
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub opening_qty: i64,
    pub lines: Vec<StockCardLine>,
    pub closing_qty: i64,
}

/// Every movement of one item with a running balance (fixes v1's double-counted history, B-11).
pub fn stock_card(ctx: &Ctx<'_>, query: &StockCardQuery) -> AppResult<StockCard> {
    ctx.require("stock.view")?;
    let info = variant(ctx.conn, &query.variant_id)?;
    let from = query.from.as_deref().map(|d| date("from", d)).transpose()?;
    let to = query.to.as_deref().map(|d| date("to", d)).transpose()?;
    let opening: i64 = match &from {
        Some(from) => ctx.conn.query_row(
            "SELECT coalesce(sum(qty), 0) FROM stock_movements WHERE variant_id = ?1 AND (?2 IS NULL OR warehouse_id = ?2) AND business_date < ?3",
            params![query.variant_id, query.warehouse_id, from],
            |r| r.get(0),
        )?,
        None => 0,
    };
    let mut stmt = ctx.conn.prepare(
        "SELECT m.business_date, m.movement_type, m.source_type, m.source_id,
           coalesce((SELECT doc_no FROM sales_invoices WHERE id = m.source_id),
                    (SELECT doc_no FROM sales_returns WHERE id = m.source_id),
                    (SELECT doc_no FROM purchase_bills WHERE id = m.source_id),
                    (SELECT doc_no FROM goods_receipts WHERE id = m.source_id),
                    (SELECT doc_no FROM purchase_returns WHERE id = m.source_id),
                    (SELECT doc_no FROM stock_adjustments WHERE id = m.source_id),
                    (SELECT doc_no FROM stock_transfers WHERE id = m.source_id),
                    (SELECT doc_no FROM stock_counts WHERE id = m.source_id)),
           w.name, m.qty, m.unit_cost
         FROM stock_movements m JOIN warehouses w ON w.id = m.warehouse_id
         WHERE m.variant_id = ?1 AND (?2 IS NULL OR m.warehouse_id = ?2)
           AND (?3 IS NULL OR m.business_date >= ?3) AND (?4 IS NULL OR m.business_date <= ?4)
         ORDER BY m.business_date, m.occurred_at, m.rowid",
    )?;
    let mut balance = opening;
    let lines = stmt
        .query_map(
            params![query.variant_id, query.warehouse_id, from, to],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get::<_, i64>(6)?,
                    r.get(7)?,
                ))
            },
        )?
        .map(|row| {
            let (
                business_date,
                movement_type,
                source_type,
                source_id,
                doc_no,
                warehouse_name,
                qty,
                unit_cost,
            ) = row?;
            balance += qty;
            Ok(StockCardLine {
                business_date,
                movement_type,
                source_type,
                source_id,
                doc_no,
                warehouse_name,
                qty,
                unit_cost,
                balance,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(StockCard {
        variant_id: query.variant_id.clone(),
        name: info.name,
        sku: info.sku,
        opening_qty: opening,
        lines,
        closing_qty: balance,
    })
}

// ------------------------------------------------------------------ stock-take

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountStartInput {
    pub warehouse_id: Option<String>,
    pub scope_note: Option<String>,
    /// Limit the count to these variants (default: every stocked item).
    pub variant_ids: Option<Vec<String>>,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountLine {
    pub id: String,
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub system_qty: i64,
    pub counted_qty: Option<i64>,
    pub variance_qty: Option<i64>,
    pub unit_cost: i64,
    pub variance_value: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountDetail {
    pub id: String,
    pub doc_no: String,
    pub warehouse_id: String,
    pub status: String,
    pub business_date: String,
    pub scope_note: Option<String>,
    pub lines: Vec<CountLine>,
    pub counted_lines: usize,
    pub total_variance_value: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountEntry {
    pub line_id: String,
    /// None clears the count; thousandths of the base unit.
    pub counted_qty: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountEntriesInput {
    pub count_id: String,
    pub entries: Vec<CountEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountFinalizeInput {
    pub count_id: String,
    pub idempotency_key: String,
}

fn count_detail(conn: &Connection, id: &str) -> AppResult<CountDetail> {
    let (doc_no, warehouse_id, status, business_date, scope_note): (String, String, String, String, Option<String>) = conn
        .query_row(
            "SELECT doc_no, warehouse_id, status, business_date, scope_note FROM stock_counts WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The stock count"))?;
    let mut stmt = conn.prepare(
        "SELECT l.id, l.variant_id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, l.system_qty, l.counted_qty, l.variance_qty, l.unit_cost
         FROM stock_count_lines l JOIN product_variants v ON v.id = l.variant_id JOIN products p ON p.id = v.product_id
         WHERE l.count_id = ?1 ORDER BY lower(p.name), v.sku",
    )?;
    let lines: Vec<CountLine> = stmt
        .query_map([id], |r| {
            let variance: Option<i64> = r.get(6)?;
            let unit_cost: i64 = r.get(7)?;
            Ok(CountLine {
                id: r.get(0)?,
                variant_id: r.get(1)?,
                name: r.get(2)?,
                sku: r.get(3)?,
                system_qty: r.get(4)?,
                counted_qty: r.get(5)?,
                variance_qty: variance,
                unit_cost,
                variance_value: variance
                    .map(|v| money::multiply_by_qty(unit_cost, v, RoundingMode::HalfUp)),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(CountDetail {
        id: id.to_string(),
        doc_no,
        warehouse_id,
        status,
        business_date,
        scope_note,
        counted_lines: lines.iter().filter(|l| l.counted_qty.is_some()).count(),
        total_variance_value: lines.iter().filter_map(|l| l.variance_value).sum(),
        lines,
    })
}

pub fn get_count(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<CountDetail> {
    ctx.require("stock.view")?;
    count_detail(ctx.conn, &input.id)
}

/// Starts a count: freezes the system quantity of every item in scope.
pub fn start_count(ctx: &mut Ctx<'_>, input: &CountStartInput) -> AppResult<CountDetail> {
    ctx.require("stock.count")?;
    let scope_note = optional("scopeNote", input.scope_note.as_deref(), 300)?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| {
        let warehouse_id = warehouse(tx, input.warehouse_id.as_deref())?;
        let open: i64 = tx.query_row(
            "SELECT count(*) FROM stock_counts WHERE warehouse_id = ?1 AND status = 'counting' AND deleted_at IS NULL",
            [&warehouse_id],
            |r| r.get(0),
        )?;
        if open > 0 {
            return Err(AppError::conflict(
                "A count is already in progress for this store. Finish it first.",
            ));
        }
        let branch_id: String = tx.query_row(
            "SELECT branch_id FROM warehouses WHERE id = ?1",
            [&warehouse_id],
            |r| r.get(0),
        )?;
        let number = numbering::next(tx, &settings, &branch_id, "stock_count", &today, &now)?;
        let id = new_id();
        tx.execute(
            "INSERT INTO stock_counts (id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, scope_note, status,
                created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'draft', ?9, ?9, ?10, ?10)",
            params![id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, today, scope_note, now, actor.user_id],
        )?;
        let wanted: Option<Vec<String>> = input.variant_ids.clone();
        let mut stmt = tx.prepare(
            "SELECT v.id, v.cost_price, coalesce((SELECT sum(qty) FROM stock_levels l WHERE l.variant_id = v.id AND l.warehouse_id = ?1), 0)
             FROM product_variants v JOIN products p ON p.id = v.product_id
             WHERE v.deleted_at IS NULL AND p.deleted_at IS NULL AND p.is_service = 0 AND (?2 IS NULL OR p.category_id = ?2)",
        )?;
        let rows: Vec<(String, i64, i64)> = stmt
            .query_map(params![warehouse_id, input.category_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        let mut added = 0;
        for (variant_id, cost, qty) in rows {
            if wanted.as_ref().is_some_and(|w| !w.contains(&variant_id)) {
                continue;
            }
            tx.execute(
                "INSERT INTO stock_count_lines (id, count_id, variant_id, system_qty, unit_cost, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![new_id(), id, variant_id, qty, cost, now],
            )?;
            added += 1;
        }
        if added == 0 {
            return Err(AppError::validation(
                "There is nothing to count in this scope.",
            ));
        }
        tx.execute(
            "UPDATE stock_counts SET status = 'counting' WHERE id = ?1",
            [&id],
        )?;
        AuditEntry::new("STOCK_COUNT_STARTED", "stock_counts", Some(&id))
            .by(actor)
            .summary(number.doc_no)
            .write(tx, &now)?;
        Ok(id)
    })?;
    count_detail(ctx.conn, &id)
}

pub fn enter_counts(ctx: &mut Ctx<'_>, input: &CountEntriesInput) -> AppResult<CountDetail> {
    ctx.require("stock.count")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let status: String = tx
            .query_row(
                "SELECT status FROM stock_counts WHERE id = ?1 AND deleted_at IS NULL",
                [&input.count_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("The stock count"))?;
        if status != "counting" {
            return Err(AppError::conflict("This count is no longer open."));
        }
        for entry in &input.entries {
            if entry.counted_qty.is_some_and(|q| q < 0) {
                return Err(AppError::field(
                    "countedQty",
                    "A counted quantity can't be negative.",
                ));
            }
            let changed = tx.execute(
                "UPDATE stock_count_lines SET counted_qty = ?1, counted_by = ?2, counted_at = ?3, updated_at = ?3 WHERE id = ?4 AND count_id = ?5",
                params![entry.counted_qty, actor.user_id, now, entry.line_id, input.count_id],
            )?;
            if changed == 0 {
                return Err(AppError::field(
                    "lineId",
                    "A line doesn't belong to this count.",
                ));
            }
        }
        Ok(())
    })?;
    count_detail(ctx.conn, &input.count_id)
}

/// Posts the variance of every counted line and closes the count. Uncounted lines are left alone.
pub fn finalize_count(ctx: &mut Ctx<'_>, input: &CountFinalizeInput) -> AppResult<CountDetail> {
    ctx.require("stock.count")?;
    ctx.require("stock.adjust")?;
    let settings: Settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "stock_count_finalize",
            actor,
            input,
            &now,
            || {
                let detail = count_detail(tx, &input.count_id)?;
                if detail.status != "counting" {
                    return Err(AppError::conflict("This count is no longer open."));
                }
                let mut net_value = 0;
                let mut by_variant: HashMap<&str, i64> = HashMap::new();
                for line in detail
                    .lines
                    .iter()
                    .filter(|l| l.variance_qty.is_some_and(|v| v != 0))
                {
                    let variance = line.variance_qty.unwrap_or(0);
                    *by_variant.entry(&line.variant_id).or_default() += variance;
                    stock_posting::post(
                        tx,
                        super::settings::NegativeStockPolicy::Allow,
                        &today,
                        &now,
                        Some(&actor.user_id),
                        &Move {
                            warehouse_id: &detail.warehouse_id,
                            variant_id: &line.variant_id,
                            batch_id: None,
                            qty: variance,
                            unit_cost: line.unit_cost,
                            kind: "count_variance",
                            source_type: "stock_count",
                            source_id: &detail.id,
                            source_line_id: Some(&line.id),
                            reversal_of: None,
                            note: Some("Stock-take variance"),
                            update_average_cost: false,
                        },
                    )?;
                    net_value += line.variance_value.unwrap_or(0);
                }
                tx.execute(
                "UPDATE stock_counts SET status = 'finalized', finalized_at = ?1, finalized_by = ?2, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
                params![now, actor.user_id, detail.id],
            )?;
                post_adjustment_journal(
                    tx,
                    &today,
                    "stock_count",
                    &detail.id,
                    &detail.doc_no,
                    net_value,
                    &actor.user_id,
                    &now,
                )?;
                AuditEntry::new("STOCK_COUNT_FINALIZED", "stock_counts", Some(&detail.id))
                    .by(actor)
                    .summary(format!(
                        "{}: {} item(s) with variance, net value {net_value}",
                        detail.doc_no,
                        by_variant.len()
                    ))
                    .write(tx, &now)?;
                count_detail(tx, &detail.id)
            },
        )
    })
}
