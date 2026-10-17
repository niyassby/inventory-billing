//! The only way stock changes: one `stock_movements` row per change. Outbound movements obey the
//! negative-stock policy; inbound purchases update the weighted-average cost.

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::settings::NegativeStockPolicy;
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::error::{AppError, AppResult, ErrorCode};

pub struct Move<'a> {
    pub warehouse_id: &'a str,
    pub variant_id: &'a str,
    pub batch_id: Option<&'a str>,
    /// Signed, thousandths of the base unit.
    pub qty: i64,
    /// Cost per base unit (minor units).
    pub unit_cost: i64,
    pub kind: &'a str,
    pub source_type: &'a str,
    pub source_id: &'a str,
    pub source_line_id: Option<&'a str>,
    pub reversal_of: Option<&'a str>,
    pub note: Option<&'a str>,
    /// Inbound purchases recompute the variant's average cost.
    pub update_average_cost: bool,
}

/// A movement allowed under the "warn" policy that took stock below zero.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StockWarning {
    pub variant_id: String,
    pub name: String,
    pub available: i64,
    pub requested: i64,
}

pub fn available(
    conn: &Connection,
    variant_id: &str,
    warehouse_id: &str,
    batch_id: Option<&str>,
) -> AppResult<i64> {
    Ok(conn
        .query_row(
            "SELECT qty FROM stock_levels WHERE variant_id = ?1 AND warehouse_id = ?2 AND batch_key = ?3",
            params![variant_id, warehouse_id, batch_id.unwrap_or("")],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

fn variant_name(conn: &Connection, variant_id: &str) -> AppResult<String> {
    Ok(conn.query_row(
        "SELECT p.name || coalesce(' ' || v.variant_name, '') FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = ?1",
        [variant_id],
        |r| r.get(0),
    )?)
}

pub fn post(
    conn: &Connection,
    policy: NegativeStockPolicy,
    business_date: &str,
    now: &str,
    user_id: Option<&str>,
    mv: &Move<'_>,
) -> AppResult<Option<StockWarning>> {
    if mv.qty == 0 {
        return Ok(None);
    }
    let mut warning = None;
    if mv.qty < 0 && mv.kind != "void_reversal" {
        let on_hand = available(conn, mv.variant_id, mv.warehouse_id, mv.batch_id)?;
        if on_hand + mv.qty < 0 {
            let name = variant_name(conn, mv.variant_id)?;
            match policy {
                NegativeStockPolicy::Block => {
                    return Err(AppError::new(
                        ErrorCode::InsufficientStock,
                        format!("Not enough stock of {name}: {} available.", on_hand as f64 / 1000.0),
                    )
                    .with_details(json!({ "variantId": mv.variant_id, "name": name, "available": on_hand, "requested": -mv.qty })));
                }
                NegativeStockPolicy::Warn => {
                    warning = Some(StockWarning {
                        variant_id: mv.variant_id.into(),
                        name,
                        available: on_hand,
                        requested: -mv.qty,
                    });
                }
                NegativeStockPolicy::Allow => {}
            }
        }
    }

    if mv.update_average_cost && mv.qty > 0 {
        let (on_hand, cost): (i64, i64) = conn.query_row(
            "SELECT coalesce((SELECT sum(qty) FROM stock_levels WHERE variant_id = ?1), 0), cost_price
             FROM product_variants WHERE id = ?1",
            [mv.variant_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let new_cost = if on_hand <= 0 {
            mv.unit_cost
        } else {
            i64::try_from(money::div_round(
                i128::from(on_hand) * i128::from(cost)
                    + i128::from(mv.qty) * i128::from(mv.unit_cost),
                i128::from(on_hand + mv.qty),
                RoundingMode::HalfUp,
            ))
            .map_err(AppError::internal)?
        };
        conn.execute(
            "UPDATE product_variants SET cost_price = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_cost, now, mv.variant_id],
        )?;
    }

    let value = money::multiply_by_qty(mv.unit_cost, mv.qty, RoundingMode::HalfUp);
    conn.execute(
        "INSERT INTO stock_movements (id, business_date, occurred_at, warehouse_id, variant_id, batch_id, movement_type,
            qty, unit_cost, value, source_type, source_id, source_line_id, reversal_of, note, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?3, ?16)",
        params![
            new_id(),
            business_date,
            now,
            mv.warehouse_id,
            mv.variant_id,
            mv.batch_id,
            mv.kind,
            mv.qty,
            mv.unit_cost,
            value,
            mv.source_type,
            mv.source_id,
            mv.source_line_id,
            mv.reversal_of,
            mv.note,
            user_id
        ],
    )?;
    Ok(warning)
}

/// Posts the opposite of every movement of a source document (voids). Never blocked by the
/// negative-stock policy: undoing a receipt that was already sold must still be possible, and the
/// resulting negative stock shows up in reports.
pub fn reverse_source(
    conn: &Connection,
    source_type: &str,
    source_id: &str,
    business_date: &str,
    now: &str,
    user_id: Option<&str>,
) -> AppResult<()> {
    let rows: Vec<(String, String, String, Option<String>, i64, i64)> = conn
        .prepare(
            "SELECT id, warehouse_id, variant_id, batch_id, qty, unit_cost FROM stock_movements
             WHERE source_type = ?1 AND source_id = ?2 AND reversal_of IS NULL",
        )?
        .query_map(params![source_type, source_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    for (id, warehouse_id, variant_id, batch_id, qty, unit_cost) in rows {
        post(
            conn,
            NegativeStockPolicy::Allow,
            business_date,
            now,
            user_id,
            &Move {
                warehouse_id: &warehouse_id,
                variant_id: &variant_id,
                batch_id: batch_id.as_deref(),
                qty: -qty,
                unit_cost,
                kind: "void_reversal",
                source_type,
                source_id,
                source_line_id: None,
                reversal_of: Some(&id),
                note: Some("Voided"),
                update_average_cost: false,
            },
        )?;
    }
    Ok(())
}
