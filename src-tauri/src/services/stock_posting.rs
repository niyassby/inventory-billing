//! The only way stock changes: one `stock_movements` row per change. Outbound movements obey the
//! negative-stock policy; inbound purchases update the weighted-average cost.
//!
//! Valuation (`inventory.valuationMethod`):
//! * weighted average: an outbound movement is valued at the variant's average cost;
//! * FIFO: inbound movements open cost layers (per variant, store and batch) and outbound ones use
//!   the oldest layers first; the movement's value is exactly what the layers gave up, so the
//!   layers always add up to the ledger (`v_cost_layer_mismatches`).
//!
//! Every caller posts the returned `value` to the books (never qty × cost again), so the
//! Inventory account, the stock ledger and the layers agree under either method.
//!
//! Batch-tracked products: an outbound movement without a batch is split over batches by
//! first expiry, first out (`post_out`), skipping expired batches as the expiry policy says.

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::settings::{ExpiredStockPolicy, NegativeStockPolicy};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::error::{AppError, AppResult, ErrorCode};

/// One slice of cost taken from (or given to) a FIFO layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub qty: i64,
    pub unit_cost: i64,
    pub value: i64,
    pub received_at: String,
    pub business_date: String,
}

#[derive(Debug, Clone, Default)]
pub struct Move<'a> {
    pub warehouse_id: &'a str,
    pub variant_id: &'a str,
    pub batch_id: Option<&'a str>,
    /// Signed, thousandths of the base unit.
    pub qty: i64,
    /// Cost per base unit (minor units). Outbound under FIFO: only the fallback for stock the
    /// layers don't cover.
    pub unit_cost: i64,
    pub kind: &'a str,
    pub source_type: &'a str,
    pub source_id: &'a str,
    pub source_line_id: Option<&'a str>,
    pub reversal_of: Option<&'a str>,
    pub note: Option<&'a str>,
    /// Inbound purchases recompute the variant's average cost.
    pub update_average_cost: bool,
    /// Inbound under FIFO: the layers to recreate (a transfer receiving what the other store gave
    /// up). The movement's value is their sum.
    pub layers: Option<&'a [Piece]>,
}

/// A movement allowed under the "warn" policy that took stock below zero, or sold an expired batch.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StockWarning {
    pub variant_id: String,
    pub name: String,
    pub available: i64,
    pub requested: i64,
    /// Set when the warning is about an expired batch (its batch number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expired_batch: Option<String>,
}

/// What a posted movement did.
#[derive(Debug, Clone)]
pub struct Posted {
    pub movement_id: String,
    pub batch_id: Option<String>,
    pub qty: i64,
    /// Signed value of the movement (what the books must post).
    pub value: i64,
    /// Cost per base unit as recorded on the movement.
    pub unit_cost: i64,
    pub warning: Option<StockWarning>,
    /// FIFO outbound: the layers taken (for a transfer to recreate them elsewhere).
    pub pieces: Vec<Piece>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Valuation {
    WeightedAverage,
    Fifo,
}

/// The valuation method in force (read straight from the stored settings).
pub fn valuation(conn: &Connection) -> AppResult<Valuation> {
    let method: Option<String> = conn
        .query_row(
            "SELECT json_extract(value_json, '$.valuationMethod') FROM settings WHERE key = 'inventory'",
            [],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(if method.as_deref() == Some("fifo") {
        Valuation::Fifo
    } else {
        Valuation::WeightedAverage
    })
}

/// Cost per base unit of a value over a quantity in thousandths.
pub fn per_unit(value: i64, qty: i64) -> i64 {
    if qty == 0 {
        return 0;
    }
    i64::try_from(money::div_round(
        i128::from(value.abs()) * 1000,
        i128::from(qty.abs()),
        RoundingMode::HalfUp,
    ))
    .unwrap_or(i64::MAX)
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

fn insufficient(name: &str, variant_id: &str, on_hand: i64, requested: i64) -> AppError {
    AppError::new(
        ErrorCode::InsufficientStock,
        format!("Not enough stock of {name}: {} available.", on_hand as f64 / 1000.0),
    )
    .with_details(json!({ "variantId": variant_id, "name": name, "available": on_hand, "requested": requested }))
}

struct OpenLayer {
    id: String,
    qty_left: i64,
    value_left: i64,
    unit_cost: i64,
    received_at: String,
    business_date: String,
}

fn open_layers(conn: &Connection, mv: &Move<'_>) -> AppResult<Vec<OpenLayer>> {
    // A reversal takes back its own receipt's layer first.
    let own = mv.reversal_of.unwrap_or("");
    let mut stmt = conn.prepare(
        "SELECT id, qty_left, value_left, unit_cost, received_at, business_date FROM cost_layers
         WHERE variant_id = ?1 AND warehouse_id = ?2 AND batch_key = ?3 AND qty_left > 0
         ORDER BY coalesce(movement_id, '') = ?4 DESC, received_at, created_at, id",
    )?;
    let rows = stmt
        .query_map(
            params![
                mv.variant_id,
                mv.warehouse_id,
                mv.batch_id.unwrap_or(""),
                own
            ],
            |r| {
                Ok(OpenLayer {
                    id: r.get(0)?,
                    qty_left: r.get(1)?,
                    value_left: r.get(2)?,
                    unit_cost: r.get(3)?,
                    received_at: r.get(4)?,
                    business_date: r.get(5)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Takes `qty` (positive) from the layers, oldest first. Returns the slices with their layer ids;
/// a shortfall (stock below zero under "warn"/"allow") is valued at `fallback_cost`.
fn take_layers(
    conn: &Connection,
    mv: &Move<'_>,
    qty: i64,
    fallback_cost: i64,
) -> AppResult<Vec<(Option<String>, Piece)>> {
    let mut left = qty;
    let mut taken = Vec::new();
    for layer in open_layers(conn, mv)? {
        if left == 0 {
            break;
        }
        let q = left.min(layer.qty_left);
        let value = if q == layer.qty_left {
            layer.value_left
        } else {
            money::multiply_by_qty(layer.unit_cost, q, RoundingMode::HalfUp).min(layer.value_left)
        };
        taken.push((
            Some(layer.id),
            Piece {
                qty: q,
                unit_cost: layer.unit_cost,
                value,
                received_at: layer.received_at,
                business_date: layer.business_date,
            },
        ));
        left -= q;
    }
    if left > 0 {
        taken.push((
            None,
            Piece {
                qty: left,
                unit_cost: fallback_cost,
                value: money::multiply_by_qty(fallback_cost, left, RoundingMode::HalfUp),
                received_at: String::new(),
                business_date: String::new(),
            },
        ));
    }
    Ok(taken)
}

#[allow(clippy::too_many_arguments, reason = "one layer row")]
fn insert_layer(
    conn: &Connection,
    mv: &Move<'_>,
    movement_id: &str,
    qty: i64,
    unit_cost: i64,
    value: i64,
    received_at: &str,
    business_date: &str,
    now: &str,
) -> AppResult<()> {
    if qty <= 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO cost_layers (id, variant_id, warehouse_id, batch_key, movement_id, received_at, business_date, qty_in,
            qty_left, unit_cost, value_left, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11)",
        params![
            new_id(),
            mv.variant_id,
            mv.warehouse_id,
            mv.batch_id.unwrap_or(""),
            movement_id,
            received_at,
            business_date,
            qty,
            unit_cost,
            value.max(0),
            now
        ],
    )?;
    Ok(())
}

pub fn post(
    conn: &Connection,
    policy: NegativeStockPolicy,
    business_date: &str,
    now: &str,
    user_id: Option<&str>,
    mv: &Move<'_>,
) -> AppResult<Posted> {
    if mv.qty == 0 {
        return Ok(Posted {
            movement_id: String::new(),
            batch_id: mv.batch_id.map(String::from),
            qty: 0,
            value: 0,
            unit_cost: mv.unit_cost,
            warning: None,
            pieces: Vec::new(),
        });
    }
    let mut warning = None;
    if mv.qty < 0 && mv.kind != "void_reversal" {
        let on_hand = available(conn, mv.variant_id, mv.warehouse_id, mv.batch_id)?;
        if on_hand + mv.qty < 0 {
            let name = variant_name(conn, mv.variant_id)?;
            match policy {
                NegativeStockPolicy::Block => {
                    return Err(insufficient(&name, mv.variant_id, on_hand, -mv.qty));
                }
                NegativeStockPolicy::Warn => {
                    warning = Some(StockWarning {
                        variant_id: mv.variant_id.into(),
                        name,
                        available: on_hand,
                        requested: -mv.qty,
                        expired_batch: None,
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

    let fifo = valuation(conn)? == Valuation::Fifo;
    // A void reversal undoes the original movement at exactly its value, so the books (which
    // reverse the original journal) and the ledger agree.
    let forced = if mv.kind == "void_reversal" {
        mv.reversal_of
            .map(|id| {
                conn.query_row(
                    "SELECT value FROM stock_movements WHERE id = ?1",
                    [id],
                    |r| r.get::<_, i64>(0),
                )
            })
            .transpose()?
            .map(|v| -v)
    } else {
        None
    };
    let mut taken = Vec::new();
    let (value, unit_cost) = if mv.qty > 0 {
        match (fifo, mv.layers) {
            (true, Some(layers)) => {
                let value: i64 = layers.iter().map(|p| p.value).sum();
                (value, per_unit(value, mv.qty))
            }
            _ => {
                let value = forced.unwrap_or_else(|| {
                    money::multiply_by_qty(mv.unit_cost, mv.qty, RoundingMode::HalfUp)
                });
                (value, mv.unit_cost)
            }
        }
    } else if fifo {
        taken = take_layers(conn, mv, -mv.qty, mv.unit_cost)?;
        let used: i64 = taken.iter().map(|(_, p)| p.value).sum();
        let value = forced.unwrap_or(-used);
        (value, per_unit(value, mv.qty))
    } else {
        let value = forced
            .unwrap_or_else(|| money::multiply_by_qty(mv.unit_cost, mv.qty, RoundingMode::HalfUp));
        (value, mv.unit_cost)
    };

    let movement_id = new_id();
    conn.execute(
        "INSERT INTO stock_movements (id, business_date, occurred_at, warehouse_id, variant_id, batch_id, movement_type,
            qty, unit_cost, value, source_type, source_id, source_line_id, reversal_of, note, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?3, ?16)",
        params![
            movement_id,
            business_date,
            now,
            mv.warehouse_id,
            mv.variant_id,
            mv.batch_id,
            mv.kind,
            mv.qty,
            unit_cost,
            value,
            mv.source_type,
            mv.source_id,
            mv.source_line_id,
            mv.reversal_of,
            mv.note,
            user_id
        ],
    )?;

    let mut pieces = Vec::new();
    if fifo {
        if mv.qty > 0 {
            match mv.layers {
                Some(layers) => {
                    for p in layers {
                        let received = if p.received_at.is_empty() {
                            now
                        } else {
                            &p.received_at
                        };
                        let day = if p.business_date.is_empty() {
                            business_date
                        } else {
                            &p.business_date
                        };
                        insert_layer(
                            conn,
                            mv,
                            &movement_id,
                            p.qty,
                            p.unit_cost,
                            p.value,
                            received,
                            day,
                            now,
                        )?;
                    }
                }
                None => insert_layer(
                    conn,
                    mv,
                    &movement_id,
                    mv.qty,
                    unit_cost,
                    value,
                    now,
                    business_date,
                    now,
                )?,
            }
        } else {
            let mut gap = value + taken.iter().map(|(_, p)| p.value).sum::<i64>();
            for (layer_id, piece) in &taken {
                if let Some(layer_id) = layer_id {
                    conn.execute(
                        "INSERT INTO cost_layer_uses (id, layer_id, movement_id, qty, value, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![new_id(), layer_id, movement_id, piece.qty, piece.value, now],
                    )?;
                    conn.execute(
                        "UPDATE cost_layers SET qty_left = qty_left - ?1, value_left = value_left - ?2 WHERE id = ?3",
                        params![piece.qty, piece.value, layer_id],
                    )?;
                }
                pieces.push(piece.clone());
            }
            // A forced value (void reversal) that differs from what the layers gave up: the
            // difference stays with the oldest remaining layer, so layers and ledger still agree.
            if gap != 0 {
                if let Some(oldest) = open_layers(
                    conn,
                    &Move {
                        reversal_of: None,
                        ..mv.clone()
                    },
                )?
                .first()
                {
                    let new_value = oldest.value_left + gap;
                    if new_value >= 0 {
                        conn.execute(
                            "UPDATE cost_layers SET value_left = ?1 WHERE id = ?2",
                            params![new_value, oldest.id],
                        )?;
                        gap = 0;
                    }
                }
                if gap != 0 {
                    tracing::warn!(
                        variant = mv.variant_id,
                        gap,
                        "FIFO layers differ from the ledger after a void"
                    );
                }
            }
        }
    }

    Ok(Posted {
        movement_id,
        batch_id: mv.batch_id.map(String::from),
        qty: mv.qty,
        value,
        unit_cost,
        warning,
        pieces,
    })
}

// ------------------------------------------------------------------ batches

/// Finds a product's batch by number (any case) or creates it. `field` prefixes error fields
/// ("lines.0"). None when no batch number is given.
#[allow(clippy::too_many_arguments, reason = "the batch's own fields")]
pub fn batch_for(
    conn: &Connection,
    variant_id: &str,
    field: &str,
    batch_no: Option<&str>,
    mfg_date: Option<&str>,
    expiry_date: Option<&str>,
    now: &str,
    user: &str,
) -> AppResult<Option<String>> {
    let Some(batch_no) = super::common::optional(&format!("{field}.batchNo"), batch_no, 40)? else {
        return Ok(None);
    };
    let expiry = expiry_date
        .filter(|d| !d.trim().is_empty())
        .map(|d| super::common::date(&format!("{field}.expiryDate"), d))
        .transpose()?;
    let mfg = mfg_date
        .filter(|d| !d.trim().is_empty())
        .map(|d| super::common::date(&format!("{field}.mfgDate"), d))
        .transpose()?;
    if let Some((id, known)) = conn
        .query_row(
            "SELECT id, expiry_date FROM batches WHERE variant_id = ?1 AND lower(batch_no) = lower(?2) AND deleted_at IS NULL",
            params![variant_id, batch_no],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        // A batch keeps its first expiry; a different one for the same batch number is a mistake.
        if let (Some(known), Some(given)) = (known.as_deref(), expiry.as_deref()) {
            if known != given {
                return Err(AppError::field(
                    &format!("{field}.expiryDate"),
                    format!("Batch {batch_no} was received before with expiry {known}."),
                ));
            }
        }
        if known.is_none() && expiry.is_some() {
            conn.execute(
                "UPDATE batches SET expiry_date = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
                params![expiry, now, user, id],
            )?;
        }
        return Ok(Some(id));
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO batches (id, variant_id, batch_no, mfg_date, expiry_date, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7)",
        params![id, variant_id, batch_no, mfg, expiry, now, user],
    )?;
    Ok(Some(id))
}

/// A batch's number and expiry.
fn batch_info(conn: &Connection, batch_id: &str) -> AppResult<(String, Option<String>)> {
    Ok(conn.query_row(
        "SELECT batch_no, expiry_date FROM batches WHERE id = ?1",
        [batch_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

pub fn tracks_batches(conn: &Connection, variant_id: &str) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT p.track_batches FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = ?1",
            [variant_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    /// None: stock of the product that has no batch.
    pub batch_id: Option<String>,
    pub batch_no: Option<String>,
    pub qty: i64,
    pub expired: bool,
}

/// Splits `qty` (positive, base units) over a variant's batches in a store: earliest expiry
/// first, batches without expiry after them, stock without a batch last. Expired batches follow
/// the expiry policy. When there isn't enough, "block" refuses; otherwise the rest goes on the
/// last batch used and the negative-stock policy decides.
#[allow(
    clippy::too_many_arguments,
    reason = "the allocation needs both policies and the day"
)]
pub fn allocate_fefo(
    conn: &Connection,
    variant_id: &str,
    warehouse_id: &str,
    qty: i64,
    today: &str,
    expired_policy: ExpiredStockPolicy,
    negative_policy: NegativeStockPolicy,
) -> AppResult<Vec<Allocation>> {
    let mut stmt = conn.prepare(
        "SELECT l.batch_id, b.batch_no, b.expiry_date, l.qty FROM stock_levels l
         LEFT JOIN batches b ON b.id = l.batch_id
         WHERE l.variant_id = ?1 AND l.warehouse_id = ?2 AND l.qty > 0
         ORDER BY l.batch_id IS NULL, b.expiry_date IS NULL, b.expiry_date, b.created_at, l.batch_key",
    )?;
    let rows: Vec<(Option<String>, Option<String>, Option<String>, i64)> = stmt
        .query_map(params![variant_id, warehouse_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let is_expired = |expiry: &Option<String>| expiry.as_deref().is_some_and(|e| e < today);
    // Unexpired first; expired batches only as the policy allows.
    let (fresh, expired): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| !is_expired(&r.2));
    let candidates: Vec<_> = match expired_policy {
        ExpiredStockPolicy::Block => fresh,
        ExpiredStockPolicy::Warn => fresh.into_iter().chain(expired).collect(),
        ExpiredStockPolicy::Allow => {
            let mut all: Vec<_> = fresh.into_iter().chain(expired).collect();
            // Pure first-expiry-first-out: expired stock goes first.
            all.sort_by(|a, b| {
                (a.0.is_none(), a.2.is_none(), &a.2).cmp(&(b.0.is_none(), b.2.is_none(), &b.2))
            });
            all
        }
    };
    let total: i64 = candidates.iter().map(|c| c.3).sum();
    if total < qty && negative_policy == NegativeStockPolicy::Block {
        let name = variant_name(conn, variant_id)?;
        return Err(insufficient(&name, variant_id, total, qty));
    }
    let mut left = qty;
    let mut out = Vec::new();
    for (batch_id, batch_no, expiry, on_hand) in candidates {
        if left == 0 {
            break;
        }
        let q = left.min(on_hand);
        out.push(Allocation {
            expired: is_expired(&expiry),
            batch_id,
            batch_no,
            qty: q,
        });
        left -= q;
    }
    if left > 0 {
        match out.last_mut() {
            Some(last) => last.qty += left,
            None => out.push(Allocation {
                batch_id: None,
                batch_no: None,
                qty: left,
                expired: false,
            }),
        }
    }
    Ok(out)
}

/// Whether an explicitly chosen batch may go out today; Some(warning) under "warn".
pub fn check_expiry(
    conn: &Connection,
    variant_id: &str,
    batch_id: &str,
    qty: i64,
    today: &str,
    policy: ExpiredStockPolicy,
) -> AppResult<Option<StockWarning>> {
    let (batch_no, expiry) = batch_info(conn, batch_id)?;
    let Some(expiry) = expiry.filter(|e| e.as_str() < today) else {
        return Ok(None);
    };
    match policy {
        ExpiredStockPolicy::Allow => Ok(None),
        ExpiredStockPolicy::Block => Err(AppError::new(
            ErrorCode::Validation,
            format!("Batch {batch_no} expired on {expiry} and can't be sold or issued."),
        )
        .with_details(json!({ "batchId": batch_id, "expiryDate": expiry }))),
        ExpiredStockPolicy::Warn => Ok(Some(StockWarning {
            variant_id: variant_id.into(),
            name: variant_name(conn, variant_id)?,
            available: 0,
            requested: qty,
            expired_batch: Some(batch_no),
        })),
    }
}

/// An outbound movement for a product: batch-tracked products without a chosen batch are split
/// by first expiry (one movement per batch); a chosen batch is checked against its expiry.
#[allow(clippy::too_many_arguments, reason = "both stock policies and the day")]
pub fn post_out(
    conn: &Connection,
    negative_policy: NegativeStockPolicy,
    expired_policy: ExpiredStockPolicy,
    business_date: &str,
    now: &str,
    user_id: Option<&str>,
    mv: &Move<'_>,
) -> AppResult<Vec<Posted>> {
    debug_assert!(mv.qty < 0, "post_out is for outbound movements");
    let today = business_date;
    if let Some(batch) = mv.batch_id {
        let expiry_warning =
            check_expiry(conn, mv.variant_id, batch, -mv.qty, today, expired_policy)?;
        let mut posted = post(conn, negative_policy, business_date, now, user_id, mv)?;
        if posted.warning.is_none() {
            posted.warning = expiry_warning;
        }
        return Ok(vec![posted]);
    }
    if !tracks_batches(conn, mv.variant_id)? {
        return Ok(vec![post(
            conn,
            negative_policy,
            business_date,
            now,
            user_id,
            mv,
        )?]);
    }
    let allocations = allocate_fefo(
        conn,
        mv.variant_id,
        mv.warehouse_id,
        -mv.qty,
        today,
        expired_policy,
        negative_policy,
    )?;
    let mut out = Vec::with_capacity(allocations.len());
    for a in allocations {
        let mut posted = post(
            conn,
            negative_policy,
            business_date,
            now,
            user_id,
            &Move {
                batch_id: a.batch_id.as_deref(),
                qty: -a.qty,
                ..mv.clone()
            },
        )?;
        if a.expired && posted.warning.is_none() {
            posted.warning = Some(StockWarning {
                variant_id: mv.variant_id.into(),
                name: variant_name(conn, mv.variant_id)?,
                available: 0,
                requested: a.qty,
                expired_batch: a.batch_no.clone(),
            });
        }
        out.push(posted);
    }
    Ok(out)
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
                reversal_of: Some(&id),
                note: Some("Voided"),
                ..Move::default()
            },
        )?;
    }
    Ok(())
}
