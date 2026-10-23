//! Inventory depth (Phase 10): batches on hand (the till's batch picker), expiry (summary and
//! notices), the audited switch between weighted-average and FIFO valuation, and unit conversions
//! that apply to every product.

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::common::optional;
use super::settings::{self, Settings};
use super::stock_posting::{self, Valuation};
use super::users::IdInput;
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

fn add_days(day: &str, days: i64) -> String {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|d| {
            (d + chrono::Duration::days(days))
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| day.to_string())
}

fn days_between(from: &str, to: &str) -> Option<i64> {
    let a = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").ok()?;
    let b = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d").ok()?;
    Some((b - a).num_days())
}

// ------------------------------------------------------------------ batches on hand

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchQuery {
    pub variant_id: String,
    pub warehouse_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchStock {
    /// None: stock of the product that has no batch.
    pub batch_id: Option<String>,
    pub batch_no: Option<String>,
    pub mfg_date: Option<String>,
    pub expiry_date: Option<String>,
    pub qty: i64,
    /// Days until expiry (negative: expired); None without an expiry date.
    pub days_left: Option<i64>,
    pub expired: bool,
    /// The batch a sale takes first (first expiry, first out).
    pub picked_first: bool,
}

/// A product's batches with stock in a store, in the order sales take them.
pub fn batches(ctx: &Ctx<'_>, q: &BatchQuery) -> AppResult<Vec<BatchStock>> {
    if !(ctx.actor.can("stock.view") || ctx.actor.can("sales.create")) {
        return Err(AppError::forbidden("stock.view"));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let warehouse = match &q.warehouse_id {
        Some(w) => w.clone(),
        None => super::setup::default_location(ctx.conn)?.1,
    };
    let mut stmt = ctx.conn.prepare(
        "SELECT l.batch_id, b.batch_no, b.mfg_date, b.expiry_date, l.qty FROM stock_levels l
         LEFT JOIN batches b ON b.id = l.batch_id
         WHERE l.variant_id = ?1 AND l.warehouse_id = ?2 AND l.qty > 0
         ORDER BY l.batch_id IS NULL, b.expiry_date IS NULL, b.expiry_date, b.created_at, l.batch_key",
    )?;
    let mut rows: Vec<BatchStock> = stmt
        .query_map(params![q.variant_id, warehouse], |r| {
            let expiry: Option<String> = r.get(3)?;
            Ok(BatchStock {
                batch_id: r.get(0)?,
                batch_no: r.get(1)?,
                mfg_date: r.get(2)?,
                days_left: expiry.as_deref().and_then(|e| days_between(&today, e)),
                expired: expiry.as_deref().is_some_and(|e| e < today.as_str()),
                expiry_date: expiry,
                qty: r.get(4)?,
                picked_first: false,
            })
        })?
        .collect::<Result<_, _>>()?;
    let blocked = settings.inventory.expired_stock_policy != settings::ExpiredStockPolicy::Allow;
    if let Some(first) = rows.iter_mut().find(|b| !(blocked && b.expired)) {
        first.picked_first = true;
    }
    Ok(rows)
}

// ------------------------------------------------------------------ expiry

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpirySummary {
    pub alert_days: i64,
    pub expired_batches: i64,
    pub expired_value: i64,
    pub near_batches: i64,
    pub near_value: i64,
}

pub(crate) fn expiry_summary_of(
    conn: &Connection,
    settings: &Settings,
    today: &str,
) -> AppResult<ExpirySummary> {
    let horizon = add_days(today, settings.inventory.expiry_alert_days);
    let (expired_batches, expired_value, near_batches, near_value): (i64, i64, i64, i64) = conn.query_row(
        "SELECT count(DISTINCT CASE WHEN b.expiry_date < ?1 THEN b.id END),
                coalesce(sum(CASE WHEN b.expiry_date < ?1 THEN l.value END), 0),
                count(DISTINCT CASE WHEN b.expiry_date >= ?1 AND b.expiry_date <= ?2 THEN b.id END),
                coalesce(sum(CASE WHEN b.expiry_date >= ?1 AND b.expiry_date <= ?2 THEN l.value END), 0)
         FROM stock_levels l JOIN batches b ON b.id = l.batch_id
         WHERE l.qty > 0 AND b.expiry_date IS NOT NULL AND b.deleted_at IS NULL",
        params![today, horizon],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(ExpirySummary {
        alert_days: settings.inventory.expiry_alert_days,
        expired_batches,
        expired_value,
        near_batches,
        near_value,
    })
}

/// The dashboard card: expired and near-expiry stock.
pub fn expiry_summary(ctx: &Ctx<'_>) -> AppResult<ExpirySummary> {
    if !(ctx.actor.can("stock.view") || ctx.actor.can("dashboard.view")) {
        return Err(AppError::forbidden("stock.view"));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    expiry_summary_of(ctx.conn, &settings, &today)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpiryRow {
    pub variant_id: String,
    pub batch_id: String,
    pub warehouse_id: String,
    pub name: String,
    pub sku: String,
    pub batch_no: String,
    pub expiry_date: String,
    pub days_left: i64,
    pub warehouse_name: String,
    pub qty: i64,
    pub value: i64,
    pub expired: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpiryQuery {
    /// Look this many days ahead (default: the alert window from the settings).
    pub days: Option<i64>,
    pub warehouse_id: Option<String>,
}

/// Batches on hand that have expired or expire within the window, soonest first.
pub fn expiry_list(ctx: &Ctx<'_>, q: &ExpiryQuery) -> AppResult<Vec<ExpiryRow>> {
    ctx.require("stock.view")?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let days = q
        .days
        .unwrap_or(settings.inventory.expiry_alert_days)
        .clamp(0, 3650);
    let horizon = add_days(&today, days);
    let mut stmt = ctx.conn.prepare(
        "SELECT l.variant_id, b.id, l.warehouse_id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, b.batch_no, b.expiry_date,
                w.name, l.qty, l.value
         FROM stock_levels l JOIN batches b ON b.id = l.batch_id JOIN product_variants v ON v.id = l.variant_id
         JOIN products p ON p.id = v.product_id JOIN warehouses w ON w.id = l.warehouse_id
         WHERE l.qty > 0 AND b.expiry_date IS NOT NULL AND b.expiry_date <= ?1 AND (?2 IS NULL OR l.warehouse_id = ?2)
         ORDER BY b.expiry_date, lower(p.name) LIMIT 5000",
    )?;
    let rows = stmt
        .query_map(params![horizon, q.warehouse_id], |r| {
            let expiry: String = r.get(6)?;
            Ok(ExpiryRow {
                variant_id: r.get(0)?,
                batch_id: r.get(1)?,
                warehouse_id: r.get(2)?,
                name: r.get(3)?,
                sku: r.get(4)?,
                batch_no: r.get(5)?,
                days_left: days_between(&today, &expiry).unwrap_or(0),
                expired: expiry.as_str() < today.as_str(),
                expiry_date: expiry,
                warehouse_name: r.get(7)?,
                qty: r.get(8)?,
                value: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ------------------------------------------------------------------ valuation switch

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuationRow {
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub warehouse_name: String,
    pub batch_no: Option<String>,
    pub qty: i64,
    pub value: i64,
    pub unit_cost: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuationPreview {
    /// "weighted_average" or "fifo".
    pub current: String,
    pub target: String,
    /// What the switch will create (to FIFO: one opening layer per row) or set (to average).
    pub rows: Vec<ValuationRow>,
    pub total_qty: i64,
    pub total_value: i64,
    /// Items that stop the switch (negative stock), in words.
    pub problems: Vec<String>,
}

fn method_name(v: Valuation) -> &'static str {
    match v {
        Valuation::WeightedAverage => "weighted_average",
        Valuation::Fifo => "fifo",
    }
}

fn preview_of(conn: &Connection) -> AppResult<ValuationPreview> {
    let current = stock_posting::valuation(conn)?;
    let target = match current {
        Valuation::WeightedAverage => Valuation::Fifo,
        Valuation::Fifo => Valuation::WeightedAverage,
    };
    let mut stmt = conn.prepare(
        "SELECT l.variant_id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, w.name, b.batch_no, l.qty, l.value
         FROM stock_levels l JOIN product_variants v ON v.id = l.variant_id JOIN products p ON p.id = v.product_id
         JOIN warehouses w ON w.id = l.warehouse_id LEFT JOIN batches b ON b.id = l.batch_id
         WHERE l.qty <> 0 OR l.value <> 0 ORDER BY lower(p.name), w.name, b.expiry_date",
    )?;
    let all: Vec<ValuationRow> = stmt
        .query_map([], |r| {
            let qty: i64 = r.get(5)?;
            let value: i64 = r.get(6)?;
            Ok(ValuationRow {
                variant_id: r.get(0)?,
                name: r.get(1)?,
                sku: r.get(2)?,
                warehouse_name: r.get(3)?,
                batch_no: r.get(4)?,
                qty,
                value,
                unit_cost: stock_posting::per_unit(value, qty),
            })
        })?
        .collect::<Result<_, _>>()?;
    let problems = all
        .iter()
        .filter(|r| r.qty < 0)
        .map(|r| {
            format!(
                "{} ({}) is below zero in {}: count it first.",
                r.name, r.sku, r.warehouse_name
            )
        })
        .collect();
    let rows: Vec<ValuationRow> = all.into_iter().filter(|r| r.qty > 0).collect();
    Ok(ValuationPreview {
        current: method_name(current).into(),
        target: method_name(target).into(),
        total_qty: rows.iter().map(|r| r.qty).sum(),
        total_value: rows.iter().map(|r| r.value).sum(),
        rows,
        problems,
    })
}

/// What switching the valuation method would do (nothing is changed).
pub fn valuation_preview(ctx: &Ctx<'_>) -> AppResult<ValuationPreview> {
    ctx.require("inventory.valuation")?;
    preview_of(ctx.conn)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuationSwitchInput {
    /// "fifo" or "weighted_average": must be the other method.
    pub target: String,
    /// The total value the preview showed; refused if stock moved since.
    pub expected_total_value: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValuationSwitchResult {
    pub method: String,
    pub layers_created: i64,
    pub items_repriced: i64,
    pub total_value: i64,
}

/// Switches the valuation method once, in one transaction, without changing the books:
/// * to FIFO: every stock row becomes one opening layer at its current value (the average cost),
///   used before anything received later;
/// * to weighted average: open layers are closed and each product's cost becomes its stock value
///   divided by its quantity.
pub fn valuation_switch(
    ctx: &mut Ctx<'_>,
    input: &ValuationSwitchInput,
) -> AppResult<ValuationSwitchResult> {
    ctx.require("inventory.valuation")?;
    let reason = super::common::required("reason", &input.reason, 300)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let preview = preview_of(tx)?;
        if input.target != preview.target {
            return Err(AppError::field(
                "target",
                format!("The method is already {}.", preview.current),
            ));
        }
        if !preview.problems.is_empty() {
            return Err(AppError::validation(preview.problems.join(" ")));
        }
        if preview.total_value != input.expected_total_value {
            return Err(AppError::conflict(
                "Stock changed since the preview. Look at the preview again before switching.",
            ));
        }
        let mut layers_created = 0;
        let mut items_repriced = 0;
        if input.target == "fifo" {
            // Close anything left from an earlier FIFO period, then one opening layer per row.
            tx.execute("UPDATE cost_layers SET qty_left = 0, value_left = 0 WHERE qty_left > 0 OR value_left > 0", [])?;
            let today: String = settings::load(tx)?.business_date(ctx.now);
            let mut stmt = tx.prepare(
                "SELECT variant_id, warehouse_id, batch_key, qty, value FROM stock_levels WHERE qty > 0",
            )?;
            let rows: Vec<(String, String, String, i64, i64)> = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<Result<_, _>>()?;
            drop(stmt);
            for (variant, warehouse, batch_key, qty, value) in rows {
                tx.execute(
                    "INSERT INTO cost_layers (id, variant_id, warehouse_id, batch_key, movement_id, received_at, business_date, qty_in,
                        qty_left, unit_cost, value_left, created_at)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?7, ?8, ?9, ?5)",
                    params![new_id(), variant, warehouse, batch_key, now, today, qty, stock_posting::per_unit(value, qty), value.max(0)],
                )?;
                layers_created += 1;
            }
        } else {
            tx.execute("UPDATE cost_layers SET qty_left = 0, value_left = 0 WHERE qty_left > 0 OR value_left > 0", [])?;
            items_repriced = i64::try_from(tx.execute(
                "UPDATE product_variants SET cost_price = (
                   SELECT (sum(value) * 1000 + sum(qty) / 2) / sum(qty) FROM stock_levels WHERE variant_id = product_variants.id AND qty > 0),
                   updated_at = ?1
                 WHERE id IN (SELECT variant_id FROM stock_levels WHERE qty > 0 GROUP BY variant_id HAVING sum(qty) > 0)",
                [&now],
            )?)
            .unwrap_or(0);
        }
        let mut stored = settings::load(tx)?.inventory;
        stored.valuation_method = input.target.clone();
        settings::store(
            tx,
            "inventory",
            &serde_json::to_value(&stored)?,
            Some(&actor.user_id),
            &now,
        )?;
        AuditEntry::new("VALUATION_SWITCHED", "settings", Some("inventory"))
            .by(actor)
            .summary(format!(
                "{} → {}: {reason}",
                preview.current, preview.target
            ))
            .before(json!({ "valuationMethod": preview.current }))
            .after(json!({
                "valuationMethod": preview.target,
                "totalValue": preview.total_value,
                "layersCreated": layers_created,
                "itemsRepriced": items_repriced,
            }))
            .write(tx, &now)?;
        Ok(ValuationSwitchResult {
            method: input.target.clone(),
            layers_created,
            items_repriced,
            total_value: preview.total_value,
        })
    })
}

// ------------------------------------------------------------------ unit conversions

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnitConversion {
    pub id: String,
    pub from_unit_id: String,
    pub from_unit_code: String,
    pub to_unit_id: String,
    pub to_unit_code: String,
    /// 1 from-unit = factor_num / factor_den to-units.
    pub factor_num: i64,
    pub factor_den: i64,
}

/// Conversions that apply to every product (a product's own pack sizes are on the product).
pub fn conversions(ctx: &Ctx<'_>) -> AppResult<Vec<UnitConversion>> {
    ctx.require("products.view")?;
    let mut stmt = ctx.conn.prepare(
        "SELECT c.id, c.from_unit_id, f.code, c.to_unit_id, t.code, c.factor_num, c.factor_den
         FROM unit_conversions c JOIN units f ON f.id = c.from_unit_id JOIN units t ON t.id = c.to_unit_id
         WHERE c.product_id IS NULL AND c.deleted_at IS NULL ORDER BY lower(f.code), lower(t.code)",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UnitConversion {
                id: r.get(0)?,
                from_unit_id: r.get(1)?,
                from_unit_code: r.get(2)?,
                to_unit_id: r.get(3)?,
                to_unit_code: r.get(4)?,
                factor_num: r.get(5)?,
                factor_den: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnitConversionInput {
    pub id: Option<String>,
    pub from_unit_id: String,
    pub to_unit_id: String,
    pub factor_num: i64,
    pub factor_den: i64,
}

pub fn save_conversion(
    ctx: &mut Ctx<'_>,
    input: &UnitConversionInput,
) -> AppResult<Vec<UnitConversion>> {
    ctx.require("products.manage")?;
    if input.from_unit_id == input.to_unit_id {
        return Err(AppError::field("toUnitId", "Choose two different units."));
    }
    if input.factor_num <= 0
        || input.factor_den <= 0
        || input.factor_num > 1_000_000_000
        || input.factor_den > 1_000_000_000
    {
        return Err(AppError::field("factorNum", "Enter a positive conversion."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        for (field, unit) in [
            ("fromUnitId", &input.from_unit_id),
            ("toUnitId", &input.to_unit_id),
        ] {
            let ok: i64 = tx.query_row(
                "SELECT count(*) FROM units WHERE id = ?1 AND deleted_at IS NULL",
                [unit],
                |r| r.get(0),
            )?;
            if ok == 0 {
                return Err(AppError::field(field, "Choose an existing unit."));
            }
        }
        let id = input.id.clone().unwrap_or_else(new_id);
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM unit_conversions WHERE id = ?1 AND product_id IS NULL AND deleted_at IS NULL",
                [&id],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            Some(_) => {
                tx.execute(
                    "UPDATE unit_conversions SET from_unit_id = ?1, to_unit_id = ?2, factor_num = ?3, factor_den = ?4, updated_at = ?5, updated_by = ?6 WHERE id = ?7",
                    params![input.from_unit_id, input.to_unit_id, input.factor_num, input.factor_den, now, actor.user_id, id],
                )?;
            }
            None if input.id.is_some() => return Err(AppError::not_found("That conversion")),
            None => {
                tx.execute(
                    "INSERT INTO unit_conversions (id, product_id, from_unit_id, to_unit_id, factor_num, factor_den, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7)",
                    params![id, input.from_unit_id, input.to_unit_id, input.factor_num, input.factor_den, now, actor.user_id],
                )
                .map_err(|e| match AppError::from(e) {
                    err if err.code == crate::error::ErrorCode::Conflict => {
                        AppError::field("fromUnitId", "These two units already have a conversion.")
                    }
                    err => err,
                })?;
            }
        }
        AuditEntry::new("UNIT_CONVERSION_SAVED", "unit_conversions", Some(&id))
            .by(actor)
            .after(json!({ "from": input.from_unit_id, "to": input.to_unit_id, "num": input.factor_num, "den": input.factor_den }))
            .write(tx, &now)
    })?;
    conversions(ctx)
}

pub fn delete_conversion(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<Vec<UnitConversion>> {
    ctx.require("products.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let changed = tx.execute(
            "UPDATE unit_conversions SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3 AND product_id IS NULL AND deleted_at IS NULL",
            params![now, actor.user_id, input.id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("That conversion"));
        }
        AuditEntry::new(
            "UNIT_CONVERSION_DELETED",
            "unit_conversions",
            Some(&input.id),
        )
        .by(actor)
        .write(tx, &now)
    })?;
    conversions(ctx)
}

// ------------------------------------------------------------------ write-off

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteOffInput {
    pub idempotency_key: String,
    pub variant_id: String,
    pub batch_id: String,
    pub warehouse_id: Option<String>,
    pub notes: Option<String>,
}

/// Writes off everything left of a batch in a store (expired or damaged), as a stock adjustment.
pub fn write_off_batch(
    ctx: &mut Ctx<'_>,
    input: &WriteOffInput,
) -> AppResult<super::stock::StockDocResult> {
    let warehouse = match &input.warehouse_id {
        Some(w) => w.clone(),
        None => super::setup::default_location(ctx.conn)?.1,
    };
    let qty = stock_posting::available(
        ctx.conn,
        &input.variant_id,
        &warehouse,
        Some(&input.batch_id),
    )?;
    if qty <= 0 {
        return Err(AppError::conflict(
            "Nothing of this batch is left in the store.",
        ));
    }
    let batch_no: String = ctx.conn.query_row(
        "SELECT batch_no FROM batches WHERE id = ?1 AND variant_id = ?2",
        params![input.batch_id, input.variant_id],
        |r| r.get(0),
    )?;
    let notes = optional("notes", input.notes.as_deref(), 1000)?;
    super::stock::adjust(
        ctx,
        &super::stock::AdjustInput {
            idempotency_key: input.idempotency_key.clone(),
            warehouse_id: Some(warehouse),
            reason: format!("Expired / written off: batch {batch_no}"),
            notes,
            lines: vec![super::stock::AdjustLine {
                variant_id: input.variant_id.clone(),
                batch_id: Some(input.batch_id.clone()),
                batch_no: None,
                expiry_date: None,
                qty_change: -qty,
                unit_cost: None,
            }],
        },
    )
}
