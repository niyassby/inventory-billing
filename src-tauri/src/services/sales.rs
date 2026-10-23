//! Billing: sales invoices, voids, returns / credit notes, quotations and orders, held bills.
//!
//! The backend prices every document itself (domain::pricing) from catalogue data; the client's
//! numbers are only a preview (`expectedTotal` → TOTALS_MISMATCH if they differ). A finalized
//! invoice is immutable: corrections are voids or credit notes.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::audit::AuditEntry;
use super::common::{
    date, like_pattern, optional, optional_gstin, optional_phone, required, Page, PageQuery,
};
use super::context::{Actor, Approver};
use super::idempotency;
use super::journal::{self, Entry, JLine};
use super::numbering;
use super::party_ledger::{self, PartyRef, Posting};
use super::period;
use super::settings::{self, Settings, TaxRegime};
use super::stock_posting::{self, Move, StockWarning};
use super::users::IdInput;
use super::{accounts, write, Ctx};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::domain::pricing::{
    self, Discount, DocumentInput, DocumentResult, LineInput, TaxComponentRate,
};
use crate::error::{AppError, AppResult, ErrorCode};

pub const PAYMENT_MODES: [&str; 7] = [
    "cash",
    "card",
    "upi",
    "bank_transfer",
    "cheque",
    "wallet",
    "other",
];

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaleLineInput {
    pub variant_id: String,
    /// Sold unit (a pack size); default: the product's base unit.
    pub unit_id: Option<String>,
    /// Quantity in the sold unit, thousandths.
    pub qty: i64,
    /// Price per sold unit; leave empty to use the catalogue price. A different price needs
    /// `sales.price_override`.
    pub unit_price: Option<i64>,
    pub discount: Option<Discount>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentInput {
    /// cash, card, upi, bank_transfer, cheque, wallet or other.
    pub mode: String,
    /// Amount applied to the document.
    pub amount: i64,
    /// Cash handed over (≥ amount); the difference is change, never "paid".
    pub tendered: Option<i64>,
    /// Cash or bank account; default by mode.
    pub account_id: Option<String>,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaleInput {
    pub idempotency_key: String,
    pub warehouse_id: Option<String>,
    pub party_id: Option<String>,
    /// Walk-in customer details (ignored when a party is chosen).
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub customer_gstin: Option<String>,
    /// Two-digit state code of the place of supply; decides CGST+SGST vs IGST.
    pub place_of_supply: Option<String>,
    /// Defaults to today; another date needs `sales.backdate`.
    pub business_date: Option<String>,
    pub prices_include_tax: Option<bool>,
    pub lines: Vec<SaleLineInput>,
    pub bill_discount: Option<Discount>,
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
    /// The total the screen showed; a different backend total fails with TOTALS_MISMATCH.
    pub expected_total: Option<i64>,
    pub notes: Option<String>,
    /// Quotation / sales order being converted.
    pub source_document_id: Option<String>,
    pub register_session_id: Option<String>,
}

// ------------------------------------------------------------------ line resolution

pub(crate) struct ResolvedLine {
    pub variant_id: String,
    pub name: String,
    pub sku: String,
    pub hsn_sac: Option<String>,
    pub unit_id: String,
    pub unit_name: String,
    /// The product's base unit (price lists may price it or a pack).
    pub base_unit_id: String,
    pub factor_num: i64,
    pub factor_den: i64,
    pub qty: i64,
    pub base_qty: i64,
    /// Catalogue price per sold unit.
    pub catalog_price: i64,
    pub unit_price: i64,
    pub mrp: Option<i64>,
    pub discount: Option<Discount>,
    pub tax_rate_id: Option<String>,
    pub tax_rate: i64,
    pub tax_type: Option<String>,
    pub cess_rate: i64,
    /// Current average cost per base unit.
    pub cost: i64,
    pub is_service: bool,
    pub batch_id: Option<String>,
}

fn scale(value: i64, num: i64, den: i64) -> AppResult<i64> {
    i64::try_from(money::div_round(
        i128::from(value) * i128::from(num),
        i128::from(den),
        RoundingMode::HalfUp,
    ))
    .map_err(AppError::internal)
}

/// Loads catalogue data for each line and converts pack quantities to base units.
pub(crate) fn resolve_line(
    conn: &Connection,
    index: usize,
    variant_id: &str,
    unit_id: Option<&str>,
    qty: i64,
    batch_id: Option<&str>,
) -> AppResult<ResolvedLine> {
    let field = |f: &str| format!("lines.{index}.{f}");
    if qty <= 0 {
        return Err(AppError::field(
            &field("qty"),
            "Quantity must be greater than zero.",
        ));
    }
    #[allow(clippy::type_complexity, reason = "one flat SQL row, decoded once below")]
    let row: Option<(String, String, String, Option<String>, String, String, i64, Option<i64>, Option<String>, i64, Option<String>, i64, i64, bool, String)> = conn
        .query_row(
            "SELECT v.id, p.name || coalesce(' — ' || v.variant_name, ''), v.sku, p.hsn_sac, p.base_unit_id, u.code, v.sale_price, v.mrp,
                    p.tax_rate_id, coalesce(t.rate, 0), t.tax_type, coalesce(t.cess_rate, 0), v.cost_price, p.is_service, p.id
             FROM product_variants v JOIN products p ON p.id = v.product_id JOIN units u ON u.id = p.base_unit_id
             LEFT JOIN tax_rates t ON t.id = p.tax_rate_id AND t.is_active = 1
             WHERE v.id = ?1 AND v.deleted_at IS NULL AND p.deleted_at IS NULL AND v.is_active = 1 AND p.is_active = 1",
            [variant_id],
            |r| {
                Ok((
                    r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?,
                    r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?,
                ))
            },
        )
        .optional()?;
    let Some((
        variant_id,
        name,
        sku,
        hsn_sac,
        base_unit,
        base_code,
        sale_price,
        mrp,
        tax_rate_id,
        tax_rate,
        tax_type,
        cess_rate,
        cost,
        is_service,
        product_id,
    )) = row
    else {
        return Err(AppError::field(
            &field("variantId"),
            "This product is not available for sale.",
        ));
    };
    let base_unit_id = base_unit.clone();
    let (unit_id, unit_name, num, den) = match unit_id.filter(|u| *u != base_unit) {
        None => (base_unit, base_code, 1, 1),
        Some(unit) => conn
            .query_row(
                "SELECT c.from_unit_id, u.code, c.factor_num, c.factor_den FROM unit_conversions c JOIN units u ON u.id = c.from_unit_id
                 WHERE c.from_unit_id = ?1 AND c.to_unit_id = ?2 AND (c.product_id = ?3 OR c.product_id IS NULL) AND c.deleted_at IS NULL
                 ORDER BY c.product_id IS NULL LIMIT 1",
                params![unit, base_unit, product_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::field(&field("unitId"), "This product isn't sold in that unit."))?,
    };
    let base_qty = scale(qty, num, den)?;
    if base_qty <= 0 {
        return Err(AppError::field(&field("qty"), "Quantity is too small."));
    }
    if let Some(batch) = batch_id {
        let ok: i64 = conn.query_row(
            "SELECT count(*) FROM batches WHERE id = ?1 AND variant_id = ?2 AND deleted_at IS NULL",
            params![batch, variant_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::field(
                &field("batchId"),
                "That batch doesn't belong to this product.",
            ));
        }
    }
    let catalog_price = scale(sale_price, num, den)?;
    Ok(ResolvedLine {
        variant_id,
        name,
        sku,
        hsn_sac,
        unit_id,
        unit_name,
        base_unit_id,
        factor_num: num,
        factor_den: den,
        qty,
        base_qty,
        catalog_price,
        unit_price: catalog_price,
        mrp: mrp.map(|m| scale(m, num, den)).transpose()?,
        discount: None,
        tax_rate_id,
        tax_rate,
        tax_type,
        cess_rate,
        cost,
        is_service,
        batch_id: batch_id.map(String::from),
    })
}

/// Tax components for a line under the business's regime.
pub(crate) fn tax_components(
    regime: TaxRegime,
    tax_type: Option<&str>,
    rate: i64,
    cess: i64,
    inter_state: bool,
) -> AppResult<Vec<TaxComponentRate>> {
    if regime == TaxRegime::None {
        return Ok(Vec::new());
    }
    let mut components = match tax_type {
        Some("gst") => {
            pricing::gst_components(rate, inter_state).map_err(|e| AppError::validation(e.0))?
        }
        Some("vat") if rate > 0 => vec![TaxComponentRate {
            code: "VAT".into(),
            rate,
        }],
        Some("other") if rate > 0 => vec![TaxComponentRate {
            code: "TAX".into(),
            rate,
        }],
        _ => Vec::new(),
    };
    if cess > 0 {
        components.push(TaxComponentRate {
            code: "CESS".into(),
            rate: cess,
        });
    }
    Ok(components)
}

pub(crate) struct TaxColumns {
    pub cgst: i64,
    pub sgst: i64,
    pub igst: i64,
    pub cess: i64,
    pub other: i64,
}

pub(crate) fn tax_columns(taxes: &[pricing::TaxAmount]) -> TaxColumns {
    let mut cols = TaxColumns {
        cgst: 0,
        sgst: 0,
        igst: 0,
        cess: 0,
        other: 0,
    };
    for t in taxes {
        match t.code.as_str() {
            "CGST" => cols.cgst += t.amount,
            "SGST" => cols.sgst += t.amount,
            "IGST" => cols.igst += t.amount,
            "CESS" => cols.cess += t.amount,
            _ => cols.other += t.amount,
        }
    }
    cols
}

pub(crate) fn branch_of(
    conn: &Connection,
    warehouse_id: Option<&str>,
) -> AppResult<(String, String, Option<String>)> {
    let row: Option<(String, String, Option<String>)> = match warehouse_id {
        Some(w) => conn
            .query_row(
                "SELECT b.id, w.id, b.state_code FROM warehouses w JOIN branches b ON b.id = w.branch_id WHERE w.id = ?1 AND w.deleted_at IS NULL",
                [w],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        None => {
            let (branch, warehouse) = super::setup::default_location(conn)?;
            let state: Option<String> = conn.query_row("SELECT state_code FROM branches WHERE id = ?1", [&branch], |r| r.get(0))?;
            Some((branch, warehouse, state))
        }
    };
    row.ok_or_else(|| AppError::field("warehouseId", "Choose an existing store."))
}

/// A payment that passed validation, with its cash/bank account resolved.
#[derive(Debug, Clone)]
pub(crate) struct ValidPayment {
    pub mode: String,
    pub amount: i64,
    pub tendered: Option<i64>,
    pub account_id: String,
    pub reference: Option<String>,
}

pub(crate) fn validate_payments(
    conn: &Connection,
    payments: &[PaymentInput],
) -> AppResult<Vec<ValidPayment>> {
    payments
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if !PAYMENT_MODES.contains(&p.mode.as_str()) {
                return Err(AppError::field(
                    &format!("payments.{i}.mode"),
                    "Choose a payment mode.",
                ));
            }
            if p.amount <= 0 {
                return Err(AppError::field(
                    &format!("payments.{i}.amount"),
                    "Enter an amount greater than zero.",
                ));
            }
            if let Some(tendered) = p.tendered {
                if p.mode != "cash" {
                    return Err(AppError::field(
                        &format!("payments.{i}.tendered"),
                        "Only cash has change.",
                    ));
                }
                if tendered < p.amount {
                    return Err(AppError::field(
                        &format!("payments.{i}.tendered"),
                        "Cash handed over is less than the amount.",
                    ));
                }
            }
            let account = accounts::payment_account(conn, p.account_id.as_deref(), &p.mode)?;
            let reference = optional(
                &format!("payments.{i}.reference"),
                p.reference.as_deref(),
                60,
            )?;
            Ok(ValidPayment {
                mode: p.mode.clone(),
                amount: p.amount,
                tendered: p.tendered,
                account_id: account,
                reference,
            })
        })
        .collect()
}

/// Inserts a finalized payment, allocates it, and posts ledger + journal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_payment(
    tx: &Connection,
    settings: &Settings,
    branch_id: &str,
    business_date: &str,
    direction: &str,
    party_id: Option<&str>,
    payment: &ValidPayment,
    allocation: Option<(&str, &str)>,
    counter_account: &str,
    register_session_id: Option<&str>,
    actor: &Actor,
    now: &str,
) -> AppResult<String> {
    let ValidPayment {
        mode,
        amount,
        tendered,
        account_id: account,
        reference,
    } = payment;
    let number = numbering::next(
        tx,
        settings,
        branch_id,
        if direction == "in" {
            "payment_in"
        } else {
            "payment_out"
        },
        business_date,
        now,
    )?;
    let id = new_id();
    // Cash that moves while the person has a till open goes through that till, whatever screen
    // made the payment (a refund, a customer's receipt, an expense, a supplier's payment), so the
    // till's expected cash at closing counts it.
    let session: Option<String> = match register_session_id {
        Some(session) => Some(session.to_string()),
        None if mode == "cash" => tx
            .query_row(
                "SELECT id FROM register_sessions WHERE status = 'open' AND opened_by = ?1 ORDER BY opened_at DESC LIMIT 1",
                [&actor.user_id],
                |r| r.get(0),
            )
            .optional()?,
        None => None,
    };
    let register_session_id = session.as_deref();
    tx.execute(
        "INSERT INTO payments (id, branch_id, series_id, number_period, doc_seq, doc_no, business_date, direction, party_id, account_id, mode,
            amount, tendered, change_given, reference_no, register_session_id, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17, ?18, ?18)",
        params![
            id, branch_id, number.series_id, number.period, number.seq, number.doc_no, business_date, direction, party_id, account, mode,
            amount, tendered, tendered.map_or(0, |t| t - amount), reference, register_session_id, now, actor.user_id
        ],
    )?;
    if let Some((target_type, target_id)) = allocation {
        tx.execute(
            "INSERT INTO payment_allocations (id, payment_id, target_type, target_id, amount, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), id, target_type, target_id, amount, now, actor.user_id],
        )?;
    }
    if let Some(party) = party_id {
        party_ledger::post(
            tx,
            Posting {
                party_id: party,
                business_date,
                entry_type: if direction == "in" {
                    "payment_in"
                } else {
                    "payment_out"
                },
                source_type: "payment",
                source_id: &id,
                amount: if direction == "in" { -amount } else { *amount },
                narration: format!("Payment {}", number.doc_no),
                user_id: Some(&actor.user_id),
                now,
            },
        )?;
    }
    let lines = if direction == "in" {
        vec![
            JLine::debit(account, *amount),
            JLine::credit(counter_account, *amount).party(party_id),
        ]
    } else {
        vec![
            JLine::debit(counter_account, *amount).party(party_id),
            JLine::credit(account, *amount),
        ]
    };
    journal::post(
        tx,
        Entry {
            business_date,
            source_type: "payment",
            source_id: &id,
            narration: format!("Payment {}", number.doc_no),
            lines,
            user_id: Some(&actor.user_id),
            now,
        },
    )?;
    Ok(id)
}

/// Voids a payment and reverses its ledger and journal postings.
pub(crate) fn void_payment(
    tx: &Connection,
    payment_id: &str,
    business_date: &str,
    reason: &str,
    actor: &Actor,
    now: &str,
) -> AppResult<()> {
    let changed = tx.execute(
        "UPDATE payments SET status = 'voided', voided_at = ?1, voided_by = ?2, void_reason = ?3, updated_at = ?1, updated_by = ?2
         WHERE id = ?4 AND status = 'finalized'",
        params![now, actor.user_id, reason, payment_id],
    )?;
    if changed == 0 {
        return Err(AppError::conflict("This payment is already voided."));
    }
    party_ledger::reverse_source(
        tx,
        "payment",
        payment_id,
        business_date,
        "Payment voided",
        Some(&actor.user_id),
        now,
    )?;
    journal::reverse_source(
        tx,
        "payment",
        payment_id,
        business_date,
        "Payment voided",
        Some(&actor.user_id),
        now,
    )
}

pub(crate) struct Priced {
    pub lines: Vec<ResolvedLine>,
    pub result: DocumentResult,
    pub inter_state: bool,
    pub prices_include_tax: bool,
}

/// The price list on a bill and the date its prices are read for.
pub(crate) struct ListContext<'a> {
    pub list_id: Option<String>,
    pub date: &'a str,
}

#[allow(clippy::too_many_arguments)]
fn price(
    conn: &Connection,
    settings: &Settings,
    lines: &[SaleLineInput],
    bill_discount: Option<&Discount>,
    prices_include_tax: Option<bool>,
    place_of_supply: Option<&str>,
    branch_state: Option<&str>,
    list: &ListContext<'_>,
) -> AppResult<Priced> {
    price_lines(
        settings,
        lines,
        bill_discount,
        prices_include_tax,
        place_of_supply,
        branch_state,
        |index, line| {
            let mut r = resolve_line(
                conn,
                index,
                &line.variant_id,
                line.unit_id.as_deref(),
                line.qty,
                line.batch_id.as_deref(),
            )?;
            if let Some(list_id) = &list.list_id {
                if let Some(price) = super::price_lists::price_for(
                    super::price_lists::entries(conn, list_id, &r.variant_id)?,
                    &r.unit_id,
                    &r.base_unit_id,
                    r.factor_num,
                    r.factor_den,
                    r.qty,
                    list.date,
                ) {
                    r.catalog_price = price;
                    r.unit_price = price;
                }
            }
            Ok(r)
        },
    )
}

/// Prices a bill from resolved catalogue lines. The hub resolves lines from its database; an
/// offline counter resolves them from its cached catalogue. Both then run exactly this code, so a
/// bill made offline has the same totals the hub computes when it syncs.
pub(crate) fn price_lines(
    settings: &Settings,
    lines: &[SaleLineInput],
    bill_discount: Option<&Discount>,
    prices_include_tax: Option<bool>,
    place_of_supply: Option<&str>,
    branch_state: Option<&str>,
    mut resolve: impl FnMut(usize, &SaleLineInput) -> AppResult<ResolvedLine>,
) -> AppResult<Priced> {
    if lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one item."));
    }
    if lines.len() > 500 {
        return Err(AppError::field(
            "lines",
            "A bill can have at most 500 lines.",
        ));
    }
    let inter_state =
        matches!((place_of_supply, branch_state), (Some(pos), Some(own)) if pos != own);
    let mut resolved = Vec::with_capacity(lines.len());
    let mut inputs = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let mut r = resolve(index, line)?;
        if let Some(price) = line.unit_price {
            if price < 0 {
                return Err(AppError::field(
                    &format!("lines.{index}.unitPrice"),
                    "Price can't be negative.",
                ));
            }
            r.unit_price = price;
        }
        r.discount = line.discount.clone();
        inputs.push(LineInput {
            unit_price: r.unit_price,
            qty: r.qty,
            discount: r.discount.clone(),
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
    let include = prices_include_tax.unwrap_or(settings.tax.prices_include_tax);
    let result = pricing::price_document(&DocumentInput {
        lines: inputs,
        prices_include_tax: include,
        bill_discount: bill_discount.cloned(),
        round_total_to: (settings.tax.round_total_to > 0).then_some(settings.tax.round_total_to),
        rounding: Some(settings.tax.rounding.into()),
    })
    .map_err(|e| AppError::validation(e.0))?;
    Ok(Priced {
        lines: resolved,
        result,
        inter_state,
        prices_include_tax: include,
    })
}

/// Approvals a sale needs: discounts above the role limit, price overrides.
fn check_pricing_approvals(
    actor: &Actor,
    approver: Option<&Approver>,
    priced: &Priced,
    prices_preapproved: bool,
) -> AppResult<Option<String>> {
    let mut approved_by = None;
    let discount = priced.result.line_discount + priced.result.bill_discount;
    if discount > 0 {
        approved_by =
            super::context::approve(actor, approver, "sales.discount", "give a discount")?
                .or(approved_by);
        let rate = if priced.result.gross > 0 {
            i64::try_from(i128::from(discount) * 1_000_000 / i128::from(priced.result.gross))
                .unwrap_or(i64::MAX)
        } else {
            0
        };
        if rate > actor.max_discount_rate {
            approved_by = super::context::approve(
                actor,
                approver,
                "sales.discount_override",
                "discount above your limit",
            )?
            .or(approved_by);
        }
    }
    if !prices_preapproved && priced.lines.iter().any(|l| l.unit_price != l.catalog_price) {
        approved_by = super::context::approve(
            actor,
            approver,
            "sales.price_override",
            "change an item's price",
        )?
        .or(approved_by);
    }
    Ok(approved_by)
}

/// Where a returned quantity goes back into stock: the batches the original line was sold from
/// (the last one first), less what was already returned to each, at the cost each batch left at.
/// A line without batches goes back as it was sold.
pub(crate) fn return_batches(
    conn: &Connection,
    original_item_id: Option<&str>,
    batch_id: Option<&str>,
    qty: i64,
    unit_cost: i64,
) -> AppResult<Vec<(Option<String>, i64, i64)>> {
    let Some(item) = original_item_id else {
        return Ok(vec![(batch_id.map(String::from), qty, unit_cost)]);
    };
    let sold: Vec<(Option<String>, i64, i64)> = conn
        .prepare(
            "SELECT batch_id, -sum(qty), -sum(value) FROM stock_movements
             WHERE source_type = 'sales_invoice' AND source_line_id = ?1 AND movement_type = 'sale'
             GROUP BY batch_id ORDER BY max(rowid) DESC",
        )?
        .query_map([item], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    if sold.len() <= 1 {
        let batch = sold
            .first()
            .map_or(batch_id.map(String::from), |s| s.0.clone());
        let cost = sold
            .first()
            .filter(|s| s.1 > 0)
            .map_or(unit_cost, |s| stock_posting::per_unit(s.2, s.1));
        return Ok(vec![(batch, qty, cost)]);
    }
    let returned: Vec<(Option<String>, i64)> = conn
        .prepare(
            "SELECT m.batch_id, sum(m.qty) FROM stock_movements m JOIN sales_return_items ri ON ri.id = m.source_line_id
             WHERE m.source_type = 'sales_return' AND ri.original_item_id = ?1 GROUP BY m.batch_id",
        )?
        .query_map([item], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut left = qty;
    let mut out = Vec::new();
    for (batch, sold_qty, sold_value) in &sold {
        if left == 0 {
            break;
        }
        let back = returned.iter().find(|r| r.0 == *batch).map_or(0, |r| r.1);
        let room = (sold_qty - back).max(0);
        let q = left.min(room);
        if q > 0 {
            out.push((
                batch.clone(),
                q,
                stock_posting::per_unit(*sold_value, *sold_qty),
            ));
            left -= q;
        }
    }
    if left > 0 {
        match out.last_mut() {
            Some(last) => last.1 += left,
            None => out.push((
                sold[0].0.clone(),
                left,
                stock_posting::per_unit(sold[0].2, sold[0].1),
            )),
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ invoice read models

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLine {
    pub id: String,
    pub line_no: i64,
    pub variant_id: String,
    pub item_name: String,
    pub sku: Option<String>,
    pub hsn_sac: Option<String>,
    pub unit_name: Option<String>,
    pub qty: i64,
    pub unit_price: i64,
    pub mrp: Option<i64>,
    pub line_discount: i64,
    pub bill_discount_share: i64,
    pub taxable: i64,
    pub tax_rate: i64,
    pub cgst: i64,
    pub sgst: i64,
    pub igst: i64,
    pub other_tax: i64,
    pub cess: i64,
    pub tax_total: i64,
    pub line_total: i64,
    pub returned_qty: i64,
    /// Batch products: the batches the line came from (base-unit quantities).
    #[serde(default)]
    pub batches: Vec<LineBatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LineBatch {
    pub batch_no: String,
    pub expiry_date: Option<String>,
    pub qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentSummary {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub mode: String,
    pub amount: i64,
    pub tendered: Option<i64>,
    pub change_given: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxSummaryRow {
    pub tax_rate: i64,
    pub taxable: i64,
    pub cgst: i64,
    pub sgst: i64,
    pub igst: i64,
    pub cess: i64,
    pub other_tax: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDetail {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub status: String,
    pub payment_status: String,
    pub party_id: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub customer_gstin: Option<String>,
    pub billing_address: Option<String>,
    pub place_of_supply: Option<String>,
    pub is_inter_state: bool,
    pub prices_include_tax: bool,
    pub gross: i64,
    pub line_discount: i64,
    pub bill_discount: i64,
    pub taxable: i64,
    pub tax_total: i64,
    pub round_off: i64,
    pub total: i64,
    pub amount_paid: i64,
    pub balance_due: i64,
    pub notes: Option<String>,
    pub created_by_name: Option<String>,
    pub finalized_at: Option<String>,
    pub voided_at: Option<String>,
    pub void_reason: Option<String>,
    pub snapshot_source: String,
    /// "online", or "offline" for a bill a counter made while the hub was unreachable.
    #[serde(default = "online")]
    pub origin: String,
    pub lines: Vec<InvoiceLine>,
    pub payments: Vec<PaymentSummary>,
    pub tax_summary: Vec<TaxSummaryRow>,
    /// Items that went below zero stock under the "warn" policy (only on creation).
    #[serde(default)]
    pub warnings: Vec<StockWarning>,
}

fn online() -> String {
    "online".into()
}

/// Taxable value and tax per rate, for the invoice footer and GST returns.
pub(crate) fn tax_summary(lines: &[InvoiceLine]) -> Vec<TaxSummaryRow> {
    let mut summary: Vec<TaxSummaryRow> = Vec::new();
    for l in lines {
        let row = match summary.iter_mut().find(|s| s.tax_rate == l.tax_rate) {
            Some(row) => row,
            None => {
                summary.push(TaxSummaryRow {
                    tax_rate: l.tax_rate,
                    taxable: 0,
                    cgst: 0,
                    sgst: 0,
                    igst: 0,
                    cess: 0,
                    other_tax: 0,
                });
                summary.last_mut().expect("just pushed")
            }
        };
        row.taxable += l.taxable;
        row.cgst += l.cgst;
        row.sgst += l.sgst;
        row.igst += l.igst;
        row.cess += l.cess;
        row.other_tax += l.other_tax;
    }
    summary.sort_by_key(|s| s.tax_rate);
    summary
}

pub fn invoice_detail(conn: &Connection, id: &str) -> AppResult<InvoiceDetail> {
    let mut detail = conn
        .query_row(
            "SELECT i.id, i.doc_no, i.business_date, i.status, i.payment_status, i.party_id, i.customer_name, i.customer_phone, i.customer_gstin,
                    i.billing_address, i.place_of_supply, i.is_inter_state, i.prices_include_tax, i.gross, i.line_discount, i.bill_discount,
                    i.taxable, i.tax_total, i.round_off, i.total, i.amount_paid, i.balance_due, i.notes, u.name, i.finalized_at, i.voided_at,
                    i.void_reason, i.snapshot_source, i.origin
             FROM sales_invoices i LEFT JOIN users u ON u.id = i.created_by WHERE i.id = ?1 AND i.status <> 'draft'",
            [id],
            |r| {
                Ok(InvoiceDetail {
                    id: r.get(0)?,
                    doc_no: r.get(1)?,
                    business_date: r.get(2)?,
                    status: r.get(3)?,
                    payment_status: r.get(4)?,
                    party_id: r.get(5)?,
                    customer_name: r.get(6)?,
                    customer_phone: r.get(7)?,
                    customer_gstin: r.get(8)?,
                    billing_address: r.get(9)?,
                    place_of_supply: r.get(10)?,
                    is_inter_state: r.get(11)?,
                    prices_include_tax: r.get(12)?,
                    gross: r.get(13)?,
                    line_discount: r.get(14)?,
                    bill_discount: r.get(15)?,
                    taxable: r.get(16)?,
                    tax_total: r.get(17)?,
                    round_off: r.get(18)?,
                    total: r.get(19)?,
                    amount_paid: r.get(20)?,
                    balance_due: r.get(21)?,
                    notes: r.get(22)?,
                    created_by_name: r.get(23)?,
                    finalized_at: r.get(24)?,
                    voided_at: r.get(25)?,
                    void_reason: r.get(26)?,
                    snapshot_source: r.get(27)?,
                    origin: r.get(28)?,
                    lines: Vec::new(),
                    payments: Vec::new(),
                    tax_summary: Vec::new(),
                    warnings: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The invoice"))?;
    detail.lines = conn
        .prepare(
            "SELECT l.id, l.line_no, l.variant_id, l.item_name, l.sku, l.hsn_sac, l.unit_name, l.qty, l.unit_price, l.mrp, l.line_discount,
                    l.bill_discount_share, l.taxable, l.tax_rate, l.cgst, l.sgst, l.igst, l.other_tax, l.cess, l.tax_total, l.line_total,
                    coalesce((SELECT sum(ri.qty) FROM sales_return_items ri JOIN sales_returns sr ON sr.id = ri.return_id
                              WHERE ri.original_item_id = l.id AND sr.status = 'finalized'), 0)
             FROM sales_invoice_items l WHERE l.invoice_id = ?1 ORDER BY l.line_no",
        )?
        .query_map([id], |r| {
            Ok(InvoiceLine {
                id: r.get(0)?,
                line_no: r.get(1)?,
                variant_id: r.get(2)?,
                item_name: r.get(3)?,
                sku: r.get(4)?,
                hsn_sac: r.get(5)?,
                unit_name: r.get(6)?,
                qty: r.get(7)?,
                unit_price: r.get(8)?,
                mrp: r.get(9)?,
                line_discount: r.get(10)?,
                bill_discount_share: r.get(11)?,
                taxable: r.get(12)?,
                tax_rate: r.get(13)?,
                cgst: r.get(14)?,
                sgst: r.get(15)?,
                igst: r.get(16)?,
                other_tax: r.get(17)?,
                cess: r.get(18)?,
                tax_total: r.get(19)?,
                line_total: r.get(20)?,
                returned_qty: r.get(21)?,
                batches: Vec::new(),
            })
        })?
        .collect::<Result<_, _>>()?;
    {
        let mut stmt = conn.prepare(
            "SELECT b.batch_no, b.expiry_date, -sum(m.qty) FROM stock_movements m JOIN batches b ON b.id = m.batch_id
             WHERE m.source_type = 'sales_invoice' AND m.source_line_id = ?1 AND m.movement_type = 'sale'
             GROUP BY b.id ORDER BY b.expiry_date IS NULL, b.expiry_date, b.batch_no",
        )?;
        for line in &mut detail.lines {
            line.batches = stmt
                .query_map([&line.id], |r| {
                    Ok(LineBatch {
                        batch_no: r.get(0)?,
                        expiry_date: r.get(1)?,
                        qty: r.get(2)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
        }
    }
    detail.payments = conn
        .prepare(
            "SELECT p.id, p.doc_no, p.business_date, p.mode, a.amount, p.tendered, p.change_given, p.status
             FROM payment_allocations a JOIN payments p ON p.id = a.payment_id
             WHERE a.target_type = 'sales_invoice' AND a.target_id = ?1 ORDER BY p.created_at",
        )?
        .query_map([id], |r| {
            Ok(PaymentSummary {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                mode: r.get(3)?,
                amount: r.get(4)?,
                tendered: r.get(5)?,
                change_given: r.get(6)?,
                status: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    detail.tax_summary = tax_summary(&detail.lines);
    Ok(detail)
}

pub fn get_invoice(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<InvoiceDetail> {
    ctx.require("sales.view")?;
    invoice_detail(ctx.conn, &input.id)
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListQuery {
    pub search: Option<String>,
    /// "finalized" or "voided".
    pub status: Option<String>,
    /// "unpaid", "partial" or "paid".
    pub payment_status: Option<String>,
    pub party_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListItem {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub total: i64,
    pub amount_paid: i64,
    pub balance_due: i64,
    pub payment_status: String,
    pub status: String,
}

pub fn list_invoices(ctx: &Ctx<'_>, query: &InvoiceListQuery) -> AppResult<Page<InvoiceListItem>> {
    ctx.require("sales.view")?;
    let mut filters = vec!["status <> 'draft'".to_string()];
    let mut args: Vec<SqlValue> = Vec::new();
    if let Some(status) = query.status.as_deref() {
        if !matches!(status, "finalized" | "voided") {
            return Err(AppError::field("status", "Use finalized or voided."));
        }
        filters.push("status = ?".into());
        args.push(status.to_string().into());
    }
    if let Some(ps) = query.payment_status.as_deref() {
        if !matches!(ps, "unpaid" | "partial" | "paid") {
            return Err(AppError::field(
                "paymentStatus",
                "Use unpaid, partial or paid.",
            ));
        }
        filters.push("payment_status = ? AND status = 'finalized'".into());
        args.push(ps.to_string().into());
    }
    if let Some(party) = &query.party_id {
        filters.push("party_id = ?".into());
        args.push(party.clone().into());
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
        filters.push("(doc_no LIKE ? ESCAPE '\\' OR lower(customer_name) LIKE lower(?) ESCAPE '\\' OR customer_phone LIKE ? ESCAPE '\\')".into());
        let pattern = like_pattern(search);
        args.extend([
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.into(),
        ]);
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM sales_invoices WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("docNo", "doc_no"),
            ("date", "business_date"),
            ("customer", "lower(customer_name)"),
            ("total", "total"),
            ("paid", "amount_paid"),
            ("due", "balance_due"),
            ("status", "payment_status"),
        ],
        "business_date DESC, created_at DESC, id",
        "id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let mut stmt = ctx.conn.prepare(&format!(
        "SELECT id, doc_no, business_date, customer_name, customer_phone, total, amount_paid, balance_due, payment_status, status
         FROM sales_invoices WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
    ))?;
    let items = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(InvoiceListItem {
                id: r.get(0)?,
                doc_no: r.get(1)?,
                business_date: r.get(2)?,
                customer_name: r.get(3)?,
                customer_phone: r.get(4)?,
                total: r.get(5)?,
                amount_paid: r.get(6)?,
                balance_due: r.get(7)?,
                payment_status: r.get(8)?,
                status: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

// ------------------------------------------------------------------ preview

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInput {
    pub warehouse_id: Option<String>,
    pub party_id: Option<String>,
    /// The till's open session (its price list applies when the customer has none).
    #[serde(default)]
    pub register_session_id: Option<String>,
    pub place_of_supply: Option<String>,
    pub prices_include_tax: Option<bool>,
    pub lines: Vec<SaleLineInput>,
    pub bill_discount: Option<Discount>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLine {
    pub variant_id: String,
    pub item_name: String,
    pub unit_name: String,
    pub qty: i64,
    pub unit_price: i64,
    pub catalog_price: i64,
    pub gross: i64,
    pub line_discount: i64,
    pub bill_discount: i64,
    pub taxable: i64,
    pub tax: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub lines: Vec<PreviewLine>,
    pub gross: i64,
    pub discount: i64,
    pub taxable: i64,
    pub tax: i64,
    pub round_off: i64,
    pub total: i64,
    pub is_inter_state: bool,
}

/// Authoritative totals for the bill on screen (no writes).
pub fn preview(ctx: &Ctx<'_>, input: &PreviewInput) -> AppResult<Preview> {
    ctx.require("sales.create")?;
    let settings = settings::load(ctx.conn)?;
    let (_, _, branch_state) = branch_of(ctx.conn, input.warehouse_id.as_deref())?;
    let party_state = match &input.party_id {
        Some(p) => party_ledger::load_party(ctx.conn, p, "customer")?.state_code,
        None => None,
    };
    let pos = input.place_of_supply.clone().or(party_state);
    let state = branch_state.or_else(|| settings.business.state_code.clone());
    let today = settings.business_date(ctx.now);
    let list = ListContext {
        list_id: super::price_lists::list_for(
            ctx.conn,
            input.party_id.as_deref(),
            input.register_session_id.as_deref(),
        )?,
        date: &today,
    };
    let priced = price(
        ctx.conn,
        &settings,
        &input.lines,
        input.bill_discount.as_ref(),
        input.prices_include_tax,
        pos.as_deref(),
        state.as_deref(),
        &list,
    )?;
    Ok(Preview {
        lines: priced
            .lines
            .iter()
            .zip(&priced.result.lines)
            .map(|(l, r)| PreviewLine {
                variant_id: l.variant_id.clone(),
                item_name: l.name.clone(),
                unit_name: l.unit_name.clone(),
                qty: l.qty,
                unit_price: l.unit_price,
                catalog_price: l.catalog_price,
                gross: r.gross,
                line_discount: r.line_discount,
                bill_discount: r.bill_discount,
                taxable: r.taxable,
                tax: r.tax,
                total: r.total,
            })
            .collect(),
        gross: priced.result.gross,
        discount: priced.result.line_discount + priced.result.bill_discount,
        taxable: priced.result.taxable,
        tax: priced.result.tax,
        round_off: priced.result.round_off,
        total: priced.result.total,
        is_inter_state: priced.inter_state,
    })
}

// ------------------------------------------------------------------ create invoice

pub fn create_invoice(ctx: &mut Ctx<'_>, input: &SaleInput) -> AppResult<InvoiceDetail> {
    create_invoice_inner(ctx, input, false, None)
}

/// An invoice a client counter made while the hub was offline (services::sync applies it).
pub(crate) struct OfflineOrigin {
    /// Id the counter gave the invoice (so its receipt and later look-ups match).
    pub invoice_id: String,
    pub device_id: String,
    /// The counter's clock when the bill was made, and that time corrected by the measured skew.
    pub client_created_at: String,
    pub corrected_at: chrono::DateTime<chrono::Utc>,
    /// Number from a block leased to the device.
    pub series_id: String,
    pub period: String,
    pub seq: i64,
}

/// Checks that an offline invoice number lies in a block leased to that device and formats it
/// exactly as the counter did.
fn leased_number(
    conn: &Connection,
    settings: &Settings,
    origin: &OfflineOrigin,
    business_date: &str,
) -> AppResult<numbering::Assigned> {
    let leased: i64 = conn.query_row(
        "SELECT count(*) FROM number_series_leases
         WHERE device_id = ?1 AND series_id = ?2 AND period = ?3 AND ?4 BETWEEN from_seq AND to_seq",
        params![origin.device_id, origin.series_id, origin.period, origin.seq],
        |r| r.get(0),
    )?;
    if leased == 0 {
        return Err(AppError::validation(
            "This offline invoice number was not leased to this counter.",
        ));
    }
    let fy = settings.fy_label(business_date);
    numbering::format_assigned(conn, &origin.series_id, &origin.period, &fy, origin.seq)
}

pub(crate) fn create_invoice_inner(
    ctx: &mut Ctx<'_>,
    input: &SaleInput,
    prices_preapproved: bool,
    offline: Option<&OfflineOrigin>,
) -> AppResult<InvoiceDetail> {
    ctx.require("sales.create")?;
    idempotency::validate_key(&input.idempotency_key)?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let mut approved_by = None;
    let business_date = if let Some(origin) = offline {
        // The bill's date is the counter's, corrected for its clock error. It was a real sale at
        // that time, so no back-dating approval applies.
        if origin.corrected_at > ctx.now + chrono::Duration::minutes(5) {
            return Err(AppError::field(
                "clientCreatedAt",
                "An offline invoice can't be dated in the future.",
            ));
        }
        settings.business_date(origin.corrected_at)
    } else {
        let business_date = match &input.business_date {
            Some(d) => date("businessDate", d)?,
            None => today.clone(),
        };
        if business_date > today {
            return Err(AppError::field(
                "businessDate",
                "An invoice can't be dated in the future.",
            ));
        }
        if business_date != today {
            approved_by = ctx.approve("sales.backdate", "date a bill in the past")?;
        }
        business_date
    };
    // A locked day needs `period.override`. An offline sale already happened: it is kept and
    // flagged for the owner instead.
    let lock = if offline.is_some() {
        None
    } else {
        period::check(ctx, &settings, &business_date, "an invoice")?
    };
    let offline_in_locked_day =
        offline.is_some() && period::is_locked(ctx.conn, &settings, &business_date)?;
    let (branch_id, warehouse_id, branch_state) =
        branch_of(ctx.conn, input.warehouse_id.as_deref())?;
    let party: Option<PartyRef> = input
        .party_id
        .as_deref()
        .map(|p| party_ledger::load_party(ctx.conn, p, "customer"))
        .transpose()?;
    let customer_gstin = match &party {
        Some(p) => p.gstin.clone(),
        None => optional_gstin("customerGstin", input.customer_gstin.as_deref())?,
    };
    let place_of_supply = optional("placeOfSupply", input.place_of_supply.as_deref(), 2)?
        .or_else(|| party.as_ref().and_then(|p| p.state_code.clone()))
        .or_else(|| customer_gstin.as_ref().map(|g| g[..2].to_string()));
    let own_state = branch_state.or_else(|| settings.business.state_code.clone());
    let list = ListContext {
        list_id: super::price_lists::list_for(
            ctx.conn,
            input.party_id.as_deref(),
            input.register_session_id.as_deref(),
        )?,
        date: &business_date,
    };
    let priced = price(
        ctx.conn,
        &settings,
        &input.lines,
        input.bill_discount.as_ref(),
        input.prices_include_tax,
        place_of_supply.as_deref(),
        own_state.as_deref(),
        &list,
    )?;
    if let Some(expected) = input.expected_total {
        if expected != priced.result.total {
            return Err(AppError::new(
                ErrorCode::TotalsMismatch,
                "The bill total changed (prices or tax were updated). Please review the bill.",
            )
            .with_details(json!({ "expected": expected, "actual": priced.result.total })));
        }
    }
    // Offline prices come from the counter's cached catalogue (no overrides are possible
    // offline); a price that changed on the hub meanwhile is reported, not refused.
    approved_by = check_pricing_approvals(
        ctx.actor,
        ctx.approver,
        &priced,
        prices_preapproved || offline.is_some(),
    )?
    .or(approved_by);

    let payments = validate_payments(ctx.conn, &input.payments)?;
    let paid: i64 = payments.iter().map(|p| p.amount).sum();
    let total = priced.result.total;
    if paid > total {
        return Err(AppError::field(
            "payments",
            "Payments are more than the bill. Enter cash handed over as 'tendered' to give change.",
        ));
    }
    let due = total - paid;
    if due > 0 && offline.is_some() {
        return Err(AppError::field(
            "payments",
            "Offline sales must be paid in full.",
        ));
    }
    if due > 0 {
        let Some(p) = &party else {
            return Err(AppError::field(
                "partyId",
                "Choose a customer to sell on credit.",
            ));
        };
        if p.credit_limit.is_some_and(|limit| p.balance + due > limit) {
            approved_by = ctx
                .approve(
                    "sales.credit_override",
                    "bill beyond the customer's credit limit",
                )?
                .or(approved_by);
        }
    }
    // An offline sale happened while its till was open; the till may have closed since.
    if let Some(session) = input
        .register_session_id
        .as_ref()
        .filter(|_| offline.is_none())
    {
        let open: i64 = ctx.conn.query_row(
            "SELECT count(*) FROM register_sessions WHERE id = ?1 AND status = 'open'",
            [session],
            |r| r.get(0),
        )?;
        if open == 0 {
            return Err(AppError::field(
                "registerSessionId",
                "The till session is closed.",
            ));
        }
    }
    let customer_name = match &party {
        Some(p) => p.name.clone(),
        None => input
            .customer_name
            .as_deref()
            .map(|n| required("customerName", n, 160))
            .transpose()?
            .unwrap_or_else(|| "Walk-in Customer".into()),
    };
    let customer_phone = match &party {
        Some(p) => p.phone.clone(),
        None => optional_phone("customerPhone", input.customer_phone.as_deref())?,
    };
    let notes = optional("notes", input.notes.as_deref(), 1000)?;

    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "sales_create_invoice",
            actor,
            input,
            &now,
            || {
                let number = match offline {
                    Some(origin) => leased_number(tx, &settings, origin, &business_date)?,
                    None => numbering::next(
                        tx,
                        &settings,
                        &branch_id,
                        "sales_invoice",
                        &business_date,
                        &now,
                    )?,
                };
                let id = offline.map_or_else(new_id, |o| o.invoice_id.clone());
                let r = &priced.result;
                tx.execute(
                "INSERT INTO sales_invoices (id, branch_id, warehouse_id, register_session_id, series_id, number_period, doc_seq, doc_no,
                    business_date, status, party_id, customer_name, customer_phone, customer_gstin, billing_address, shipping_address,
                    place_of_supply, is_inter_state, prices_include_tax, currency, gross, line_discount, bill_discount, taxable, tax_total,
                    round_off, total, source_document_id, notes, idempotency_key, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'draft', ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
                    ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?30, ?31, ?31)",
                params![
                    id, branch_id, warehouse_id, input.register_session_id, number.series_id, number.period, number.seq, number.doc_no,
                    business_date, input.party_id, customer_name, customer_phone, customer_gstin,
                    party.as_ref().and_then(|p| p.billing_address.clone()), party.as_ref().and_then(|p| p.shipping_address.clone()),
                    place_of_supply, priced.inter_state, priced.prices_include_tax, settings.business.currency, r.gross, r.line_discount,
                    r.bill_discount, r.taxable, r.tax, r.round_off, r.total, input.source_document_id, notes, input.idempotency_key, now,
                    actor.user_id
                ],
            )?;
                if list.list_id.is_some() {
                    tx.execute(
                        "UPDATE sales_invoices SET price_list_id = ?1 WHERE id = ?2",
                        params![list.list_id, id],
                    )?;
                }
                if let Some(origin) = offline {
                    tx.execute(
                        "UPDATE sales_invoices SET origin = 'offline', device_id = ?1, client_created_at = ?2 WHERE id = ?3",
                        params![origin.device_id, origin.client_created_at, id],
                    )?;
                } else if actor.device_id.is_some() {
                    tx.execute(
                        "UPDATE sales_invoices SET device_id = ?1 WHERE id = ?2",
                        params![actor.device_id, id],
                    )?;
                }
                let mut warnings = Vec::new();
                let mut cogs = 0;
                for (index, (line, lr)) in priced.lines.iter().zip(&r.lines).enumerate() {
                    let cols = tax_columns(&lr.taxes);
                    let line_id = new_id();
                    // Stock first: the line records what the ledger actually charged (FIFO layers,
                    // or the average cost) and, for batch products, the batch(es) it came from.
                    let mut cost_value = 0;
                    let mut batch_id = line.batch_id.clone();
                    if !line.is_service {
                        // A sale made offline already happened: it is posted even if stock goes
                        // below zero or a batch had expired, and that is reported for a manager.
                        let (policy, expired_policy) = if offline.is_some() {
                            (
                                settings::NegativeStockPolicy::Warn,
                                settings::ExpiredStockPolicy::Warn,
                            )
                        } else {
                            (
                                settings.inventory.negative_stock_policy,
                                settings.inventory.expired_stock_policy,
                            )
                        };
                        let posted = stock_posting::post_out(
                            tx,
                            policy,
                            expired_policy,
                            &business_date,
                            &now,
                            Some(&actor.user_id),
                            &Move {
                                warehouse_id: &warehouse_id,
                                variant_id: &line.variant_id,
                                batch_id: line.batch_id.as_deref(),
                                qty: -line.base_qty,
                                unit_cost: line.cost,
                                kind: "sale",
                                source_type: "sales_invoice",
                                source_id: &id,
                                source_line_id: Some(&line_id),
                                ..Move::default()
                            },
                        )?;
                        if posted.len() == 1 {
                            batch_id = posted[0].batch_id.clone();
                        }
                        for p in posted {
                            cost_value -= p.value;
                            if let (Some(origin), Some(w)) = (offline, &p.warning) {
                                if w.expired_batch.is_none() {
                                    super::sync::record_exception(
                                        tx,
                                        &super::sync::Exception {
                                            kind: "offline_oversell",
                                            device_id: &origin.device_id,
                                            source: ("sales_invoice", &id),
                                            variant_id: Some(&line.variant_id),
                                            warehouse_id: Some(&warehouse_id),
                                            amount: w.requested - w.available.max(0),
                                            details: json!({ "docNo": number.doc_no, "available": w.available, "requested": w.requested }),
                                        },
                                        &now,
                                    )?;
                                }
                            }
                            warnings.extend(p.warning);
                        }
                        cogs += cost_value;
                    }
                    let unit_cost = if line.is_service {
                        line.cost
                    } else {
                        stock_posting::per_unit(cost_value, line.base_qty)
                    };
                    tx.execute(
                    "INSERT INTO sales_invoice_items (id, invoice_id, line_no, variant_id, batch_id, unit_id, unit_factor_num, unit_factor_den,
                        item_name, sku, hsn_sac, unit_name, qty, base_qty, unit_price, mrp, line_discount, bill_discount_share, taxable,
                        tax_rate_id, tax_rate, cgst, sgst, igst, other_tax, cess, tax_total, line_total, unit_cost, created_at, updated_at,
                        cost_value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                        ?25, ?26, ?27, ?28, ?29, ?30, ?30, ?31)",
                    params![
                        line_id, id, index + 1, line.variant_id, batch_id, line.unit_id, line.factor_num, line.factor_den, line.name,
                        line.sku, line.hsn_sac, line.unit_name, line.qty, line.base_qty, line.unit_price, line.mrp, lr.line_discount,
                        lr.bill_discount, lr.taxable, line.tax_rate_id, line.tax_rate + line.cess_rate, cols.cgst, cols.sgst, cols.igst,
                        cols.other, cols.cess, lr.tax, lr.total, unit_cost, now, (!line.is_service).then_some(cost_value)
                    ],
                )?;
                    if let Some(origin) = offline.filter(|_| line.unit_price != line.catalog_price)
                    {
                        super::sync::record_exception(
                            tx,
                            &super::sync::Exception {
                                kind: "offline_price_changed",
                                device_id: &origin.device_id,
                                source: ("sales_invoice", &id),
                                variant_id: Some(&line.variant_id),
                                warehouse_id: Some(&warehouse_id),
                                amount: line.unit_price - line.catalog_price,
                                details: json!({ "docNo": number.doc_no, "soldAt": line.unit_price, "catalogue": line.catalog_price }),
                            },
                            &now,
                        )?;
                    }
                }
                tx.execute(
                "UPDATE sales_invoices SET status = 'finalized', finalized_at = ?1, finalized_by = ?2 WHERE id = ?3",
                params![now, actor.user_id, id],
            )?;
                if let Some(p) = &party {
                    party_ledger::post(
                        tx,
                        Posting {
                            party_id: &p.id,
                            business_date: &business_date,
                            entry_type: "sales_invoice",
                            source_type: "sales_invoice",
                            source_id: &id,
                            amount: r.total,
                            narration: format!("Invoice {}", number.doc_no),
                            user_id: Some(&actor.user_id),
                            now: &now,
                        },
                    )?;
                }
                let party_id = party.as_ref().map(|p| p.id.as_str());
                journal::post(
                    tx,
                    Entry {
                        business_date: &business_date,
                        source_type: "sales_invoice",
                        source_id: &id,
                        narration: format!("Sale {}", number.doc_no),
                        lines: vec![
                            JLine::debit(accounts::RECEIVABLE, r.total).party(party_id),
                            JLine::credit(accounts::SALES, r.taxable),
                            JLine::credit(accounts::OUTPUT_TAX, r.tax),
                            JLine::credit(accounts::ROUND_OFF, r.round_off),
                            JLine::debit(accounts::COGS, cogs),
                            JLine::credit(accounts::INVENTORY, cogs),
                        ],
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
                        "in",
                        party_id,
                        payment,
                        Some(("sales_invoice", &id)),
                        accounts::RECEIVABLE,
                        input.register_session_id.as_deref(),
                        actor,
                        &now,
                    )?;
                }
                if let Some(source) = &input.source_document_id {
                    let changed = tx.execute(
                    "UPDATE sales_documents SET status = 'converted', converted_invoice_id = ?1, updated_at = ?2, updated_by = ?3
                     WHERE id = ?4 AND status IN ('open', 'accepted') AND deleted_at IS NULL",
                    params![id, now, actor.user_id, source],
                )?;
                    if changed == 0 {
                        return Err(AppError::conflict(
                            "That quotation or order was already converted or is closed.",
                        ));
                    }
                }
                period::audit(&lock, tx, actor, "sales_invoices", &id, &now)?;
                if offline_in_locked_day {
                    AuditEntry::new("OFFLINE_SALE_IN_LOCKED_DAY", "sales_invoices", Some(&id))
                        .by(actor)
                        .summary(format!("{} dated {business_date}", number.doc_no))
                        .write(tx, &now)?;
                    super::notifications::raise(
                        tx,
                        &super::notifications::Notice {
                            kind: "locked_day",
                            severity: "warning",
                            title: &format!("Offline sale {} is in a locked day", number.doc_no),
                            body: &format!(
                                "A counter synced a sale dated {business_date}, which is closed. It was kept; check the day's figures."
                            ),
                            key: &format!("locked_day:{id}"),
                            entity: Some(("sales_invoice", &id)),
                        },
                        &now,
                    )?;
                }
                let discount = r.line_discount + r.bill_discount;
                AuditEntry::new("SALE_FINALIZED", "sales_invoices", Some(&id))
                    .by(actor)
                    .approved_by(approved_by.as_deref())
                    .summary(format!(
                        "{} total {} discount {}",
                        number.doc_no, r.total, discount
                    ))
                    .write(tx, &now)?;
                let mut detail = invoice_detail(tx, &id)?;
                detail.warnings = warnings;
                Ok(detail)
            },
        )
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidInput {
    pub id: String,
    pub reason: String,
}

/// Voids a finalized invoice: stock back, ledger and journal reversed, its payments voided
/// (cash handed back). Needs `sales.void` (or a manager's approval).
pub fn void_invoice(ctx: &mut Ctx<'_>, input: &VoidInput) -> AppResult<InvoiceDetail> {
    ctx.require("sales.view")?;
    let reason = required("reason", &input.reason, 300)?;
    let approved_by = ctx.approve("sales.void", "void an invoice")?;
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let dated: String = ctx
        .conn
        .query_row(
            "SELECT business_date FROM sales_invoices WHERE id = ?1",
            [&input.id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The invoice"))?;
    let lock = period::check_void(ctx, &settings, &dated, &today, "voiding an invoice")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let invoice = invoice_detail(tx, &input.id)?;
        if invoice.status != "finalized" {
            return Err(AppError::conflict(
                "Only a finalized invoice can be voided.",
            ));
        }
        if invoice.lines.iter().any(|l| l.returned_qty > 0) {
            return Err(AppError::conflict(
                "Items from this invoice were returned. Issue a credit note instead of voiding.",
            ));
        }
        for payment in invoice.payments.iter().filter(|p| p.status == "finalized") {
            let elsewhere: i64 = tx.query_row(
                "SELECT count(*) FROM payment_allocations WHERE payment_id = ?1 AND NOT (target_type = 'sales_invoice' AND target_id = ?2)",
                params![payment.id, invoice.id],
                |r| r.get(0),
            )?;
            if elsewhere > 0 {
                return Err(AppError::conflict(format!(
                    "Payment {} also pays other bills. Void it separately first.",
                    payment.doc_no
                )));
            }
            void_payment(
                tx,
                &payment.id,
                &today,
                &format!("Invoice {} voided", invoice.doc_no),
                actor,
                &now,
            )?;
        }
        stock_posting::reverse_source(
            tx,
            "sales_invoice",
            &invoice.id,
            &today,
            &now,
            Some(&actor.user_id),
        )?;
        party_ledger::reverse_source(
            tx,
            "sales_invoice",
            &invoice.id,
            &today,
            &format!("Invoice {} voided", invoice.doc_no),
            Some(&actor.user_id),
            &now,
        )?;
        journal::reverse_source(
            tx,
            "sales_invoice",
            &invoice.id,
            &today,
            &format!("Void {}", invoice.doc_no),
            Some(&actor.user_id),
            &now,
        )?;
        tx.execute(
            "UPDATE sales_invoices SET status = 'voided', voided_at = ?1, voided_by = ?2, void_reason = ?3, updated_at = ?1, updated_by = ?2 WHERE id = ?4",
            params![now, actor.user_id, reason, invoice.id],
        )?;
        period::audit(&lock, tx, actor, "sales_invoices", &invoice.id, &now)?;
        AuditEntry::new("SALE_VOIDED", "sales_invoices", Some(&invoice.id))
            .by(actor)
            .approved_by(approved_by.as_deref())
            .summary(format!("{}: {reason}", invoice.doc_no))
            .write(tx, &now)?;
        invoice_detail(tx, &invoice.id)
    })
}

// ------------------------------------------------------------------ returns / credit notes

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnLineInput {
    /// Line of the original invoice (required when returning against an invoice).
    pub original_item_id: Option<String>,
    /// Product returned without an invoice.
    pub variant_id: Option<String>,
    /// Quantity in the invoice line's unit (or base unit without an invoice), thousandths.
    pub qty: i64,
    #[serde(default = "yes")]
    pub restock: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnInput {
    pub idempotency_key: String,
    pub original_invoice_id: Option<String>,
    pub party_id: Option<String>,
    pub warehouse_id: Option<String>,
    pub reason: String,
    pub lines: Vec<ReturnLineInput>,
    /// Money paid back now. Walk-in returns are always refunded in full (the amount is set to the
    /// return's total); a customer's return without a refund becomes credit on their account.
    pub refund: Option<PaymentInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnDetail {
    pub id: String,
    pub doc_no: String,
    pub business_date: String,
    pub status: String,
    pub original_invoice_id: Option<String>,
    pub party_id: Option<String>,
    pub customer_name: String,
    pub reason: Option<String>,
    pub taxable: i64,
    pub tax_total: i64,
    pub total: i64,
    pub amount_refunded: i64,
    pub lines: Vec<ReturnLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnLine {
    pub variant_id: String,
    pub item_name: String,
    pub qty: i64,
    pub unit_price: i64,
    pub taxable: i64,
    pub tax_total: i64,
    pub line_total: i64,
    pub restock: bool,
}

struct ReturnCalc {
    original_item_id: Option<String>,
    variant_id: String,
    batch_id: Option<String>,
    unit_id: String,
    factor_num: i64,
    factor_den: i64,
    item_name: String,
    sku: Option<String>,
    hsn_sac: Option<String>,
    qty: i64,
    base_qty: i64,
    unit_price: i64,
    discount: i64,
    taxable: i64,
    tax_rate: i64,
    cols: TaxColumns,
    tax_total: i64,
    unit_cost: i64,
    restock: bool,
    is_service: bool,
}

fn proportional(value: i64, part: i64, whole: i64) -> i64 {
    i64::try_from(money::div_round(
        i128::from(value) * i128::from(part),
        i128::from(whole),
        RoundingMode::HalfUp,
    ))
    .unwrap_or(0)
}

pub(crate) fn return_detail(conn: &Connection, id: &str) -> AppResult<ReturnDetail> {
    let mut detail = conn
        .query_row(
            "SELECT id, doc_no, business_date, status, original_invoice_id, party_id, customer_name, reason, taxable, tax_total, total, amount_refunded
             FROM sales_returns WHERE id = ?1 AND status <> 'draft'",
            [id],
            |r| {
                Ok(ReturnDetail {
                    id: r.get(0)?,
                    doc_no: r.get(1)?,
                    business_date: r.get(2)?,
                    status: r.get(3)?,
                    original_invoice_id: r.get(4)?,
                    party_id: r.get(5)?,
                    customer_name: r.get(6)?,
                    reason: r.get(7)?,
                    taxable: r.get(8)?,
                    tax_total: r.get(9)?,
                    total: r.get(10)?,
                    amount_refunded: r.get(11)?,
                    lines: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The return"))?;
    detail.lines = conn
        .prepare("SELECT variant_id, item_name, qty, unit_price, taxable, tax_total, line_total, restock FROM sales_return_items WHERE return_id = ?1 ORDER BY line_no")?
        .query_map([id], |r| {
            Ok(ReturnLine {
                variant_id: r.get(0)?,
                item_name: r.get(1)?,
                qty: r.get(2)?,
                unit_price: r.get(3)?,
                taxable: r.get(4)?,
                tax_total: r.get(5)?,
                line_total: r.get(6)?,
                restock: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(detail)
}

pub fn get_return(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<ReturnDetail> {
    ctx.require("sales.view")?;
    return_detail(ctx.conn, &input.id)
}

/// Customer return / credit note. Against an invoice, amounts are the invoice's own (pro rata),
/// and no more than was sold can come back.
pub fn create_return(ctx: &mut Ctx<'_>, input: &ReturnInput) -> AppResult<ReturnDetail> {
    let approved_by = ctx.approve("sales.return", "take back goods")?;
    let reason = required("reason", &input.reason, 300)?;
    if input.lines.is_empty() {
        return Err(AppError::field("lines", "Add at least one item."));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let lock = period::check(ctx, &settings, &today, "a sales return")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let refund = input
        .refund
        .as_ref()
        .map(|r| validate_payments(ctx.conn, std::slice::from_ref(r)))
        .transpose()?
        .map(|mut v| v.remove(0));
    write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "sales_create_return",
            actor,
            input,
            &now,
            || {
                let (invoice, party_id) = match &input.original_invoice_id {
                    Some(invoice_id) => {
                        let invoice = invoice_detail(tx, invoice_id)?;
                        if invoice.status != "finalized" {
                            return Err(AppError::conflict(
                                "Returns can only be taken against a finalized invoice.",
                            ));
                        }
                        let party = invoice.party_id.clone();
                        (Some(invoice), party)
                    }
                    None => (None, input.party_id.clone()),
                };
                let party = party_id
                    .as_deref()
                    .map(|p| party_ledger::load_party(tx, p, "customer"))
                    .transpose()?;
                let warehouse_hint = match &invoice {
                    Some(inv) => Some(tx.query_row(
                        "SELECT warehouse_id FROM sales_invoices WHERE id = ?1",
                        [&inv.id],
                        |r| r.get::<_, String>(0),
                    )?),
                    None => input.warehouse_id.clone(),
                };
                let (branch_id, warehouse_id, branch_state) =
                    branch_of(tx, warehouse_hint.as_deref())?;

                let mut calcs = Vec::new();
                for (index, line) in input.lines.iter().enumerate() {
                    if line.qty <= 0 {
                        return Err(AppError::field(
                            &format!("lines.{index}.qty"),
                            "Quantity must be greater than zero.",
                        ));
                    }
                    match (&invoice, &line.original_item_id) {
                        (Some(inv), Some(item_id)) => {
                            let orig =
                                inv.lines.iter().find(|l| l.id == *item_id).ok_or_else(|| {
                                    AppError::field(
                                        &format!("lines.{index}.originalItemId"),
                                        "That line isn't on this invoice.",
                                    )
                                })?;
                            let remaining = orig.qty - orig.returned_qty;
                            if line.qty > remaining {
                                return Err(AppError::field(
                                    &format!("lines.{index}.qty"),
                                    format!(
                                        "Only {} can still be returned.",
                                        remaining as f64 / 1000.0
                                    ),
                                ));
                            }
                            #[allow(clippy::type_complexity)]
                        let (variant_id, batch_id, unit_id, num, den, base_qty, unit_cost, is_service): (String, Option<String>, String, i64, i64, i64, i64, bool) = tx.query_row(
                            "SELECT i.variant_id, i.batch_id, i.unit_id, i.unit_factor_num, i.unit_factor_den, i.base_qty, i.unit_cost, p.is_service
                             FROM sales_invoice_items i JOIN product_variants v ON v.id = i.variant_id JOIN products p ON p.id = v.product_id WHERE i.id = ?1",
                            [item_id],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
                        )?;
                            // Returning everything that's left: take exactly what remains, so partial
                            // returns never add up to more (or less) than the invoice.
                            let (prev_taxable, prev_cgst, prev_sgst, prev_igst, prev_other, prev_cess, prev_base): (i64, i64, i64, i64, i64, i64, i64) = tx.query_row(
                            "SELECT coalesce(sum(ri.taxable), 0), coalesce(sum(ri.cgst), 0), coalesce(sum(ri.sgst), 0), coalesce(sum(ri.igst), 0),
                                    coalesce(sum(ri.other_tax), 0), coalesce(sum(ri.cess), 0), coalesce(sum(ri.base_qty), 0)
                             FROM sales_return_items ri JOIN sales_returns sr ON sr.id = ri.return_id
                             WHERE ri.original_item_id = ?1 AND sr.status = 'finalized'",
                            [item_id],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
                        )?;
                            let last = line.qty == remaining;
                            let part = |value: i64, prev: i64| {
                                if last {
                                    value - prev
                                } else {
                                    proportional(value, line.qty, orig.qty)
                                }
                            };
                            let cols = TaxColumns {
                                cgst: part(orig.cgst, prev_cgst),
                                sgst: part(orig.sgst, prev_sgst),
                                igst: part(orig.igst, prev_igst),
                                other: part(orig.other_tax, prev_other),
                                cess: part(orig.cess, prev_cess),
                            };
                            calcs.push(ReturnCalc {
                                original_item_id: Some(item_id.clone()),
                                variant_id,
                                batch_id,
                                unit_id,
                                factor_num: num,
                                factor_den: den,
                                item_name: orig.item_name.clone(),
                                sku: orig.sku.clone(),
                                hsn_sac: orig.hsn_sac.clone(),
                                qty: line.qty,
                                base_qty: if last {
                                    base_qty - prev_base
                                } else {
                                    proportional(base_qty, line.qty, orig.qty)
                                },
                                unit_price: orig.unit_price,
                                discount: proportional(
                                    orig.line_discount + orig.bill_discount_share,
                                    line.qty,
                                    orig.qty,
                                ),
                                taxable: part(orig.taxable, prev_taxable),
                                tax_rate: orig.tax_rate,
                                tax_total: cols.cgst
                                    + cols.sgst
                                    + cols.igst
                                    + cols.other
                                    + cols.cess,
                                cols,
                                unit_cost,
                                restock: line.restock,
                                is_service,
                            });
                        }
                        (Some(_), None) => {
                            return Err(AppError::field(
                                &format!("lines.{index}.originalItemId"),
                                "Choose the invoice line being returned.",
                            ))
                        }
                        (None, _) => {
                            let variant_id = line.variant_id.as_deref().ok_or_else(|| {
                                AppError::field(
                                    &format!("lines.{index}.variantId"),
                                    "Choose the product being returned.",
                                )
                            })?;
                            let r = resolve_line(tx, index, variant_id, None, line.qty, None)?;
                            let pos = party.as_ref().and_then(|p| p.state_code.clone());
                            let own = branch_state
                                .clone()
                                .or_else(|| settings.business.state_code.clone());
                            let inter = matches!((&pos, &own), (Some(a), Some(b)) if a != b);
                            let priced = pricing::price_document(&DocumentInput {
                                lines: vec![LineInput {
                                    unit_price: r.unit_price,
                                    qty: r.qty,
                                    discount: None,
                                    taxes: tax_components(
                                        settings.tax.regime,
                                        r.tax_type.as_deref(),
                                        r.tax_rate,
                                        r.cess_rate,
                                        inter,
                                    )?,
                                }],
                                prices_include_tax: settings.tax.prices_include_tax,
                                bill_discount: None,
                                round_total_to: None,
                                rounding: Some(settings.tax.rounding.into()),
                            })
                            .map_err(|e| AppError::validation(e.0))?;
                            let lr = &priced.lines[0];
                            let cols = tax_columns(&lr.taxes);
                            calcs.push(ReturnCalc {
                                original_item_id: None,
                                variant_id: r.variant_id.clone(),
                                batch_id: None,
                                unit_id: r.unit_id.clone(),
                                factor_num: 1,
                                factor_den: 1,
                                item_name: r.name.clone(),
                                sku: Some(r.sku.clone()),
                                hsn_sac: r.hsn_sac.clone(),
                                qty: r.qty,
                                base_qty: r.base_qty,
                                unit_price: r.unit_price,
                                discount: 0,
                                taxable: lr.taxable,
                                tax_rate: r.tax_rate + r.cess_rate,
                                tax_total: lr.tax,
                                cols,
                                unit_cost: r.cost,
                                restock: line.restock,
                                is_service: r.is_service,
                            });
                        }
                    }
                }
                let taxable: i64 = calcs.iter().map(|c| c.taxable).sum();
                let tax: i64 = calcs.iter().map(|c| c.tax_total).sum();
                // When this return takes back everything left on the invoice, it also takes back the
                // invoice's remaining round-off, so credit notes add up to the invoice exactly.
                let round_off = match &invoice {
                    Some(inv) => {
                        let mut returning: std::collections::HashMap<&str, i64> =
                            std::collections::HashMap::new();
                        for c in &calcs {
                            if let Some(item) = &c.original_item_id {
                                *returning.entry(item.as_str()).or_default() += c.qty;
                            }
                        }
                        let complete = inv.lines.iter().all(|l| {
                            l.returned_qty + returning.get(l.id.as_str()).copied().unwrap_or(0)
                                >= l.qty
                        });
                        if complete {
                            let previous: i64 = tx.query_row(
                            "SELECT coalesce(sum(round_off), 0) FROM sales_returns WHERE original_invoice_id = ?1 AND status = 'finalized'",
                            [&inv.id],
                            |r| r.get(0),
                        )?;
                            inv.round_off - previous
                        } else {
                            0
                        }
                    }
                    None => 0,
                };
                let total = taxable + tax + round_off;
                // A walk-in customer always gets the full value back; the screen can't know the
                // exact amount (tax and round-off) before this point, so it only chooses the mode.
                let refund: Option<ValidPayment> = match (&party, refund.clone()) {
                    (None, Some(mut r)) => {
                        r.amount = total;
                        r.tendered = None;
                        Some(r)
                    }
                    (_, r) => r,
                };
                let refund_amount = refund.as_ref().map_or(0, |r| r.amount);
                if refund_amount > total {
                    return Err(AppError::field(
                        "refund.amount",
                        "The refund is more than the return's value.",
                    ));
                }
                match &party {
                    None if refund_amount != total => {
                        return Err(AppError::field(
                            "refund.amount",
                            "Without a customer account the full value must be refunded.",
                        ));
                    }
                    // Never pay out cash to a customer who still owes more than the return is worth.
                    Some(p) if refund_amount > (total - p.balance.max(0)).max(0) => {
                        return Err(AppError::field(
                        "refund.amount",
                        "The customer still owes money; the return reduces their balance instead of a refund.",
                    ));
                    }
                    _ => {}
                }

                let number =
                    numbering::next(tx, &settings, &branch_id, "sales_return", &today, &now)?;
                let id = new_id();
                let customer_name = match (&invoice, &party) {
                    (Some(inv), _) => inv.customer_name.clone(),
                    (None, Some(p)) => p.name.clone(),
                    (None, None) => "Walk-in Customer".into(),
                };
                tx.execute(
                "INSERT INTO sales_returns (id, branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, status,
                    original_invoice_id, party_id, customer_name, customer_gstin, reason, gross, discount, taxable, tax_total, round_off,
                    total, idempotency_key, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'draft', ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?21, ?22, ?22)",
                params![
                    id, branch_id, warehouse_id, number.series_id, number.period, number.seq, number.doc_no, today,
                    input.original_invoice_id, party_id, customer_name, invoice.as_ref().and_then(|i| i.customer_gstin.clone()), reason,
                    taxable + calcs.iter().map(|c| c.discount).sum::<i64>(), calcs.iter().map(|c| c.discount).sum::<i64>(), taxable, tax,
                    round_off, total, input.idempotency_key, now, actor.user_id
                ],
            )?;
                let mut restock_value = 0;
                for (index, c) in calcs.iter().enumerate() {
                    let item_id = new_id();
                    tx.execute(
                    "INSERT INTO sales_return_items (id, return_id, line_no, original_item_id, variant_id, batch_id, unit_id, unit_factor_num,
                        unit_factor_den, item_name, sku, hsn_sac, qty, base_qty, unit_price, discount, taxable, tax_rate, cgst, sgst, igst,
                        other_tax, cess, tax_total, line_total, unit_cost, restock, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                        ?25, ?26, ?27, ?28, ?28)",
                    params![
                        item_id, id, index + 1, c.original_item_id, c.variant_id, c.batch_id, c.unit_id, c.factor_num, c.factor_den,
                        c.item_name, c.sku, c.hsn_sac, c.qty, c.base_qty, c.unit_price, c.discount, c.taxable, c.tax_rate, c.cols.cgst,
                        c.cols.sgst, c.cols.igst, c.cols.other, c.cols.cess, c.tax_total, c.taxable + c.tax_total, c.unit_cost, c.restock, now
                    ],
                )?;
                    if c.restock && !c.is_service {
                        for (batch_id, qty, unit_cost) in return_batches(
                            tx,
                            c.original_item_id.as_deref(),
                            c.batch_id.as_deref(),
                            c.base_qty,
                            c.unit_cost,
                        )? {
                            let posted = stock_posting::post(
                                tx,
                                settings.inventory.negative_stock_policy,
                                &today,
                                &now,
                                Some(&actor.user_id),
                                &Move {
                                    warehouse_id: &warehouse_id,
                                    variant_id: &c.variant_id,
                                    batch_id: batch_id.as_deref(),
                                    qty,
                                    unit_cost,
                                    kind: "sale_return",
                                    source_type: "sales_return",
                                    source_id: &id,
                                    source_line_id: Some(&item_id),
                                    ..Move::default()
                                },
                            )?;
                            restock_value += posted.value;
                        }
                    }
                }
                tx.execute("UPDATE sales_returns SET status = 'finalized', finalized_at = ?1, finalized_by = ?2 WHERE id = ?3", params![now, actor.user_id, id])?;
                let party_ref = party.as_ref().map(|p| p.id.as_str());
                if let Some(p) = party_ref {
                    party_ledger::post(
                        tx,
                        Posting {
                            party_id: p,
                            business_date: &today,
                            entry_type: "sales_return",
                            source_type: "sales_return",
                            source_id: &id,
                            amount: -total,
                            narration: format!("Credit note {}", number.doc_no),
                            user_id: Some(&actor.user_id),
                            now: &now,
                        },
                    )?;
                }
                journal::post(
                    tx,
                    Entry {
                        business_date: &today,
                        source_type: "sales_return",
                        source_id: &id,
                        narration: format!("Return {}", number.doc_no),
                        lines: vec![
                            JLine::debit(accounts::SALES_RETURNS, taxable),
                            JLine::debit(accounts::OUTPUT_TAX, tax),
                            JLine::debit(accounts::ROUND_OFF, round_off),
                            JLine::credit(accounts::RECEIVABLE, total).party(party_ref),
                            JLine::debit(accounts::INVENTORY, restock_value),
                            JLine::credit(accounts::COGS, restock_value),
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
                        "out",
                        party_ref,
                        refund,
                        Some(("sales_return", &id)),
                        accounts::RECEIVABLE,
                        None,
                        actor,
                        &now,
                    )?;
                }
                period::audit(&lock, tx, actor, "sales_returns", &id, &now)?;
                AuditEntry::new("SALE_RETURN", "sales_returns", Some(&id))
                    .by(actor)
                    .approved_by(approved_by.as_deref())
                    .summary(format!("{} total {total}: {reason}", number.doc_no))
                    .write(tx, &now)?;
                return_detail(tx, &id)
            },
        )
    })
}

// ------------------------------------------------------------------ quotations and orders

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocInput {
    pub id: Option<String>,
    /// "quotation", "sales_order" or "delivery_challan".
    pub doc_type: String,
    pub party_id: Option<String>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub valid_until: Option<String>,
    pub prices_include_tax: Option<bool>,
    pub lines: Vec<SaleLineInput>,
    pub bill_discount: Option<Discount>,
    pub notes: Option<String>,
    pub terms: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocLine {
    pub variant_id: String,
    pub unit_id: String,
    pub item_name: String,
    pub qty: i64,
    pub unit_price: i64,
    pub discount: i64,
    pub taxable: i64,
    pub tax_total: i64,
    pub line_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocDetail {
    pub id: String,
    pub doc_type: String,
    pub doc_no: String,
    pub business_date: String,
    pub valid_until: Option<String>,
    pub status: String,
    pub party_id: Option<String>,
    pub customer_name: String,
    pub prices_include_tax: bool,
    pub discount: i64,
    pub taxable: i64,
    pub tax_total: i64,
    pub round_off: i64,
    pub total: i64,
    pub converted_invoice_id: Option<String>,
    pub notes: Option<String>,
    pub terms: Option<String>,
    pub updated_at: String,
    pub lines: Vec<SalesDocLine>,
}

fn sales_doc_detail(conn: &Connection, id: &str) -> AppResult<SalesDocDetail> {
    let mut doc = conn
        .query_row(
            "SELECT id, doc_type, doc_no, business_date, valid_until, status, party_id, customer_name, prices_include_tax, discount, taxable,
                    tax_total, round_off, total, converted_invoice_id, notes, terms, updated_at
             FROM sales_documents WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| {
                Ok(SalesDocDetail {
                    id: r.get(0)?,
                    doc_type: r.get(1)?,
                    doc_no: r.get(2)?,
                    business_date: r.get(3)?,
                    valid_until: r.get(4)?,
                    status: r.get(5)?,
                    party_id: r.get(6)?,
                    customer_name: r.get(7)?,
                    prices_include_tax: r.get(8)?,
                    discount: r.get(9)?,
                    taxable: r.get(10)?,
                    tax_total: r.get(11)?,
                    round_off: r.get(12)?,
                    total: r.get(13)?,
                    converted_invoice_id: r.get(14)?,
                    notes: r.get(15)?,
                    terms: r.get(16)?,
                    updated_at: r.get(17)?,
                    lines: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The document"))?;
    doc.lines = conn
        .prepare("SELECT variant_id, unit_id, item_name, qty, unit_price, discount, taxable, tax_total, line_total FROM sales_document_items WHERE document_id = ?1 ORDER BY line_no")?
        .query_map([id], |r| {
            Ok(SalesDocLine {
                variant_id: r.get(0)?,
                unit_id: r.get(1)?,
                item_name: r.get(2)?,
                qty: r.get(3)?,
                unit_price: r.get(4)?,
                discount: r.get(5)?,
                taxable: r.get(6)?,
                tax_total: r.get(7)?,
                line_total: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(doc)
}

pub fn get_sales_doc(ctx: &Ctx<'_>, input: &IdInput) -> AppResult<SalesDocDetail> {
    ctx.require("sales.view")?;
    sales_doc_detail(ctx.conn, &input.id)
}

/// Creates or updates a quotation / sales order / challan (open documents only).
pub fn save_sales_doc(ctx: &mut Ctx<'_>, input: &SalesDocInput) -> AppResult<SalesDocDetail> {
    ctx.require("quotations.manage")?;
    if !matches!(
        input.doc_type.as_str(),
        "quotation" | "sales_order" | "delivery_challan"
    ) {
        return Err(AppError::field(
            "docType",
            "Use quotation, sales_order or delivery_challan.",
        ));
    }
    let settings = settings::load(ctx.conn)?;
    let today = settings.business_date(ctx.now);
    let valid_until = input
        .valid_until
        .as_deref()
        .map(|d| date("validUntil", d))
        .transpose()?;
    let party = input
        .party_id
        .as_deref()
        .map(|p| party_ledger::load_party(ctx.conn, p, "customer"))
        .transpose()?;
    let (branch_id, _, branch_state) = branch_of(ctx.conn, None)?;
    let own = branch_state.or_else(|| settings.business.state_code.clone());
    let pos = party.as_ref().and_then(|p| p.state_code.clone());
    let list = ListContext {
        list_id: super::price_lists::list_for(ctx.conn, input.party_id.as_deref(), None)?,
        date: &today,
    };
    let priced = price(
        ctx.conn,
        &settings,
        &input.lines,
        input.bill_discount.as_ref(),
        input.prices_include_tax,
        pos.as_deref(),
        own.as_deref(),
        &list,
    )?;
    let approved_by = check_pricing_approvals(ctx.actor, ctx.approver, &priced, false)?;
    let customer_name = match &party {
        Some(p) => p.name.clone(),
        None => input
            .customer_name
            .as_deref()
            .map(|n| required("customerName", n, 160))
            .transpose()?
            .unwrap_or_else(|| "Walk-in Customer".into()),
    };
    let notes = optional("notes", input.notes.as_deref(), 2000)?;
    let terms = optional("terms", input.terms.as_deref(), 2000)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let r = &priced.result;
    let id = write(ctx.conn, |tx| {
        let id = match &input.id {
            Some(id) => {
                let existing = sales_doc_detail(tx, id)?;
                if !matches!(existing.status.as_str(), "draft" | "open") {
                    return Err(AppError::conflict("Only open documents can be edited."));
                }
                tx.execute(
                    "UPDATE sales_documents SET party_id = ?1, customer_name = ?2, customer_phone = ?3, valid_until = ?4, prices_include_tax = ?5,
                       gross = ?6, discount = ?7, taxable = ?8, tax_total = ?9, round_off = ?10, total = ?11, notes = ?12, terms = ?13,
                       updated_at = ?14, updated_by = ?15 WHERE id = ?16",
                    params![
                        input.party_id, customer_name, input.customer_phone, valid_until, priced.prices_include_tax, r.gross,
                        r.line_discount + r.bill_discount, r.taxable, r.tax, r.round_off, r.total, notes, terms, now, actor.user_id, id
                    ],
                )?;
                tx.execute(
                    "DELETE FROM sales_document_items WHERE document_id = ?1",
                    [id],
                )?;
                id.clone()
            }
            None => {
                let number =
                    numbering::next(tx, &settings, &branch_id, &input.doc_type, &today, &now)?;
                let id = new_id();
                tx.execute(
                    "INSERT INTO sales_documents (id, doc_type, branch_id, series_id, number_period, doc_seq, doc_no, business_date, valid_until,
                        status, party_id, customer_name, customer_phone, prices_include_tax, gross, discount, taxable, tax_total, round_off,
                        total, notes, terms, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'open', ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?22, ?23, ?23)",
                    params![
                        id, input.doc_type, branch_id, number.series_id, number.period, number.seq, number.doc_no, today, valid_until,
                        input.party_id, customer_name, input.customer_phone, priced.prices_include_tax, r.gross,
                        r.line_discount + r.bill_discount, r.taxable, r.tax, r.round_off, r.total, notes, terms, now, actor.user_id
                    ],
                )?;
                id
            }
        };
        for (index, (line, lr)) in priced.lines.iter().zip(&r.lines).enumerate() {
            tx.execute(
                "INSERT INTO sales_document_items (id, document_id, line_no, variant_id, unit_id, item_name, sku, qty, base_qty, unit_price,
                    discount, taxable, tax_rate, tax_total, line_total, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
                params![
                    new_id(), id, index + 1, line.variant_id, line.unit_id, line.name, line.sku, line.qty, line.base_qty, line.unit_price,
                    lr.line_discount + lr.bill_discount, lr.taxable, line.tax_rate, lr.tax, lr.total, now
                ],
            )?;
        }
        AuditEntry::new("SALES_DOC_SAVED", "sales_documents", Some(&id))
            .by(actor)
            .approved_by(approved_by.as_deref())
            .write(tx, &now)?;
        Ok(id)
    })?;
    sales_doc_detail(ctx.conn, &id)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocStatusInput {
    pub id: String,
    /// "open", "accepted", "declined" or "cancelled".
    pub status: String,
}

pub fn set_sales_doc_status(
    ctx: &mut Ctx<'_>,
    input: &SalesDocStatusInput,
) -> AppResult<SalesDocDetail> {
    ctx.require("quotations.manage")?;
    if !matches!(
        input.status.as_str(),
        "open" | "accepted" | "declined" | "cancelled"
    ) {
        return Err(AppError::field(
            "status",
            "Use open, accepted, declined or cancelled.",
        ));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let doc = sales_doc_detail(tx, &input.id)?;
        if matches!(doc.status.as_str(), "converted" | "cancelled") {
            return Err(AppError::conflict("This document is closed."));
        }
        tx.execute("UPDATE sales_documents SET status = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4", params![input.status, now, actor.user_id, input.id])?;
        AuditEntry::new("SALES_DOC_STATUS", "sales_documents", Some(&input.id))
            .by(actor)
            .summary(input.status.clone())
            .write(tx, &now)?;
        sales_doc_detail(tx, &input.id)
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConvertInput {
    pub document_id: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
    pub expected_total: Option<i64>,
    pub register_session_id: Option<String>,
}

/// Turns an open quotation / order into an invoice at the quoted prices.
pub fn convert_sales_doc(ctx: &mut Ctx<'_>, input: &ConvertInput) -> AppResult<InvoiceDetail> {
    let doc = sales_doc_detail(ctx.conn, &input.document_id)?;
    if !matches!(doc.status.as_str(), "open" | "accepted") {
        return Err(AppError::conflict(
            "Only open or accepted documents can be converted.",
        ));
    }
    let sale = SaleInput {
        idempotency_key: input.idempotency_key.clone(),
        warehouse_id: None,
        party_id: doc.party_id.clone(),
        customer_name: Some(doc.customer_name.clone()),
        customer_phone: None,
        customer_gstin: None,
        place_of_supply: None,
        business_date: None,
        prices_include_tax: Some(doc.prices_include_tax),
        lines: doc
            .lines
            .iter()
            .map(|l| SaleLineInput {
                variant_id: l.variant_id.clone(),
                unit_id: Some(l.unit_id.clone()),
                qty: l.qty,
                unit_price: Some(l.unit_price),
                discount: (l.discount > 0).then_some(Discount::Amount { amount: l.discount }),
                batch_id: None,
            })
            .collect(),
        bill_discount: None,
        payments: input.payments.clone(),
        expected_total: input.expected_total,
        notes: doc.notes.clone(),
        source_document_id: Some(doc.id.clone()),
        register_session_id: input.register_session_id.clone(),
    };
    // The quoted prices were approved when the quotation was saved.
    create_invoice_inner(ctx, &sale, true, None)
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocListQuery {
    pub doc_type: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub party_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocListItem {
    pub id: String,
    pub doc_type: String,
    pub doc_no: String,
    pub business_date: String,
    pub customer_name: String,
    pub total: i64,
    pub status: String,
    pub valid_until: Option<String>,
}

pub fn list_sales_docs(
    ctx: &Ctx<'_>,
    query: &SalesDocListQuery,
) -> AppResult<Page<SalesDocListItem>> {
    ctx.require("sales.view")?;
    let mut filters = vec!["deleted_at IS NULL".to_string()];
    let mut args: Vec<SqlValue> = Vec::new();
    if let Some(t) = &query.doc_type {
        filters.push("doc_type = ?".into());
        args.push(t.clone().into());
    }
    if let Some(s) = &query.status {
        filters.push("status = ?".into());
        args.push(s.clone().into());
    }
    if let Some(party) = &query.party_id {
        filters.push("party_id = ?".into());
        args.push(party.clone().into());
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
        filters.push(
            "(doc_no LIKE ? ESCAPE '\\' OR lower(customer_name) LIKE lower(?) ESCAPE '\\')".into(),
        );
        let p = like_pattern(search);
        args.extend([p.clone().into(), p.into()]);
    }
    let where_sql = filters.join(" AND ");
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM sales_documents WHERE {where_sql}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let order = query.page.order_by(
        &[
            ("docNo", "doc_no"),
            ("type", "doc_type"),
            ("date", "business_date"),
            ("customer", "lower(customer_name)"),
            ("total", "total"),
            ("status", "status"),
            ("validUntil", "coalesce(valid_until, '')"),
        ],
        "business_date DESC, created_at DESC, id",
        "id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    args.extend([limit.into(), offset.into()]);
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT id, doc_type, doc_no, business_date, customer_name, total, status, valid_until FROM sales_documents WHERE {where_sql}
             ORDER BY {order} LIMIT ? OFFSET ?"
        ))?
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(SalesDocListItem {
                id: r.get(0)?,
                doc_type: r.get(1)?,
                doc_no: r.get(2)?,
                business_date: r.get(3)?,
                customer_name: r.get(4)?,
                total: r.get(5)?,
                status: r.get(6)?,
                valid_until: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

// ------------------------------------------------------------------ held bills

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HoldInput {
    pub label: String,
    pub party_id: Option<String>,
    /// The POS cart as the client stores it (opaque to the backend).
    pub cart: Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HeldBill {
    pub id: String,
    pub label: String,
    pub party_id: Option<String>,
    pub cart: Value,
    pub cart_format: String,
    pub created_at: String,
    pub created_by_name: Option<String>,
}

fn held(conn: &Connection, id: &str) -> AppResult<HeldBill> {
    conn.query_row(
        "SELECT h.id, h.label, h.party_id, h.cart_json, h.cart_format, h.created_at, u.name FROM held_bills h LEFT JOIN users u ON u.id = h.created_by
         WHERE h.id = ?1",
        [id],
        |r| {
            let cart: String = r.get(3)?;
            Ok(HeldBill {
                id: r.get(0)?,
                label: r.get(1)?,
                party_id: r.get(2)?,
                cart: serde_json::from_str(&cart).unwrap_or(Value::Null),
                cart_format: r.get(4)?,
                created_at: r.get(5)?,
                created_by_name: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The held bill"))
}

pub fn hold(ctx: &mut Ctx<'_>, input: &HoldInput) -> AppResult<HeldBill> {
    ctx.require("sales.create")?;
    let label = required("label", &input.label, 60)?;
    if !input.cart.is_object() && !input.cart.is_array() {
        return Err(AppError::field(
            "cart",
            "The cart must be a JSON object or array.",
        ));
    }
    let id = new_id();
    let now = ctx.now_str();
    ctx.conn.execute(
        "INSERT INTO held_bills (id, label, party_id, cart_json, cart_format, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, 'v2', ?5, ?5, ?6, ?6)",
        params![id, label, input.party_id, input.cart.to_string(), now, ctx.actor.user_id],
    )?;
    held(ctx.conn, &id)
}

pub fn list_held(ctx: &Ctx<'_>) -> AppResult<Vec<HeldBill>> {
    ctx.require("sales.create")?;
    let ids: Vec<String> = ctx
        .conn
        .prepare("SELECT id FROM held_bills WHERE deleted_at IS NULL ORDER BY created_at")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    ids.iter().map(|id| held(ctx.conn, id)).collect()
}

/// Takes a held bill back to the till. It is removed in the same step, so two counters can't
/// both recall (and bill) it (fixes B-16).
pub fn recall(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<HeldBill> {
    ctx.require("sales.create")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let bill = held(tx, &input.id)?;
        let changed = tx.execute(
            "UPDATE held_bills SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![now, actor.user_id, input.id],
        )?;
        if changed == 0 {
            return Err(AppError::conflict(
                "This bill was already recalled or discarded.",
            ));
        }
        Ok(bill)
    })
}
