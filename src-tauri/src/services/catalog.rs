//! Catalogue: categories, brands, units, tax rates and products (with variants, pack sizes and
//! opening stock), plus the fast POS lookups (barcode, SKU, prefix search).

use std::collections::HashSet;

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::common::{non_negative, optional, rate, required, Page, PageQuery};
use super::journal::{self, Entry, JLine};
use super::settings;
use super::setup::default_location;
use super::stock_posting::{self, Move};
use super::users::IdInput;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::error::{AppError, AppResult};

// ------------------------------------------------------------------ simple masters

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub product_count: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Brand {
    pub id: String,
    pub name: String,
    pub product_count: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrandInput {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    pub id: String,
    pub code: String,
    pub name: String,
    pub decimals: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnitInput {
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    pub decimals: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxRate {
    pub id: String,
    pub name: String,
    pub tax_type: String,
    /// ppm (18% = 180000).
    pub rate: i64,
    pub cess_rate: i64,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxRateInput {
    pub id: Option<String>,
    pub name: String,
    /// "gst", "vat", "none" or "other".
    pub tax_type: String,
    pub rate: i64,
    pub cess_rate: i64,
    pub is_active: bool,
}

pub fn list_categories(ctx: &Ctx<'_>) -> AppResult<Vec<Category>> {
    ctx.require("products.view")?;
    let mut stmt = ctx.conn.prepare(
        "SELECT c.id, c.name, c.parent_id,
           (SELECT count(*) FROM products p WHERE p.category_id = c.id AND p.deleted_at IS NULL)
         FROM categories c WHERE c.deleted_at IS NULL ORDER BY lower(c.name)",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                product_count: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn save_category(ctx: &mut Ctx<'_>, input: &CategoryInput) -> AppResult<IdOut> {
    ctx.require("products.manage")?;
    let name = required("name", &input.name, 80)?;
    if input.parent_id.is_some() && input.parent_id == input.id {
        return Err(AppError::field(
            "parentId",
            "A category can't be its own parent.",
        ));
    }
    save_master(
        ctx,
        "categories",
        input.id.as_deref(),
        |tx, id, now, user| {
            if let Some(parent) = &input.parent_id {
                exists(tx, "categories", parent, "parentId")?;
            }
            Ok(tx.execute(
            "INSERT INTO categories (id, name, parent_id, created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?5)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, parent_id = excluded.parent_id,
               updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            params![id, name, input.parent_id, now, user],
        )?)
        },
    )
}

pub fn list_brands(ctx: &Ctx<'_>) -> AppResult<Vec<Brand>> {
    ctx.require("products.view")?;
    let mut stmt = ctx.conn.prepare(
        "SELECT b.id, b.name, (SELECT count(*) FROM products p WHERE p.brand_id = b.id AND p.deleted_at IS NULL)
         FROM brands b WHERE b.deleted_at IS NULL ORDER BY lower(b.name)",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Brand {
                id: r.get(0)?,
                name: r.get(1)?,
                product_count: r.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn save_brand(ctx: &mut Ctx<'_>, input: &BrandInput) -> AppResult<IdOut> {
    ctx.require("products.manage")?;
    let name = required("name", &input.name, 80)?;
    save_master(ctx, "brands", input.id.as_deref(), |tx, id, now, user| {
        Ok(tx.execute(
            "INSERT INTO brands (id, name, created_at, updated_at, created_by, updated_by) VALUES (?1, ?2, ?3, ?3, ?4, ?4)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            params![id, name, now, user],
        )?)
    })
}

pub fn list_units(ctx: &Ctx<'_>) -> AppResult<Vec<Unit>> {
    ctx.require("products.view")?;
    let mut stmt = ctx.conn.prepare(
        "SELECT id, code, name, decimals FROM units WHERE deleted_at IS NULL ORDER BY lower(code)",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Unit {
                id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                decimals: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn save_unit(ctx: &mut Ctx<'_>, input: &UnitInput) -> AppResult<IdOut> {
    ctx.require("products.manage")?;
    let code = required("code", &input.code, 12)?;
    let name = required("name", &input.name, 40)?;
    if !(0..=3).contains(&input.decimals) {
        return Err(AppError::field("decimals", "Use 0 to 3 decimals."));
    }
    save_master(ctx, "units", input.id.as_deref(), |tx, id, now, user| {
        Ok(tx.execute(
            "INSERT INTO units (id, code, name, decimals, created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?6)
             ON CONFLICT (id) DO UPDATE SET code = excluded.code, name = excluded.name, decimals = excluded.decimals,
               updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            params![id, code, name, input.decimals, now, user],
        )?)
    })
}

pub fn list_tax_rates(ctx: &Ctx<'_>) -> AppResult<Vec<TaxRate>> {
    ctx.require("products.view")?;
    let mut stmt = ctx.conn.prepare(
        "SELECT id, name, tax_type, rate, cess_rate, is_active FROM tax_rates WHERE deleted_at IS NULL ORDER BY rate, lower(name)",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TaxRate {
                id: r.get(0)?,
                name: r.get(1)?,
                tax_type: r.get(2)?,
                rate: r.get(3)?,
                cess_rate: r.get(4)?,
                is_active: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn save_tax_rate(ctx: &mut Ctx<'_>, input: &TaxRateInput) -> AppResult<IdOut> {
    ctx.require("settings.manage")?;
    let name = required("name", &input.name, 60)?;
    if !matches!(input.tax_type.as_str(), "gst" | "vat" | "none" | "other") {
        return Err(AppError::field("taxType", "Use gst, vat, none or other."));
    }
    let tax_rate = rate("rate", input.rate)?;
    let cess = rate("cessRate", input.cess_rate)?;
    if input.tax_type == "gst" && tax_rate % 2 != 0 {
        return Err(AppError::field(
            "rate",
            "A GST rate must split evenly into CGST and SGST.",
        ));
    }
    save_master(
        ctx,
        "tax_rates",
        input.id.as_deref(),
        |tx, id, now, user| {
            Ok(tx.execute(
            "INSERT INTO tax_rates (id, name, tax_type, rate, cess_rate, is_active, created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?8)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, tax_type = excluded.tax_type, rate = excluded.rate,
               cess_rate = excluded.cess_rate, is_active = excluded.is_active, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
            params![id, name, input.tax_type, tax_rate, cess, input.is_active, now, user],
        )?)
        },
    )
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMasterInput {
    /// "categories", "brands" or "units".
    pub kind: String,
    pub id: String,
}

/// Soft-deletes a category, brand or unit that nothing live uses.
pub fn delete_master(ctx: &mut Ctx<'_>, input: &DeleteMasterInput) -> AppResult<()> {
    ctx.require("products.manage")?;
    let usage_sql = match input.kind.as_str() {
        "categories" => {
            "SELECT (SELECT count(*) FROM products WHERE category_id = ?1 AND deleted_at IS NULL)
                  + (SELECT count(*) FROM categories WHERE parent_id = ?1 AND deleted_at IS NULL)"
        }
        "brands" => "SELECT count(*) FROM products WHERE brand_id = ?1 AND deleted_at IS NULL",
        "units" => {
            "SELECT (SELECT count(*) FROM products WHERE base_unit_id = ?1 AND deleted_at IS NULL)
                  + (SELECT count(*) FROM unit_conversions WHERE (from_unit_id = ?1 OR to_unit_id = ?1) AND deleted_at IS NULL)"
        }
        _ => return Err(AppError::field("kind", "Use categories, brands or units.")),
    };
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        exists(tx, &input.kind, &input.id, "id")?;
        let used: i64 = tx.query_row(usage_sql, [&input.id], |r| r.get(0))?;
        if used > 0 {
            return Err(AppError::conflict(
                "It is still used by products (or sub-categories). Reassign them first.",
            ));
        }
        tx.execute(
            &format!(
                "UPDATE {} SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
                input.kind
            ),
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("DELETED", &input.kind, Some(&input.id))
            .by(actor)
            .write(tx, &now)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdOut {
    pub id: String,
}

fn exists(conn: &Connection, table: &str, id: &str, field: &str) -> AppResult<()> {
    let found: i64 = conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE id = ?1 AND deleted_at IS NULL"),
        [id],
        |r| r.get(0),
    )?;
    if found == 0 {
        return Err(AppError::field(
            field,
            "The selected item no longer exists.",
        ));
    }
    Ok(())
}

fn save_master(
    ctx: &mut Ctx<'_>,
    table: &str,
    id: Option<&str>,
    upsert: impl FnOnce(&Connection, &str, &str, &str) -> AppResult<usize>,
) -> AppResult<IdOut> {
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let id = match id {
            Some(id) => {
                exists(tx, table, id, "id")?;
                id.to_string()
            }
            None => new_id(),
        };
        upsert(tx, &id, &now, &actor.user_id)?;
        AuditEntry::new("SAVED", table, Some(&id))
            .by(actor)
            .write(tx, &now)?;
        Ok(IdOut { id })
    })
}

// ------------------------------------------------------------------ products

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariantInput {
    pub id: Option<String>,
    pub variant_name: Option<String>,
    pub sku: String,
    pub barcode: Option<String>,
    pub sale_price: i64,
    pub mrp: Option<i64>,
    /// Starting cost per base unit (new variants only; afterwards purchases maintain it).
    pub cost_price: Option<i64>,
    #[serde(default)]
    pub reorder_level: i64,
    #[serde(default)]
    pub reorder_qty: i64,
    #[serde(default = "yes")]
    pub is_active: bool,
    /// Opening stock in the default store (new variants only, thousandths).
    pub opening_stock: Option<i64>,
    /// Batch of the opening stock (batch-tracked products).
    #[serde(default)]
    pub opening_batch_no: Option<String>,
    #[serde(default)]
    pub opening_expiry_date: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnitConversionInput {
    pub from_unit_id: String,
    /// 1 from-unit = factorNum / factorDen base units (1 box = 12 pcs → 12 / 1).
    pub factor_num: i64,
    pub factor_den: i64,
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductInput {
    pub id: Option<String>,
    /// The `updatedAt` the editor loaded; a newer one means someone else saved meanwhile.
    pub expected_updated_at: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category_id: Option<String>,
    pub brand_id: Option<String>,
    pub base_unit_id: String,
    pub tax_rate_id: Option<String>,
    pub hsn_sac: Option<String>,
    #[serde(default)]
    pub track_batches: bool,
    #[serde(default)]
    pub track_expiry: bool,
    #[serde(default)]
    pub is_service: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
    pub variants: Vec<VariantInput>,
    #[serde(default)]
    pub unit_conversions: Vec<UnitConversionInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarehouseStock {
    pub warehouse_id: String,
    pub warehouse_name: String,
    pub qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariantDetail {
    pub id: String,
    pub variant_name: Option<String>,
    pub sku: String,
    pub barcode: Option<String>,
    pub sale_price: i64,
    pub mrp: Option<i64>,
    pub cost_price: i64,
    pub reorder_level: i64,
    pub reorder_qty: i64,
    pub is_active: bool,
    pub stock: i64,
    pub stock_by_warehouse: Vec<WarehouseStock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnitConversionDetail {
    pub from_unit_id: String,
    pub from_unit_code: String,
    pub factor_num: i64,
    pub factor_den: i64,
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub brand_id: Option<String>,
    pub brand_name: Option<String>,
    pub base_unit_id: String,
    pub base_unit_code: String,
    pub tax_rate_id: Option<String>,
    pub tax_rate: i64,
    pub hsn_sac: Option<String>,
    pub track_batches: bool,
    pub track_expiry: bool,
    pub is_service: bool,
    pub is_active: bool,
    pub updated_at: String,
    /// The picture's name (see `services::images`), or null; also the screen's cache key.
    pub image_key: Option<String>,
    pub variants: Vec<VariantDetail>,
    pub unit_conversions: Vec<UnitConversionDetail>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListQuery {
    pub search: Option<String>,
    pub category_id: Option<String>,
    pub brand_id: Option<String>,
    #[serde(default)]
    pub low_stock_only: bool,
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListItem {
    pub product_id: String,
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub category_name: Option<String>,
    pub brand_name: Option<String>,
    pub unit_code: String,
    pub sale_price: i64,
    pub mrp: Option<i64>,
    pub cost_price: i64,
    pub tax_rate: i64,
    pub stock: i64,
    pub reorder_level: i64,
    pub is_active: bool,
    pub is_low_stock: bool,
    pub updated_at: String,
    pub image_key: Option<String>,
}

fn validate_code(field: &str, value: Option<&str>, min: usize) -> AppResult<Option<String>> {
    let Some(code) = optional(field, value, 48)? else {
        return Ok(None);
    };
    if code.len() < min || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::field(
            field,
            format!("Use {min}–48 letters, digits or -."),
        ));
    }
    Ok(Some(code))
}

struct CleanVariant {
    input: VariantInput,
    variant_name: Option<String>,
    sku: String,
    barcode: Option<String>,
}

fn clean_variants(input: &ProductInput) -> AppResult<Vec<CleanVariant>> {
    if input.variants.is_empty() {
        return Err(AppError::field(
            "variants",
            "Add at least one variant (SKU).",
        ));
    }
    if input.variants.len() > 200 {
        return Err(AppError::field(
            "variants",
            "A product can have at most 200 variants.",
        ));
    }
    let mut skus = HashSet::new();
    let mut barcodes = HashSet::new();
    input
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let sku = required(&format!("variants.{i}.sku"), &v.sku, 64)?;
            if !sku
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
            {
                return Err(AppError::field(
                    &format!("variants.{i}.sku"),
                    "SKU may use letters, digits and . _ / -",
                ));
            }
            if !skus.insert(sku.to_lowercase()) {
                return Err(AppError::field(
                    &format!("variants.{i}.sku"),
                    "SKUs must be different.",
                ));
            }
            let barcode = validate_code(&format!("variants.{i}.barcode"), v.barcode.as_deref(), 4)?;
            if let Some(b) = &barcode {
                if !barcodes.insert(b.clone()) {
                    return Err(AppError::field(
                        &format!("variants.{i}.barcode"),
                        "Barcodes must be different.",
                    ));
                }
            }
            non_negative(&format!("variants.{i}.salePrice"), v.sale_price)?;
            if let Some(mrp) = v.mrp {
                non_negative(&format!("variants.{i}.mrp"), mrp)?;
                if v.sale_price > mrp {
                    return Err(AppError::field(
                        &format!("variants.{i}.salePrice"),
                        "The sale price can't be above the MRP.",
                    ));
                }
            }
            non_negative(&format!("variants.{i}.reorderLevel"), v.reorder_level)?;
            non_negative(&format!("variants.{i}.reorderQty"), v.reorder_qty)?;
            if let Some(cost) = v.cost_price {
                non_negative(&format!("variants.{i}.costPrice"), cost)?;
            }
            let variant_name = optional(
                &format!("variants.{i}.variantName"),
                v.variant_name.as_deref(),
                60,
            )?;
            Ok(CleanVariant {
                input: v.clone(),
                variant_name,
                sku,
                barcode,
            })
        })
        .collect()
}

fn insert_variant(
    conn: &Connection,
    product_id: &str,
    v: &CleanVariant,
    now: &str,
    user: &str,
) -> AppResult<String> {
    let id = new_id();
    conn.execute(
        "INSERT INTO product_variants (id, product_id, variant_name, sku, barcode, sale_price, mrp, cost_price, reorder_level,
            reorder_qty, is_active, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?13)",
        params![
            id,
            product_id,
            v.variant_name,
            v.sku,
            v.barcode,
            v.input.sale_price,
            v.input.mrp,
            v.input.cost_price.unwrap_or(0),
            v.input.reorder_level,
            v.input.reorder_qty,
            v.input.is_active,
            now,
            user
        ],
    )?;
    Ok(id)
}

fn total_stock(conn: &Connection, variant_id: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(sum(qty), 0) FROM stock_levels WHERE variant_id = ?1",
        [variant_id],
        |r| r.get(0),
    )?)
}

/// A barcode identifies one thing at the till: it may not repeat within the product, nor belong
/// to another product's variant or pack size (the unique indexes only check each table alone).
fn check_barcodes(
    conn: &Connection,
    product_id: Option<&str>,
    variants: &[CleanVariant],
    packs: &[UnitConversionInput],
) -> AppResult<()> {
    let mut seen = HashSet::new();
    let variant_codes = variants.iter().enumerate().filter_map(|(i, v)| {
        v.barcode
            .as_deref()
            .map(|b| (format!("variants.{i}.barcode"), b.to_string()))
    });
    let pack_codes = packs.iter().enumerate().filter_map(|(i, c)| {
        c.barcode
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|b| (format!("unitConversions.{i}.barcode"), b.to_string()))
    });
    for (field, code) in variant_codes.chain(pack_codes) {
        if !seen.insert(code.clone()) {
            return Err(AppError::field(
                &field,
                "This barcode is used twice in this product.",
            ));
        }
        if super::printing::barcode_taken(conn, &code, product_id)? {
            return Err(AppError::field(
                &field,
                "Another product already has this barcode.",
            ));
        }
    }
    Ok(())
}

pub fn save_product(ctx: &mut Ctx<'_>, input: &ProductInput) -> AppResult<ProductDetail> {
    ctx.require("products.manage")?;
    let name = required("name", &input.name, 160)?;
    let description = optional("description", input.description.as_deref(), 2000)?;
    let hsn_sac = optional("hsnSac", input.hsn_sac.as_deref(), 10)?;
    if hsn_sac
        .as_ref()
        .is_some_and(|h| !h.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(AppError::field("hsnSac", "HSN/SAC codes are digits only."));
    }
    let variants = clean_variants(input)?;
    let has_opening = variants
        .iter()
        .any(|v| v.input.opening_stock.unwrap_or(0) != 0);
    if has_opening {
        ctx.require("stock.adjust")?;
    }
    if input.is_service && has_opening {
        return Err(AppError::field("variants", "Services don't carry stock."));
    }
    for c in &input.unit_conversions {
        if c.factor_num <= 0 || c.factor_den <= 0 {
            return Err(AppError::field(
                "unitConversions",
                "Pack sizes need positive factors.",
            ));
        }
        if c.from_unit_id == input.base_unit_id {
            return Err(AppError::field(
                "unitConversions",
                "A pack size must use a different unit from the base unit.",
            ));
        }
        validate_code("unitConversions.barcode", c.barcode.as_deref(), 4)?;
    }
    let has_variants = variants.len() > 1 || variants.iter().any(|v| v.variant_name.is_some());

    // Price changes on an existing product need permission (or a manager's approval).
    let mut approved_by = None;
    if let Some(id) = &input.id {
        for v in &variants {
            let Some(variant_id) = &v.input.id else {
                continue;
            };
            let old: Option<(i64, Option<i64>)> = ctx
                .conn
                .query_row(
                    "SELECT sale_price, mrp FROM product_variants WHERE id = ?1 AND product_id = ?2 AND deleted_at IS NULL",
                    params![variant_id, id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if old.is_some_and(|(price, mrp)| price != v.input.sale_price || mrp != v.input.mrp) {
                approved_by = ctx.approve("products.price_change", "change a selling price")?;
            }
        }
    }

    let settings = settings::load(ctx.conn)?;
    let now = ctx.now_str();
    let today = settings.business_date(ctx.now);
    let actor = ctx.actor;
    let product_id = write(ctx.conn, |tx| {
        exists(tx, "units", &input.base_unit_id, "baseUnitId")?;
        check_barcodes(tx, input.id.as_deref(), &variants, &input.unit_conversions)?;
        if let Some(c) = &input.category_id {
            exists(tx, "categories", c, "categoryId")?;
        }
        if let Some(b) = &input.brand_id {
            exists(tx, "brands", b, "brandId")?;
        }
        if let Some(t) = &input.tax_rate_id {
            exists(tx, "tax_rates", t, "taxRateId")?;
        }
        let creating = input.id.is_none();
        let product_id = match &input.id {
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO products (id, name, description, category_id, brand_id, base_unit_id, tax_rate_id, hsn_sac,
                        has_variants, track_batches, track_expiry, is_service, is_active, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?15)",
                    params![
                        id, name, description, input.category_id, input.brand_id, input.base_unit_id, input.tax_rate_id, hsn_sac,
                        has_variants, input.track_batches, input.track_expiry, input.is_service, input.is_active, now, actor.user_id
                    ],
                )?;
                id
            }
            Some(id) => {
                let updated_at: Option<String> = tx
                    .query_row(
                        "SELECT updated_at FROM products WHERE id = ?1 AND deleted_at IS NULL",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let updated_at = updated_at.ok_or_else(|| AppError::not_found("The product"))?;
                if input
                    .expected_updated_at
                    .as_ref()
                    .is_some_and(|e| *e != updated_at)
                {
                    return Err(AppError::conflict(
                        "Someone else changed this product. Reload it and try again.",
                    ));
                }
                tx.execute(
                    "UPDATE products SET name = ?1, description = ?2, category_id = ?3, brand_id = ?4, base_unit_id = ?5,
                       tax_rate_id = ?6, hsn_sac = ?7, has_variants = ?8, track_batches = ?9, track_expiry = ?10, is_service = ?11,
                       is_active = ?12, updated_at = ?13, updated_by = ?14 WHERE id = ?15",
                    params![
                        name, description, input.category_id, input.brand_id, input.base_unit_id, input.tax_rate_id, hsn_sac,
                        has_variants, input.track_batches, input.track_expiry, input.is_service, input.is_active, now, actor.user_id, id
                    ],
                )?;
                id.clone()
            }
        };

        // Variants: update the ones sent, add new ones, retire the ones left out (stock must be 0).
        let existing: Vec<String> = tx
            .prepare(
                "SELECT id FROM product_variants WHERE product_id = ?1 AND deleted_at IS NULL",
            )?
            .query_map([&product_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let kept: HashSet<&str> = variants
            .iter()
            .filter_map(|v| v.input.id.as_deref())
            .collect();
        for old in existing.iter().filter(|id| !kept.contains(id.as_str())) {
            if total_stock(tx, old)? != 0 {
                return Err(AppError::conflict(
                    "A variant you removed still has stock. Adjust its stock to zero first.",
                ));
            }
            tx.execute(
                "UPDATE product_variants SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
                params![now, actor.user_id, old],
            )?;
        }
        let (_, warehouse_id) = default_location(tx)?;
        let mut opening_value = 0;
        for v in &variants {
            let variant_id = match &v.input.id {
                Some(id) if existing.contains(id) => {
                    let (old_price, old_mrp): (i64, Option<i64>) = tx.query_row(
                        "SELECT sale_price, mrp FROM product_variants WHERE id = ?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    tx.execute(
                        "UPDATE product_variants SET variant_name = ?1, sku = ?2, barcode = ?3, sale_price = ?4, mrp = ?5,
                           reorder_level = ?6, reorder_qty = ?7, is_active = ?8, updated_at = ?9, updated_by = ?10 WHERE id = ?11",
                        params![
                            v.variant_name, v.sku, v.barcode, v.input.sale_price, v.input.mrp, v.input.reorder_level,
                            v.input.reorder_qty, v.input.is_active, now, actor.user_id, id
                        ],
                    )?;
                    if old_price != v.input.sale_price || old_mrp != v.input.mrp {
                        AuditEntry::new("PRICE_CHANGE", "product_variants", Some(id))
                            .by(actor)
                            .approved_by(approved_by.as_deref())
                            .before(json!({ "salePrice": old_price, "mrp": old_mrp }))
                            .after(json!({ "salePrice": v.input.sale_price, "mrp": v.input.mrp }))
                            .write(tx, &now)?;
                    }
                    id.clone()
                }
                Some(_) => {
                    return Err(AppError::field(
                        "variants",
                        "A variant doesn't belong to this product.",
                    ))
                }
                None => insert_variant(tx, &product_id, v, &now, &actor.user_id)?,
            };
            let opening = if v.input.id.is_none() {
                v.input.opening_stock.unwrap_or(0)
            } else {
                0
            };
            if opening != 0 {
                let cost = v.input.cost_price.unwrap_or(0);
                let batch = if input.track_batches {
                    stock_posting::batch_for(
                        tx,
                        &variant_id,
                        "variants.opening",
                        v.input.opening_batch_no.as_deref(),
                        None,
                        v.input.opening_expiry_date.as_deref(),
                        &now,
                        &actor.user_id,
                    )?
                } else {
                    None
                };
                let posted = stock_posting::post(
                    tx,
                    settings.inventory.negative_stock_policy,
                    &today,
                    &now,
                    Some(&actor.user_id),
                    &Move {
                        warehouse_id: &warehouse_id,
                        variant_id: &variant_id,
                        batch_id: batch.as_deref(),
                        qty: opening,
                        unit_cost: cost,
                        kind: "opening",
                        source_type: "product",
                        source_id: &product_id,
                        source_line_id: Some(&variant_id),
                        note: Some("Opening stock"),
                        ..Move::default()
                    },
                )?;
                opening_value += posted.value;
            }
        }
        journal::post(
            tx,
            Entry {
                business_date: &today,
                source_type: "product_opening",
                source_id: &product_id,
                narration: format!("Opening stock: {name}"),
                lines: vec![
                    JLine::debit(accounts::INVENTORY, opening_value),
                    JLine::credit(accounts::OPENING_BALANCES, opening_value),
                ],
                user_id: Some(&actor.user_id),
                now: &now,
            },
        )?;

        tx.execute(
            "UPDATE unit_conversions SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE product_id = ?3 AND deleted_at IS NULL",
            params![now, actor.user_id, product_id],
        )?;
        for c in &input.unit_conversions {
            exists(tx, "units", &c.from_unit_id, "unitConversions.fromUnitId")?;
            tx.execute(
                "INSERT INTO unit_conversions (id, product_id, from_unit_id, to_unit_id, factor_num, factor_den, barcode,
                    created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
                params![new_id(), product_id, c.from_unit_id, input.base_unit_id, c.factor_num, c.factor_den, c.barcode, now, actor.user_id],
            )?;
        }
        AuditEntry::new(
            if creating {
                "PRODUCT_CREATED"
            } else {
                "PRODUCT_UPDATED"
            },
            "products",
            Some(&product_id),
        )
        .by(actor)
        .summary(name.clone())
        .write(tx, &now)?;
        Ok(product_id)
    })?;
    get_product_unchecked(ctx.conn, &product_id)
}

pub fn delete_product(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("products.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let product = get_product_unchecked(tx, &input.id)?;
        if product.variants.iter().any(|v| v.stock != 0) {
            return Err(AppError::conflict(
                "This product still has stock. Adjust it to zero before deleting.",
            ));
        }
        tx.execute(
            "UPDATE product_variants SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE product_id = ?3 AND deleted_at IS NULL",
            params![now, actor.user_id, input.id],
        )?;
        tx.execute(
            "UPDATE products SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("PRODUCT_DELETED", "products", Some(&input.id))
            .by(actor)
            .summary(product.name)
            .write(tx, &now)
    })
}

pub fn get_product(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<ProductDetail> {
    ctx.require("products.view")?;
    get_product_unchecked(ctx.conn, &input.id)
}

fn get_product_unchecked(conn: &Connection, id: &str) -> AppResult<ProductDetail> {
    let mut detail = conn
        .query_row(
            "SELECT p.id, p.name, p.description, p.category_id, c.name, p.brand_id, b.name, p.base_unit_id, u.code,
                    p.tax_rate_id, coalesce(t.rate, 0), p.hsn_sac, p.track_batches, p.track_expiry, p.is_service, p.is_active, p.updated_at, p.image_path
             FROM products p
             JOIN units u ON u.id = p.base_unit_id
             LEFT JOIN categories c ON c.id = p.category_id
             LEFT JOIN brands b ON b.id = p.brand_id
             LEFT JOIN tax_rates t ON t.id = p.tax_rate_id
             WHERE p.id = ?1 AND p.deleted_at IS NULL",
            [id],
            |r| {
                Ok(ProductDetail {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    category_id: r.get(3)?,
                    category_name: r.get(4)?,
                    brand_id: r.get(5)?,
                    brand_name: r.get(6)?,
                    base_unit_id: r.get(7)?,
                    base_unit_code: r.get(8)?,
                    tax_rate_id: r.get(9)?,
                    tax_rate: r.get(10)?,
                    hsn_sac: r.get(11)?,
                    track_batches: r.get(12)?,
                    track_expiry: r.get(13)?,
                    is_service: r.get(14)?,
                    is_active: r.get(15)?,
                    updated_at: r.get(16)?,
                    image_key: r.get(17)?,
                    variants: Vec::new(),
                    unit_conversions: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The product"))?;
    let mut stmt = conn.prepare(
        "SELECT id, variant_name, sku, barcode, sale_price, mrp, cost_price, reorder_level, reorder_qty, is_active
         FROM product_variants WHERE product_id = ?1 AND deleted_at IS NULL ORDER BY created_at, id",
    )?;
    let variants: Vec<VariantDetail> = stmt
        .query_map([id], |r| {
            Ok(VariantDetail {
                id: r.get(0)?,
                variant_name: r.get(1)?,
                sku: r.get(2)?,
                barcode: r.get(3)?,
                sale_price: r.get(4)?,
                mrp: r.get(5)?,
                cost_price: r.get(6)?,
                reorder_level: r.get(7)?,
                reorder_qty: r.get(8)?,
                is_active: r.get(9)?,
                stock: 0,
                stock_by_warehouse: Vec::new(),
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut by_warehouse = conn.prepare(
        "SELECT l.warehouse_id, w.name, sum(l.qty) FROM stock_levels l JOIN warehouses w ON w.id = l.warehouse_id
         WHERE l.variant_id = ?1 GROUP BY l.warehouse_id ORDER BY w.name",
    )?;
    for mut v in variants {
        v.stock_by_warehouse = by_warehouse
            .query_map([&v.id], |r| {
                Ok(WarehouseStock {
                    warehouse_id: r.get(0)?,
                    warehouse_name: r.get(1)?,
                    qty: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        v.stock = v.stock_by_warehouse.iter().map(|w| w.qty).sum();
        detail.variants.push(v);
    }
    detail.unit_conversions = conn
        .prepare(
            "SELECT c.from_unit_id, u.code, c.factor_num, c.factor_den, c.barcode FROM unit_conversions c
             JOIN units u ON u.id = c.from_unit_id WHERE c.product_id = ?1 AND c.deleted_at IS NULL ORDER BY u.code",
        )?
        .query_map([id], |r| {
            Ok(UnitConversionDetail { from_unit_id: r.get(0)?, from_unit_code: r.get(1)?, factor_num: r.get(2)?, factor_den: r.get(3)?, barcode: r.get(4)? })
        })?
        .collect::<Result<_, _>>()?;
    Ok(detail)
}

/// FTS5 query for prefix search: each word becomes `"word"*`, all words must match.
pub fn fts_query(term: &str) -> Option<String> {
    let tokens: Vec<String> = term
        .split_whitespace()
        .map(|t| {
            t.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .take(8)
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

pub fn list_products(ctx: &Ctx<'_>, query: &ProductListQuery) -> AppResult<Page<ProductListItem>> {
    ctx.require("products.view")?;
    let mut filters = vec!["p.deleted_at IS NULL", "v.deleted_at IS NULL"];
    let mut args: Vec<SqlValue> = Vec::new();
    if !query.include_inactive {
        filters.push("p.is_active = 1 AND v.is_active = 1");
    }
    if let Some(category) = query.category_id.as_deref().filter(|c| !c.is_empty()) {
        filters.push("p.category_id = ?");
        args.push(category.to_string().into());
    }
    if let Some(brand) = query.brand_id.as_deref().filter(|b| !b.is_empty()) {
        filters.push("p.brand_id = ?");
        args.push(brand.to_string().into());
    }
    if query.low_stock_only {
        filters.push("v.reorder_level > 0 AND (SELECT coalesce(sum(qty), 0) FROM stock_levels l WHERE l.variant_id = v.id) <= v.reorder_level");
    }
    if let Some(term) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        match fts_query(term) {
            Some(fts) => {
                filters.push("(v.id IN (SELECT variant_id FROM product_search WHERE product_search MATCH ?) OR v.barcode = ? OR lower(v.sku) = lower(?))");
                args.extend([fts.into(), term.to_string().into(), term.to_string().into()]);
            }
            None => {
                filters.push("(v.barcode = ? OR lower(v.sku) = lower(?))");
                args.extend([term.to_string().into(), term.to_string().into()]);
            }
        }
    }
    let from = format!(
        "FROM product_variants v JOIN products p ON p.id = v.product_id
         JOIN units u ON u.id = p.base_unit_id
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN brands b ON b.id = p.brand_id
         LEFT JOIN tax_rates t ON t.id = p.tax_rate_id
         WHERE {}",
        filters.join(" AND ")
    );
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) {from}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let stock = "(SELECT coalesce(sum(qty), 0) FROM stock_levels l WHERE l.variant_id = v.id)";
    let keys = [
        ("name", "lower(p.name), lower(coalesce(v.variant_name, ''))"),
        ("sku", "lower(v.sku)"),
        ("category", "lower(coalesce(c.name, ''))"),
        ("price", "v.sale_price"),
        ("cost", "v.cost_price"),
        ("tax", "coalesce(t.rate, 0)"),
        ("stock", stock),
        ("updated", "max(p.updated_at, v.updated_at)"),
    ];
    let order = query.page.order_by(
        &keys,
        "lower(p.name), lower(coalesce(v.variant_name, '')), v.id",
        "v.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    let mut page_args = args.clone();
    page_args.extend([limit.into(), offset.into()]);
    let mut stmt = ctx.conn.prepare(&format!(
        "SELECT p.id, v.id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, v.barcode, c.name, b.name, u.code,
                v.sale_price, v.mrp, v.cost_price, coalesce(t.rate, 0),
                (SELECT coalesce(sum(qty), 0) FROM stock_levels l WHERE l.variant_id = v.id), v.reorder_level,
                p.is_active AND v.is_active, max(p.updated_at, v.updated_at), p.image_path
         {from} ORDER BY {order} LIMIT ? OFFSET ?"
    ))?;
    let items = stmt
        .query_map(params_from_iter(page_args.iter()), |r| {
            let stock: i64 = r.get(12)?;
            let reorder: i64 = r.get(13)?;
            Ok(ProductListItem {
                product_id: r.get(0)?,
                variant_id: r.get(1)?,
                name: r.get(2)?,
                sku: r.get(3)?,
                barcode: r.get(4)?,
                category_name: r.get(5)?,
                brand_name: r.get(6)?,
                unit_code: r.get(7)?,
                sale_price: r.get(8)?,
                mrp: r.get(9)?,
                cost_price: r.get(10)?,
                tax_rate: r.get(11)?,
                stock,
                reorder_level: reorder,
                is_active: r.get(14)?,
                is_low_stock: reorder > 0 && stock <= reorder,
                updated_at: r.get(15)?,
                image_key: r.get(16)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub term: String,
    pub limit: Option<u32>,
    /// Stock is reported for this store (default: the main store).
    pub warehouse_id: Option<String>,
}

/// One sellable line for the POS: the variant, in the unit the code identified (pack barcodes
/// resolve to the pack, with its factor and pack price).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchItem {
    pub variant_id: String,
    pub product_id: String,
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub unit_id: String,
    pub unit_code: String,
    pub factor_num: i64,
    pub factor_den: i64,
    /// Price per sold unit (pack price for packs).
    pub sale_price: i64,
    pub mrp: Option<i64>,
    pub tax_rate_id: Option<String>,
    pub tax_rate: i64,
    pub tax_type: Option<String>,
    pub cess_rate: i64,
    pub hsn_sac: Option<String>,
    /// Base units in the store.
    pub stock: i64,
    pub is_service: bool,
    pub track_batches: bool,
    /// The picture's name, or null.
    #[serde(default)]
    pub image_key: Option<String>,
}

const SEARCH_SELECT: &str = "SELECT v.id, p.id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, v.barcode,
    u.id, u.code, v.sale_price, v.mrp, p.tax_rate_id, coalesce(t.rate, 0), t.tax_type, coalesce(t.cess_rate, 0), p.hsn_sac,
    coalesce((SELECT sum(qty) FROM stock_levels l WHERE l.variant_id = v.id AND l.warehouse_id = ?1), 0),
    p.is_service, p.track_batches, p.image_path
    FROM product_variants v JOIN products p ON p.id = v.product_id JOIN units u ON u.id = p.base_unit_id
    LEFT JOIN tax_rates t ON t.id = p.tax_rate_id
    WHERE v.deleted_at IS NULL AND p.deleted_at IS NULL AND v.is_active = 1 AND p.is_active = 1";

fn map_search(r: &rusqlite::Row<'_>) -> rusqlite::Result<SearchItem> {
    Ok(SearchItem {
        variant_id: r.get(0)?,
        product_id: r.get(1)?,
        name: r.get(2)?,
        sku: r.get(3)?,
        barcode: r.get(4)?,
        unit_id: r.get(5)?,
        unit_code: r.get(6)?,
        factor_num: 1,
        factor_den: 1,
        sale_price: r.get(7)?,
        mrp: r.get(8)?,
        tax_rate_id: r.get(9)?,
        tax_rate: r.get(10)?,
        tax_type: r.get(11)?,
        cess_rate: r.get(12)?,
        hsn_sac: r.get(13)?,
        stock: r.get(14)?,
        is_service: r.get(15)?,
        track_batches: r.get(16)?,
        image_key: r.get(17)?,
    })
}

/// Exact match on a variant barcode, a pack barcode, or a SKU.
fn exact_lookup(
    conn: &Connection,
    code: &str,
    warehouse_id: &str,
) -> AppResult<Option<SearchItem>> {
    let code = code.trim();
    if code.is_empty() {
        return Ok(None);
    }
    if let Some(item) = conn
        .query_row(
            &format!("{SEARCH_SELECT} AND v.barcode = ?2"),
            params![warehouse_id, code],
            map_search,
        )
        .optional()?
    {
        return Ok(Some(item));
    }
    let pack: Option<(String, String, String, i64, i64)> = conn
        .query_row(
            "SELECT v.id, c.from_unit_id, u.code, c.factor_num, c.factor_den FROM unit_conversions c
             JOIN product_variants v ON v.product_id = c.product_id AND v.deleted_at IS NULL
             JOIN units u ON u.id = c.from_unit_id
             WHERE c.barcode = ?1 AND c.deleted_at IS NULL ORDER BY v.created_at LIMIT 1",
            [code],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    if let Some((variant_id, unit_id, unit_code, num, den)) = pack {
        if let Some(mut item) = conn
            .query_row(
                &format!("{SEARCH_SELECT} AND v.id = ?2"),
                params![warehouse_id, variant_id],
                map_search,
            )
            .optional()?
        {
            item.unit_id = unit_id;
            item.unit_code = unit_code;
            item.factor_num = num;
            item.factor_den = den;
            item.sale_price = i64::try_from(money::div_round(
                i128::from(item.sale_price) * i128::from(num),
                i128::from(den),
                RoundingMode::HalfUp,
            ))
            .map_err(AppError::internal)?;
            item.mrp = item.mrp.map(|m| m * num / den);
            return Ok(Some(item));
        }
    }
    Ok(conn
        .query_row(
            &format!("{SEARCH_SELECT} AND lower(v.sku) = lower(?2)"),
            params![warehouse_id, code],
            map_search,
        )
        .optional()?)
}

fn warehouse_or_default(conn: &Connection, warehouse_id: Option<&str>) -> AppResult<String> {
    match warehouse_id {
        Some(w) => Ok(w.to_string()),
        None => Ok(default_location(conn)?.1),
    }
}

pub fn lookup_code(ctx: &Ctx<'_>, query: &SearchQuery) -> AppResult<Option<SearchItem>> {
    ctx.require("products.view")?;
    let warehouse = warehouse_or_default(ctx.conn, query.warehouse_id.as_deref())?;
    exact_lookup(ctx.conn, &query.term, &warehouse)
}

/// POS search: exact barcode/SKU first, then word-prefix matches. Uses only indexed lookups, so it
/// stays fast with 50,000 products (Master Prompt §12: < 100 ms).
pub fn search(ctx: &Ctx<'_>, query: &SearchQuery) -> AppResult<Vec<SearchItem>> {
    ctx.require("products.view")?;
    let limit = i64::from(query.limit.unwrap_or(20).clamp(1, 50));
    let warehouse = warehouse_or_default(ctx.conn, query.warehouse_id.as_deref())?;
    let mut results = Vec::new();
    if let Some(item) = exact_lookup(ctx.conn, &query.term, &warehouse)? {
        results.push(item);
    }
    if let Some(fts) = fts_query(&query.term) {
        let mut stmt = ctx.conn.prepare(&format!(
            "{SEARCH_SELECT} AND v.id IN (SELECT variant_id FROM product_search WHERE product_search MATCH ?2 ORDER BY rank LIMIT ?3)
             ORDER BY lower(p.name) LIMIT ?3"
        ))?;
        for item in stmt.query_map(params![warehouse, fts, limit], map_search)? {
            let item = item?;
            if !results
                .iter()
                .any(|r: &SearchItem| r.variant_id == item.variant_id)
            {
                results.push(item);
            }
        }
    }
    results.truncate(usize::try_from(limit).unwrap_or(20));
    Ok(results)
}

/// Everything a counter needs to bill while the hub is offline: one entry per sellable variant
/// in its base unit, plus one per product pack size (with the pack price and pack barcode).
pub(crate) fn offline_catalogue(
    conn: &Connection,
    warehouse_id: &str,
) -> AppResult<Vec<SearchItem>> {
    let mut items = conn
        .prepare(&format!("{SEARCH_SELECT} ORDER BY lower(p.name), v.sku"))?
        .query_map([warehouse_id], map_search)?
        .collect::<Result<Vec<_>, _>>()?;
    let packs = conn
        .prepare(
            "SELECT v.id, c.from_unit_id, u.code, c.factor_num, c.factor_den, c.barcode FROM unit_conversions c
             JOIN product_variants v ON v.product_id = c.product_id AND v.deleted_at IS NULL
             JOIN units u ON u.id = c.from_unit_id
             WHERE c.product_id IS NOT NULL AND c.deleted_at IS NULL ORDER BY c.created_at",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut pack_items = Vec::with_capacity(packs.len());
    for (variant_id, unit_id, unit_code, num, den, barcode) in packs {
        let Some(base) = items.iter().find(|i| i.variant_id == variant_id) else {
            continue;
        };
        let mut item = base.clone();
        item.unit_id = unit_id;
        item.unit_code = unit_code;
        item.factor_num = num;
        item.factor_den = den;
        item.barcode = barcode;
        item.sale_price = i64::try_from(money::div_round(
            i128::from(item.sale_price) * i128::from(num),
            i128::from(den),
            RoundingMode::HalfUp,
        ))
        .map_err(AppError::internal)?;
        item.mrp = item.mrp.map(|m| m * num / den);
        pack_items.push(item);
    }
    items.extend(pack_items);
    Ok(items)
}
