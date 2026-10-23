//! Price lists (retail, wholesale, custom): prices per product and unit, quantity slabs and
//! effective dates, assigned to customers and to tills, with a bulk update that is previewed before
//! it is applied. Which price applies is decided by `domain::price_lists` only (the same rule as
//! `packages/shared/src/price-lists.ts`); the sale, the preview and an offline counter all call it.
//!
//! The list for a bill: the customer's own list, else the till's, else the default list, else the
//! product's own price.

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::common::{date, like_pattern, optional, required, Page, PageQuery};
use super::users::IdInput;
use super::{write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::domain::price_lists::{self, ListPriceEntry, ListPriceQuery};
use crate::error::{AppError, AppResult};

const KINDS: [&str; 3] = ["retail", "wholesale", "custom"];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceListSummary {
    pub id: String,
    pub name: String,
    /// "retail", "wholesale" or "custom".
    pub kind: String,
    pub is_default: bool,
    pub is_active: bool,
    pub notes: Option<String>,
    pub item_count: i64,
    pub customer_count: i64,
    pub till_count: i64,
    pub updated_at: String,
}

const SUMMARY_SELECT: &str = "SELECT l.id, l.name, l.kind, l.is_default, l.is_active, l.notes,
    (SELECT count(*) FROM price_list_items i WHERE i.price_list_id = l.id AND i.deleted_at IS NULL),
    (SELECT count(*) FROM parties p WHERE p.price_list_id = l.id AND p.deleted_at IS NULL),
    (SELECT count(*) FROM cash_registers r WHERE r.price_list_id = l.id AND r.deleted_at IS NULL),
    l.updated_at
  FROM price_lists l";

fn summary_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PriceListSummary> {
    Ok(PriceListSummary {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        is_default: r.get(3)?,
        is_active: r.get(4)?,
        notes: r.get(5)?,
        item_count: r.get(6)?,
        customer_count: r.get(7)?,
        till_count: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

fn can_view(ctx: &Ctx<'_>) -> AppResult<()> {
    if ctx.actor.can("products.view") || ctx.actor.can("price_lists.manage") {
        Ok(())
    } else {
        Err(AppError::forbidden("products.view"))
    }
}

pub fn list(ctx: &Ctx<'_>) -> AppResult<Vec<PriceListSummary>> {
    can_view(ctx)?;
    let mut stmt = ctx.conn.prepare(&format!(
        "{SUMMARY_SELECT} WHERE l.deleted_at IS NULL ORDER BY l.is_default DESC, lower(l.name)"
    ))?;
    let rows = stmt
        .query_map([], summary_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn get(conn: &Connection, id: &str) -> AppResult<PriceListSummary> {
    conn.query_row(
        &format!("{SUMMARY_SELECT} WHERE l.id = ?1 AND l.deleted_at IS NULL"),
        [id],
        summary_row,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That price list"))
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceListInput {
    pub id: Option<String>,
    pub expected_updated_at: Option<String>,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
    pub notes: Option<String>,
}

fn yes() -> bool {
    true
}

pub fn save(ctx: &mut Ctx<'_>, input: &PriceListInput) -> AppResult<PriceListSummary> {
    ctx.require("price_lists.manage")?;
    let name = required("name", &input.name, 80)?;
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::field(
            "kind",
            "Choose retail, wholesale or custom.",
        ));
    }
    if input.is_default && !input.is_active {
        return Err(AppError::field(
            "isDefault",
            "The default list must be active.",
        ));
    }
    let notes = optional("notes", input.notes.as_deref(), 500)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = write(ctx.conn, |tx| {
        if input.is_default {
            tx.execute(
                "UPDATE price_lists SET is_default = 0, updated_at = ?1, updated_by = ?2 WHERE is_default = 1 AND deleted_at IS NULL AND id <> coalesce(?3, '')",
                params![now, actor.user_id, input.id],
            )?;
        }
        let id = match &input.id {
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO price_lists (id, name, kind, is_default, is_active, notes, prices_include_tax, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?8, ?8)",
                    params![id, name, input.kind, input.is_default, input.is_active, notes, now, actor.user_id],
                )?;
                id
            }
            Some(id) => {
                let current = get(tx, id)?;
                if input
                    .expected_updated_at
                    .as_ref()
                    .is_some_and(|e| *e != current.updated_at)
                {
                    return Err(AppError::conflict(
                        "Someone else changed this price list. Reload it and try again.",
                    ));
                }
                if !input.is_active && (current.customer_count > 0 || current.till_count > 0) {
                    return Err(AppError::field(
                        "isActive",
                        "Customers or tills still use this list. Move them to another list first.",
                    ));
                }
                tx.execute(
                    "UPDATE price_lists SET name = ?1, kind = ?2, is_default = ?3, is_active = ?4, notes = ?5, updated_at = ?6, updated_by = ?7 WHERE id = ?8",
                    params![name, input.kind, input.is_default, input.is_active, notes, now, actor.user_id, id],
                )?;
                id.clone()
            }
        };
        AuditEntry::new("PRICE_LIST_SAVED", "price_lists", Some(&id))
            .by(actor)
            .summary(name.clone())
            .after(json!({ "kind": input.kind, "isDefault": input.is_default, "isActive": input.is_active }))
            .write(tx, &now)?;
        Ok(id)
    })?;
    get(ctx.conn, &id)
}

pub fn delete(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("price_lists.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let current = get(tx, &input.id)?;
        if current.customer_count > 0 || current.till_count > 0 {
            return Err(AppError::conflict(
                "Customers or tills still use this list. Move them to another list first.",
            ));
        }
        tx.execute(
            "UPDATE price_list_items SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE price_list_id = ?3 AND deleted_at IS NULL",
            params![now, actor.user_id, input.id],
        )?;
        tx.execute(
            "UPDATE price_lists SET deleted_at = ?1, is_default = 0, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("PRICE_LIST_DELETED", "price_lists", Some(&input.id))
            .by(actor)
            .summary(current.name)
            .write(tx, &now)
    })
}

// ------------------------------------------------------------------ prices

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceListItemQuery {
    pub price_list_id: String,
    pub search: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceListItemRow {
    pub id: String,
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    /// None: the product's base unit.
    pub unit_id: Option<String>,
    pub unit_code: String,
    pub min_qty: i64,
    pub price: i64,
    /// The product's own price per base unit, for comparison.
    pub base_price: i64,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

pub fn items(ctx: &Ctx<'_>, q: &PriceListItemQuery) -> AppResult<Page<PriceListItemRow>> {
    can_view(ctx)?;
    get(ctx.conn, &q.price_list_id)?;
    let keys = [
        (
            "name",
            "lower(p.name) || char(31) || lower(coalesce(v.variant_name, ''))",
        ),
        ("sku", "lower(v.sku)"),
        ("price", "i.price"),
        ("minQty", "i.min_qty"),
        ("validFrom", "coalesce(i.valid_from, '')"),
    ];
    let order = q.page.order_by(
        &keys,
        "lower(p.name), lower(coalesce(v.variant_name, '')), i.min_qty, coalesce(i.valid_from, '')",
        "i.id",
    )?;
    let search = q
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(like_pattern);
    let filter = "i.price_list_id = ?1 AND i.deleted_at IS NULL
        AND (?2 IS NULL OR lower(p.name) LIKE lower(?2) ESCAPE '\\' OR lower(v.sku) LIKE lower(?2) ESCAPE '\\' OR v.barcode = ?3)";
    let from = "FROM price_list_items i JOIN product_variants v ON v.id = i.variant_id JOIN products p ON p.id = v.product_id
        JOIN units u ON u.id = coalesce(i.unit_id, p.base_unit_id)";
    let raw = q.search.as_deref().map(str::trim);
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) {from} WHERE {filter}"),
        params![q.price_list_id, search, raw],
        |r| r.get(0),
    )?;
    let (limit, offset) = q.page.limit_offset();
    let mut stmt = ctx.conn.prepare(&format!(
        "SELECT i.id, i.variant_id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, i.unit_id, u.code, i.min_qty, i.price,
                v.sale_price, i.valid_from, i.valid_to
         {from} WHERE {filter} ORDER BY {order} LIMIT ?4 OFFSET ?5"
    ))?;
    let rows = stmt
        .query_map(params![q.price_list_id, search, raw, limit, offset], |r| {
            Ok(PriceListItemRow {
                id: r.get(0)?,
                variant_id: r.get(1)?,
                name: r.get(2)?,
                sku: r.get(3)?,
                unit_id: r.get(4)?,
                unit_code: r.get(5)?,
                min_qty: r.get(6)?,
                price: r.get(7)?,
                base_price: r.get(8)?,
                valid_from: r.get(9)?,
                valid_to: r.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page::new(rows, total, &q.page))
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceItemInput {
    /// An existing price to change.
    pub id: Option<String>,
    pub variant_id: String,
    /// A pack unit of the product; empty: the base unit.
    pub unit_id: Option<String>,
    /// Quantity slab start (thousandths of the price's unit); 0 = from the first unit.
    #[serde(default)]
    pub min_qty: i64,
    pub price: i64,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPricesInput {
    pub price_list_id: String,
    #[serde(default)]
    pub upsert: Vec<PriceItemInput>,
    /// Price ids to remove.
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPricesResult {
    pub saved: i64,
    pub removed: i64,
}

fn check_item(
    conn: &Connection,
    i: usize,
    item: &PriceItemInput,
) -> AppResult<(Option<String>, Option<String>)> {
    let field = |f: &str| format!("upsert.{i}.{f}");
    if item.price < 0 {
        return Err(AppError::field(
            &field("price"),
            "A price can't be negative.",
        ));
    }
    if item.min_qty < 0 {
        return Err(AppError::field(
            &field("minQty"),
            "A quantity can't be negative.",
        ));
    }
    let base_unit: String = conn
        .query_row(
            "SELECT p.base_unit_id FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = ?1 AND v.deleted_at IS NULL",
            [&item.variant_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::field(&field("variantId"), "That product no longer exists."))?;
    if let Some(unit) = item.unit_id.as_deref().filter(|u| *u != base_unit) {
        let ok: i64 = conn.query_row(
            "SELECT count(*) FROM unit_conversions c JOIN product_variants v ON v.id = ?1
             WHERE c.from_unit_id = ?2 AND c.deleted_at IS NULL AND (c.product_id = v.product_id OR c.product_id IS NULL)",
            params![item.variant_id, unit],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::field(
                &field("unitId"),
                "The product isn't sold in that unit.",
            ));
        }
    }
    let from = item
        .valid_from
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| date(&field("validFrom"), d))
        .transpose()?;
    let to = item
        .valid_to
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| date(&field("validTo"), d))
        .transpose()?;
    if let (Some(f), Some(t)) = (&from, &to) {
        if f > t {
            return Err(AppError::field(
                &field("validTo"),
                "The end is before the start.",
            ));
        }
    }
    Ok((from, to))
}

pub fn set_prices(ctx: &mut Ctx<'_>, input: &SetPricesInput) -> AppResult<SetPricesResult> {
    ctx.require("price_lists.manage")?;
    if input.upsert.len() + input.remove.len() > 2_000 {
        return Err(AppError::validation("At most 2,000 prices at a time."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let list = get(tx, &input.price_list_id)?;
        let mut saved = 0;
        for (i, item) in input.upsert.iter().enumerate() {
            let (from, to) = check_item(tx, i, item)?;
            let base_unit: String = tx.query_row(
                "SELECT p.base_unit_id FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = ?1",
                [&item.variant_id],
                |r| r.get(0),
            )?;
            // A price for the base unit is stored without a unit (one way to say it).
            let unit = item
                .unit_id
                .clone()
                .filter(|u| *u != base_unit && !u.is_empty());
            match &item.id {
                Some(id) => {
                    let changed = tx.execute(
                        "UPDATE price_list_items SET unit_id = ?1, min_qty = ?2, price = ?3, valid_from = ?4, valid_to = ?5, updated_at = ?6, updated_by = ?7
                         WHERE id = ?8 AND price_list_id = ?9 AND variant_id = ?10 AND deleted_at IS NULL",
                        params![unit, item.min_qty, item.price, from, to, now, actor.user_id, id, input.price_list_id, item.variant_id],
                    )?;
                    if changed == 0 {
                        return Err(AppError::field(
                            &format!("upsert.{i}.id"),
                            "That price isn't on this list.",
                        ));
                    }
                }
                None => {
                    tx.execute(
                        "INSERT INTO price_list_items (id, price_list_id, variant_id, unit_id, min_qty, price, valid_from, valid_to, created_at, updated_at, created_by, updated_by)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10)
                         ON CONFLICT DO NOTHING",
                        params![new_id(), input.price_list_id, item.variant_id, unit, item.min_qty, item.price, from, to, now, actor.user_id],
                    )
                    .and_then(|n| {
                        if n == 0 {
                            // Same product, unit, slab and start: change that price instead.
                            tx.execute(
                                "UPDATE price_list_items SET price = ?1, valid_to = ?2, updated_at = ?3, updated_by = ?4
                                 WHERE price_list_id = ?5 AND variant_id = ?6 AND coalesce(unit_id, '') = coalesce(?7, '') AND min_qty = ?8
                                   AND coalesce(valid_from, '') = coalesce(?9, '') AND deleted_at IS NULL",
                                params![item.price, to, now, actor.user_id, input.price_list_id, item.variant_id, unit, item.min_qty, from],
                            )
                        } else {
                            Ok(n)
                        }
                    })?;
                }
            }
            saved += 1;
        }
        let mut removed = 0;
        for id in &input.remove {
            removed += tx.execute(
                "UPDATE price_list_items SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3 AND price_list_id = ?4 AND deleted_at IS NULL",
                params![now, actor.user_id, id, input.price_list_id],
            )?;
        }
        tx.execute(
            "UPDATE price_lists SET updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.price_list_id],
        )?;
        AuditEntry::new(
            "PRICE_LIST_PRICES",
            "price_lists",
            Some(&input.price_list_id),
        )
        .by(actor)
        .summary(format!("{}: {saved} saved, {removed} removed", list.name))
        .after(serde_json::to_value(
            input.upsert.iter().take(200).collect::<Vec<_>>(),
        )?)
        .write(tx, &now)?;
        Ok(SetPricesResult {
            saved,
            removed: i64::try_from(removed).unwrap_or(i64::MAX),
        })
    })
}

// ------------------------------------------------------------------ bulk update

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BulkUpdateInput {
    /// A price list; None: the products' own selling prices.
    pub price_list_id: Option<String>,
    pub category_id: Option<String>,
    /// Only these products (variants); empty: all in the category (or all).
    #[serde(default)]
    pub variant_ids: Vec<String>,
    /// "percent" (value in ppm: 50000 = +5%) or "amount" (minor units).
    pub mode: String,
    pub value: i64,
    /// Round each new price to a multiple of this (minor units: 100 = ₹1); 0 = no rounding.
    #[serde(default)]
    pub round_to: i64,
    /// false: only show what would change.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BulkRow {
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub unit_code: String,
    pub min_qty: i64,
    pub old_price: i64,
    pub new_price: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BulkUpdateResult {
    pub rows: Vec<BulkRow>,
    pub changed: i64,
    pub applied: bool,
}

/// The new price after a bulk change (never below zero).
pub fn adjusted(old: i64, mode: &str, value: i64, round_to: i64) -> i64 {
    let raw = if mode == "percent" {
        old + money::apply_rate(old, value, RoundingMode::HalfUp)
    } else {
        old + value
    };
    let rounded = if round_to > 0 {
        i64::try_from(money::div_round(
            i128::from(raw),
            i128::from(round_to),
            RoundingMode::HalfUp,
        ))
        .unwrap_or(0)
            * round_to
    } else {
        raw
    };
    rounded.max(0)
}

pub fn bulk_update(ctx: &mut Ctx<'_>, input: &BulkUpdateInput) -> AppResult<BulkUpdateResult> {
    match &input.price_list_id {
        Some(_) => ctx.require("price_lists.manage")?,
        None => ctx.require("products.price_change")?,
    }
    if !["percent", "amount"].contains(&input.mode.as_str()) {
        return Err(AppError::field("mode", "Choose a percentage or an amount."));
    }
    if input.mode == "percent" && !(-900_000..=10_000_000).contains(&input.value) {
        return Err(AppError::field(
            "value",
            "Use a change between -90% and +1000%.",
        ));
    }
    if !(0..=1_000_000).contains(&input.round_to) {
        return Err(AppError::field("roundTo", "Round to at most 10,000."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    let variants = (!input.variant_ids.is_empty())
        .then(|| serde_json::to_string(&input.variant_ids))
        .transpose()?;
    write(ctx.conn, |tx| {
        let scope = "(?2 IS NULL OR p.category_id = ?2) AND (?3 IS NULL OR v.id IN (SELECT value FROM json_each(?3)))
            AND v.deleted_at IS NULL AND p.deleted_at IS NULL";
        // (row id, variant, name, sku, unit, slab, old price)
        let rows: Vec<(String, String, String, String, String, i64, i64)> = match &input
            .price_list_id
        {
            Some(list) => {
                get(tx, list)?;
                let mut stmt = tx.prepare(&format!(
                    "SELECT i.id, v.id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, u.code, i.min_qty, i.price
                     FROM price_list_items i JOIN product_variants v ON v.id = i.variant_id JOIN products p ON p.id = v.product_id
                     JOIN units u ON u.id = coalesce(i.unit_id, p.base_unit_id)
                     WHERE i.price_list_id = ?1 AND i.deleted_at IS NULL AND {scope} ORDER BY lower(p.name), i.min_qty"
                ))?;
                let rows = stmt
                    .query_map(params![list, input.category_id, variants], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    })?
                    .collect::<Result<_, _>>()?;
                rows
            }
            None => {
                let mut stmt = tx.prepare(&format!(
                    "SELECT v.id, v.id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, u.code, 0, v.sale_price
                     FROM product_variants v JOIN products p ON p.id = v.product_id JOIN units u ON u.id = p.base_unit_id
                     WHERE ?1 IS NULL AND {scope} ORDER BY lower(p.name)"
                ))?;
                let rows = stmt
                    .query_map(
                        params![Option::<String>::None, input.category_id, variants],
                        |r| {
                            Ok((
                                r.get(0)?,
                                r.get(1)?,
                                r.get(2)?,
                                r.get(3)?,
                                r.get(4)?,
                                r.get(5)?,
                                r.get(6)?,
                            ))
                        },
                    )?
                    .collect::<Result<_, _>>()?;
                rows
            }
        };
        if rows.len() > 5_000 {
            return Err(AppError::validation(
                "More than 5,000 prices would change: narrow it down by category.",
            ));
        }
        let mut out = Vec::with_capacity(rows.len());
        let mut changed = 0;
        for (row_id, variant_id, name, sku, unit_code, min_qty, old) in rows {
            let new = adjusted(old, &input.mode, input.value, input.round_to);
            if new != old {
                changed += 1;
                if input.apply {
                    match &input.price_list_id {
                        Some(_) => {
                            tx.execute(
                                "UPDATE price_list_items SET price = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
                                params![new, now, actor.user_id, row_id],
                            )?;
                        }
                        None => {
                            tx.execute(
                                "UPDATE product_variants SET sale_price = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
                                params![new, now, actor.user_id, variant_id],
                            )?;
                            // An open product editor must reload before saving over the new price.
                            tx.execute(
                                "UPDATE products SET updated_at = ?1, updated_by = ?2 WHERE id = (SELECT product_id FROM product_variants WHERE id = ?3)",
                                params![now, actor.user_id, variant_id],
                            )?;
                        }
                    }
                }
            }
            out.push(BulkRow {
                variant_id,
                name,
                sku,
                unit_code,
                min_qty,
                old_price: old,
                new_price: new,
            });
        }
        if input.apply && changed > 0 {
            let target = input.price_list_id.as_deref();
            AuditEntry::new(
                "PRICE_BULK_UPDATE",
                if target.is_some() {
                    "price_lists"
                } else {
                    "product_variants"
                },
                target,
            )
            .by(actor)
            .summary(format!(
                "{changed} prices: {} {}{}",
                input.mode,
                input.value,
                if input.round_to > 0 {
                    format!(", rounded to {}", input.round_to)
                } else {
                    String::new()
                }
            ))
            .after(serde_json::to_value(
                out.iter()
                    .filter(|r| r.old_price != r.new_price)
                    .take(500)
                    .collect::<Vec<_>>(),
            )?)
            .write(tx, &now)?;
        }
        Ok(BulkUpdateResult {
            rows: out,
            changed,
            applied: input.apply,
        })
    })
}

// ------------------------------------------------------------------ tills

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TillPriceList {
    pub register_id: String,
    pub register_name: String,
    pub price_list_id: Option<String>,
}

pub fn tills(ctx: &Ctx<'_>) -> AppResult<Vec<TillPriceList>> {
    can_view(ctx)?;
    let mut stmt = ctx.conn.prepare(
        "SELECT id, name, price_list_id FROM cash_registers WHERE deleted_at IS NULL ORDER BY lower(name)",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TillPriceList {
                register_id: r.get(0)?,
                register_name: r.get(1)?,
                price_list_id: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetTillPriceListInput {
    pub register_id: String,
    pub price_list_id: Option<String>,
}

pub fn set_till(ctx: &mut Ctx<'_>, input: &SetTillPriceListInput) -> AppResult<Vec<TillPriceList>> {
    ctx.require("price_lists.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        if let Some(list) = &input.price_list_id {
            let active: bool = get(tx, list)?.is_active;
            if !active {
                return Err(AppError::field(
                    "priceListId",
                    "Choose an active price list.",
                ));
            }
        }
        let changed = tx.execute(
            "UPDATE cash_registers SET price_list_id = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4 AND deleted_at IS NULL",
            params![input.price_list_id, now, actor.user_id, input.register_id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("That till"));
        }
        AuditEntry::new(
            "PRICE_LIST_ASSIGNED",
            "cash_registers",
            Some(&input.register_id),
        )
        .by(actor)
        .after(json!({ "priceListId": input.price_list_id }))
        .write(tx, &now)
    })?;
    tills(ctx)
}

// ------------------------------------------------------------------ the price on a bill

/// The list a bill is priced at: the customer's, else the till's, else the default list.
pub(crate) fn list_for(
    conn: &Connection,
    party_id: Option<&str>,
    register_session_id: Option<&str>,
) -> AppResult<Option<String>> {
    if let Some(party) = party_id {
        let list: Option<String> = conn
            .query_row(
                "SELECT l.id FROM parties p JOIN price_lists l ON l.id = p.price_list_id
                 WHERE p.id = ?1 AND l.is_active = 1 AND l.deleted_at IS NULL",
                [party],
                |r| r.get(0),
            )
            .optional()?;
        if list.is_some() {
            return Ok(list);
        }
    }
    if let Some(session) = register_session_id {
        let list: Option<String> = conn
            .query_row(
                "SELECT l.id FROM register_sessions s JOIN cash_registers r ON r.id = s.register_id JOIN price_lists l ON l.id = r.price_list_id
                 WHERE s.id = ?1 AND l.is_active = 1 AND l.deleted_at IS NULL",
                [session],
                |r| r.get(0),
            )
            .optional()?;
        if list.is_some() {
            return Ok(list);
        }
    }
    Ok(conn
        .query_row(
            "SELECT id FROM price_lists WHERE is_default = 1 AND is_active = 1 AND deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

/// A list's prices for one product.
pub(crate) fn entries(
    conn: &Connection,
    list_id: &str,
    variant_id: &str,
) -> AppResult<Vec<ListPriceEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT unit_id, min_qty, price, valid_from, valid_to FROM price_list_items
         WHERE price_list_id = ?1 AND variant_id = ?2 AND deleted_at IS NULL ORDER BY created_at, id",
    )?;
    let rows = stmt
        .query_map(params![list_id, variant_id], |r| {
            Ok(ListPriceEntry {
                unit_id: r.get(0)?,
                min_qty: r.get(1)?,
                price: r.get(2)?,
                valid_from: r.get(3)?,
                valid_to: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The list price per sold unit for a line, if the list has one.
#[allow(clippy::too_many_arguments, reason = "the line's unit and quantity")]
pub(crate) fn price_for(
    entries: Vec<ListPriceEntry>,
    sold_unit_id: &str,
    base_unit_id: &str,
    factor_num: i64,
    factor_den: i64,
    qty: i64,
    business_date: &str,
) -> Option<i64> {
    price_lists::resolve(&ListPriceQuery {
        entries,
        sold_unit_id,
        base_unit_id,
        factor_num,
        factor_den,
        qty,
        date: business_date,
    })
    .price
}
