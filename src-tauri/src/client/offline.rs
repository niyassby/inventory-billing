//! What a counter can do while the hub is unreachable (docs/ARCHITECTURE.md §6, D-8): search and
//! look up items, preview bills and make **fully paid** sales from the cached catalogue, numbered
//! from a leased block. Everything else needs the hub and fails with HUB_OFFLINE.
//!
//! Offline bills are priced by `sales::price_lines`, the same code the hub runs, so the totals on
//! the receipt are the totals the hub posts when the sale syncs (it checks them: TOTALS_MISMATCH
//! otherwise). Rules that need live data or a manager stay on the hub: price overrides, discounts
//! above the cashier's limit, credit sales, back-dating and conversions are refused offline.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::store::{self, CachedContext, NewOutboxItem};
use crate::domain::ids::new_id;
use crate::domain::money::{self, RoundingMode};
use crate::domain::numbering::format_doc_number;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::services::auth::SessionUser;
use crate::services::catalog::{SearchItem, SearchQuery};
use crate::services::sales::{
    self, InvoiceDetail, InvoiceLine, PaymentSummary, Preview, PreviewInput, PreviewLine, Priced,
    ResolvedLine, SaleInput, SaleLineInput, PAYMENT_MODES,
};
use crate::services::settings::{OfflineSalesPolicy, Settings};
use crate::services::sync::OfflineInvoice;

/// Commands a counter answers by itself while the hub is offline.
pub const OFFLINE_COMMANDS: &[&str] = &[
    "auth_me",
    "settings_public",
    "products_search",
    "products_lookup",
    "sales_preview",
    "sales_create_invoice",
];

fn offline(message: &str) -> AppError {
    AppError::new(ErrorCode::HubOffline, message)
}

fn context(conn: &Connection) -> AppResult<CachedContext> {
    store::context(conn)?.ok_or_else(|| {
        offline("The hub can't be reached and this counter has no catalogue saved yet.")
    })
}

fn user(conn: &Connection, token: &str, now: DateTime<Utc>) -> AppResult<SessionUser> {
    store::session(conn, token, &now.to_rfc3339())?.ok_or_else(|| {
        AppError::new(
            ErrorCode::Unauthorized,
            "Your session can't be checked while the hub is offline. Sign in again when it is back.",
        )
    })
}

fn require(user: &SessionUser, permission: &str) -> AppResult<()> {
    if user.permissions.iter().any(|p| p == permission) {
        Ok(())
    } else {
        Err(AppError::forbidden(permission))
    }
}

fn input<T: serde::de::DeserializeOwned>(value: &Value) -> AppResult<T> {
    serde_json::from_value(value.clone())
        .map_err(|e| AppError::validation(format!("Invalid input: {e}")))
}

/// Handles one command offline. `None`: the command needs the hub.
pub fn handle(
    conn: &mut Connection,
    name: &str,
    token: &str,
    payload: &Value,
    now: DateTime<Utc>,
) -> Option<AppResult<Value>> {
    if !OFFLINE_COMMANDS.contains(&name) {
        return None;
    }
    let result = (|| -> AppResult<Value> {
        let user = user(conn, token, now)?;
        match name {
            "auth_me" => Ok(serde_json::to_value(&user)?),
            "settings_public" => {
                let s = context(conn)?.settings;
                Ok(
                    json!({ "business": s.business, "tax": s.tax, "invoice": s.invoice,
                           "inventory": { "negativeStockPolicy": s.inventory.negative_stock_policy } }),
                )
            }
            "products_search" => {
                require(&user, "products.view")?;
                let q: SearchQuery = input(payload)?;
                Ok(serde_json::to_value(store::search(
                    conn,
                    &q.term,
                    q.limit.unwrap_or(20),
                )?)?)
            }
            "products_lookup" => {
                require(&user, "products.view")?;
                let q: SearchQuery = input(payload)?;
                Ok(serde_json::to_value(store::lookup(conn, &q.term)?)?)
            }
            "sales_preview" => {
                require(&user, "sales.create")?;
                let p: PreviewInput = input(payload)?;
                Ok(serde_json::to_value(preview(conn, &p)?)?)
            }
            "sales_create_invoice" => {
                let sale: SaleInput = input(payload)?;
                Ok(serde_json::to_value(create_invoice(
                    conn, &user, token, &sale, now,
                )?)?)
            }
            _ => Err(offline("This needs the hub.")),
        }
    })();
    Some(result)
}

fn resolved(item: &SearchItem, line: &SaleLineInput, index: usize) -> AppResult<ResolvedLine> {
    if line.qty <= 0 {
        return Err(AppError::field(
            &format!("lines.{index}.qty"),
            "Quantity must be greater than zero.",
        ));
    }
    let base_qty = i64::try_from(money::div_round(
        i128::from(line.qty) * i128::from(item.factor_num),
        i128::from(item.factor_den),
        RoundingMode::HalfUp,
    ))
    .map_err(AppError::internal)?;
    Ok(ResolvedLine {
        variant_id: item.variant_id.clone(),
        name: item.name.clone(),
        sku: item.sku.clone(),
        hsn_sac: item.hsn_sac.clone(),
        unit_id: item.unit_id.clone(),
        unit_name: item.unit_code.clone(),
        factor_num: item.factor_num,
        factor_den: item.factor_den,
        qty: line.qty,
        base_qty,
        catalog_price: item.sale_price,
        unit_price: item.sale_price,
        mrp: item.mrp,
        discount: None,
        tax_rate_id: item.tax_rate_id.clone(),
        tax_rate: item.tax_rate,
        tax_type: item.tax_type.clone(),
        cess_rate: item.cess_rate,
        cost: 0,
        is_service: item.is_service,
        batch_id: line.batch_id.clone(),
    })
}

/// Prices lines from the cache, applying the offline restrictions.
fn price(
    conn: &Connection,
    ctx: &CachedContext,
    lines: &[SaleLineInput],
    bill_discount: Option<&crate::domain::pricing::Discount>,
    prices_include_tax: Option<bool>,
    place_of_supply: Option<&str>,
) -> AppResult<Priced> {
    let settings = &ctx.settings;
    let own_state = ctx
        .branch_state
        .clone()
        .or_else(|| settings.business.state_code.clone());
    sales::price_lines(
        settings,
        lines,
        bill_discount,
        prices_include_tax,
        place_of_supply,
        own_state.as_deref(),
        |index, line| {
            let item =
                store::item(conn, &line.variant_id, line.unit_id.as_deref())?.ok_or_else(|| {
                    AppError::field(
                    &format!("lines.{index}.variantId"),
                    "This item can't be sold offline (it isn't in this counter's catalogue yet).",
                )
                })?;
            if settings.network.offline_sales_policy == OfflineSalesPolicy::AllowRestricted
                && !item.is_service
                && (item.track_batches || item.stock <= settings.network.restricted_min_stock)
            {
                return Err(AppError::field(
                    &format!("lines.{index}.variantId"),
                    format!(
                        "{} can't be sold while the hub is offline (low stock or batch item).",
                        item.name
                    ),
                ));
            }
            resolved(&item, line, index)
        },
    )
}

fn place_of_supply(
    conn: &Connection,
    party_id: Option<&str>,
    explicit: Option<&str>,
) -> AppResult<Option<String>> {
    if let Some(pos) = explicit {
        return Ok(Some(pos.to_string()));
    }
    Ok(match party_id {
        Some(id) => store::customer(conn, id)?.and_then(|c| {
            c.state_code
                .or_else(|| c.gstin.as_ref().map(|g| g[..2].to_string()))
        }),
        None => None,
    })
}

pub fn preview(conn: &Connection, input: &PreviewInput) -> AppResult<Preview> {
    let ctx = context(conn)?;
    let pos = place_of_supply(
        conn,
        input.party_id.as_deref(),
        input.place_of_supply.as_deref(),
    )?;
    let priced = price(
        conn,
        &ctx,
        &input.lines,
        input.bill_discount.as_ref(),
        input.prices_include_tax,
        pos.as_deref(),
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

fn refuse(message: &str) -> AppError {
    AppError::new(ErrorCode::HubOffline, message)
}

/// A fully paid sale made offline: numbered from a lease, queued for the hub, returned for the
/// receipt exactly as the hub will record it.
pub fn create_invoice(
    conn: &mut Connection,
    user: &SessionUser,
    token: &str,
    input: &SaleInput,
    now: DateTime<Utc>,
) -> AppResult<InvoiceDetail> {
    require(user, "sales.create")?;
    crate::services::idempotency::validate_key(&input.idempotency_key)?;
    let ctx = context(conn)?;
    let settings: &Settings = &ctx.settings;
    if settings.network.offline_sales_policy == OfflineSalesPolicy::Block {
        return Err(refuse(
            "The hub is offline and offline sales are turned off.",
        ));
    }
    let today = settings.business_date(now);
    if input.business_date.as_deref().is_some_and(|d| d != today) {
        return Err(refuse("A bill for another day needs the hub."));
    }
    if input.source_document_id.is_some() {
        return Err(refuse("Converting a quotation or order needs the hub."));
    }
    let customer = match &input.party_id {
        Some(id) => Some(store::customer(conn, id)?.ok_or_else(|| {
            AppError::field("partyId", "This customer isn't saved on this counter yet.")
        })?),
        None => None,
    };
    let pos = place_of_supply(
        conn,
        input.party_id.as_deref(),
        input.place_of_supply.as_deref(),
    )?;
    let priced = price(
        conn,
        &ctx,
        &input.lines,
        input.bill_discount.as_ref(),
        input.prices_include_tax,
        pos.as_deref(),
    )?;
    if priced.lines.iter().any(|l| l.unit_price != l.catalog_price) {
        return Err(refuse("Changing an item's price needs the hub."));
    }
    let r = &priced.result;
    let discount = r.line_discount + r.bill_discount;
    if discount > 0 {
        require(user, "sales.discount")?;
        let rate = if r.gross > 0 {
            i64::try_from(i128::from(discount) * 1_000_000 / i128::from(r.gross))
                .unwrap_or(i64::MAX)
        } else {
            0
        };
        if rate > user.max_discount_rate {
            return Err(refuse(
                "This discount needs a manager's approval, which needs the hub.",
            ));
        }
    }
    if input.expected_total.is_some_and(|t| t != r.total) {
        return Err(AppError::new(
            ErrorCode::TotalsMismatch,
            "The bill total changed. Please review the bill.",
        )
        .with_details(json!({ "expected": input.expected_total, "actual": r.total })));
    }
    let mut paid = 0;
    for (i, p) in input.payments.iter().enumerate() {
        if !PAYMENT_MODES.contains(&p.mode.as_str()) || p.amount <= 0 {
            return Err(AppError::field(
                &format!("payments.{i}"),
                "Invalid payment.",
            ));
        }
        if p.tendered.is_some_and(|t| t < p.amount) {
            return Err(AppError::field(
                &format!("payments.{i}.tendered"),
                "Cash handed over is less than the amount.",
            ));
        }
        paid += p.amount;
    }
    if paid != r.total {
        return Err(AppError::field(
            "payments",
            "While the hub is offline, bills must be paid in full.",
        ));
    }

    // What the hub will receive: prices fixed to what the customer paid, the total to check.
    let mut queued = input.clone();
    for (line, resolved) in queued.lines.iter_mut().zip(&priced.lines) {
        line.unit_price = Some(resolved.unit_price);
    }
    queued.expected_total = Some(r.total);
    queued.business_date = None;

    let fy = settings.fy_label(&today);
    let now_str = now.to_rfc3339();
    let invoice_id = new_id();
    let tx = conn.transaction()?;
    let (lease, seq) = store::take_number(&tx, &fy)?.ok_or_else(|| {
        refuse("This counter has no offline invoice numbers left. Reconnect to the hub.")
    })?;
    let doc_no = format_doc_number(
        &lease.template,
        &lease.prefix,
        &fy,
        &lease.branch_code,
        seq,
        lease.padding,
    );
    let payload = serde_json::to_value(OfflineInvoice {
        invoice_id: invoice_id.clone(),
        series_id: lease.series_id.clone(),
        period: lease.period.clone(),
        seq,
        sale: queued,
    })?;
    store::enqueue(
        &tx,
        &NewOutboxItem {
            id: &new_id(),
            idempotency_key: &input.idempotency_key,
            command: "sales_create_invoice",
            payload: &payload,
            session_token: token,
            client_created_at: &now_str,
            doc_no: &doc_no,
            total: r.total,
        },
    )?;
    tx.commit()?;

    let lines: Vec<InvoiceLine> = priced
        .lines
        .iter()
        .zip(&r.lines)
        .enumerate()
        .map(|(i, (l, lr))| {
            let cols = sales::tax_columns(&lr.taxes);
            InvoiceLine {
                id: new_id(),
                line_no: i as i64 + 1,
                variant_id: l.variant_id.clone(),
                item_name: l.name.clone(),
                sku: Some(l.sku.clone()),
                hsn_sac: l.hsn_sac.clone(),
                unit_name: Some(l.unit_name.clone()),
                qty: l.qty,
                unit_price: l.unit_price,
                mrp: l.mrp,
                line_discount: lr.line_discount,
                bill_discount_share: lr.bill_discount,
                taxable: lr.taxable,
                tax_rate: l.tax_rate + l.cess_rate,
                cgst: cols.cgst,
                sgst: cols.sgst,
                igst: cols.igst,
                other_tax: cols.other,
                cess: cols.cess,
                tax_total: lr.tax,
                line_total: lr.total,
                returned_qty: 0,
            }
        })
        .collect();
    let payments = input
        .payments
        .iter()
        .map(|p| PaymentSummary {
            id: new_id(),
            doc_no: String::new(),
            business_date: today.clone(),
            mode: p.mode.clone(),
            amount: p.amount,
            tendered: p.tendered,
            change_given: p.tendered.map_or(0, |t| t - p.amount),
            status: "pending_sync".into(),
        })
        .collect();
    Ok(InvoiceDetail {
        id: invoice_id,
        doc_no,
        business_date: today,
        status: "finalized".into(),
        payment_status: "paid".into(),
        party_id: input.party_id.clone(),
        customer_name: customer
            .as_ref()
            .map(|c| c.name.clone())
            .or_else(|| input.customer_name.clone())
            .unwrap_or_else(|| "Walk-in Customer".into()),
        customer_phone: customer
            .as_ref()
            .and_then(|c| c.phone.clone())
            .or_else(|| input.customer_phone.clone()),
        customer_gstin: customer
            .as_ref()
            .and_then(|c| c.gstin.clone())
            .or_else(|| input.customer_gstin.clone()),
        billing_address: None,
        place_of_supply: pos,
        is_inter_state: priced.inter_state,
        prices_include_tax: priced.prices_include_tax,
        gross: r.gross,
        line_discount: r.line_discount,
        bill_discount: r.bill_discount,
        taxable: r.taxable,
        tax_total: r.tax,
        round_off: r.round_off,
        total: r.total,
        amount_paid: r.total,
        balance_due: 0,
        notes: input.notes.clone(),
        created_by_name: Some(user.name.clone()),
        finalized_at: Some(now_str),
        voided_at: None,
        void_reason: None,
        snapshot_source: "live".into(),
        origin: "offline".into(),
        tax_summary: sales::tax_summary(&lines),
        lines,
        payments,
        warnings: Vec::new(),
    })
}
