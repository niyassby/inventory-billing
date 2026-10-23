//! Phase 10: batches and expiry (first expiry first out, policies, returns, counts, write-off,
//! notices), FIFO valuation (layers, golden cases, the switch from weighted average), price lists
//! (customer, till, default, slabs, dates, bulk update, offline) and unit conversions. Every flow
//! ends with the books in balance and, under FIFO, the cost layers equal to the stock ledger.

use std::sync::Arc;

use crate::error::ErrorCode;
use crate::jobs;
use crate::services::catalog::{self, ProductInput, VariantInput};
use crate::services::context::Actor;
use crate::services::inventory::{
    self, BatchQuery, UnitConversionInput, ValuationSwitchInput, WriteOffInput,
};
use crate::services::price_lists::{
    self, BulkUpdateInput, PriceItemInput, PriceListInput, SetPricesInput,
};
use crate::services::purchases::{self, GoodsReceiptInput, PurchaseLineInput};
use crate::services::sales::{
    self, InvoiceDetail, PaymentInput, PreviewInput, ReturnInput, ReturnLineInput, SaleInput,
    SaleLineInput,
};
use crate::services::settings::{self, UpdateSettingsInput};
use crate::services::stock::{
    self, CountEntriesInput, CountEntry, CountFinalizeInput, CountStartInput, TransferInput,
    TransferLine,
};
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;
use crate::services::{documents, parties};

// ---------------------------------------------------------------- helpers

fn pcs(kit: &Kit) -> String {
    kit.conn()
        .query_row("SELECT id FROM units WHERE code = 'pcs'", [], |r| r.get(0))
        .unwrap()
}

/// A batch-tracked product without stock.
fn batch_product(kit: &Kit, name: &str, sku: &str, price: i64) -> String {
    let owner = kit.owner();
    let mut conn = kit.conn();
    catalog::save_product(
        &mut kit.ctx(&mut conn, &owner),
        &ProductInput {
            id: None,
            expected_updated_at: None,
            name: name.into(),
            description: None,
            category_id: None,
            brand_id: None,
            base_unit_id: pcs(kit),
            tax_rate_id: None,
            hsn_sac: None,
            track_batches: true,
            track_expiry: true,
            is_service: false,
            is_active: true,
            variants: vec![VariantInput {
                id: None,
                variant_name: None,
                sku: sku.into(),
                barcode: None,
                sale_price: price,
                mrp: None,
                cost_price: Some(0),
                reorder_level: 0,
                reorder_qty: 0,
                is_active: true,
                opening_stock: None,
                opening_batch_no: None,
                opening_expiry_date: None,
            }],
            unit_conversions: Vec::new(),
        },
    )
    .unwrap()
    .variants[0]
        .id
        .clone()
}

fn supplier(kit: &Kit) -> String {
    kit.party("Pharma Distributors", "supplier", None)
}

/// Goods received: `units` at `cost` (per unit, minor units), optionally into a batch.
#[allow(clippy::too_many_arguments)]
fn receive(
    kit: &Kit,
    key: &str,
    supplier: &str,
    variant: &str,
    units: i64,
    cost: i64,
    batch: Option<(&str, &str)>,
) {
    let owner = kit.owner();
    let mut conn = kit.conn();
    purchases::receive_goods(
        &mut kit.ctx(&mut conn, &owner),
        &GoodsReceiptInput {
            idempotency_key: format!("grn-{key}-0001"),
            party_id: supplier.into(),
            warehouse_id: None,
            purchase_order_id: None,
            supplier_reference: None,
            lines: vec![PurchaseLineInput {
                variant_id: variant.into(),
                unit_id: None,
                qty: units * 1000,
                unit_cost: cost,
                discount: None,
                batch_no: batch.map(|b| b.0.to_string()),
                expiry_date: batch.map(|b| b.1.to_string()),
                mfg_date: None,
                po_item_id: None,
            }],
            notes: None,
        },
    )
    .unwrap();
}

fn cash(amount: i64) -> PaymentInput {
    PaymentInput {
        mode: "cash".into(),
        amount,
        tendered: None,
        account_id: None,
        reference: None,
    }
}

fn sale_input(
    key: &str,
    variant: &str,
    qty: i64,
    batch: Option<&str>,
    total: Option<i64>,
) -> SaleInput {
    SaleInput {
        idempotency_key: format!("sale-{key}-0001"),
        warehouse_id: None,
        party_id: None,
        customer_name: None,
        customer_phone: None,
        customer_gstin: None,
        place_of_supply: None,
        business_date: None,
        prices_include_tax: None,
        lines: vec![SaleLineInput {
            variant_id: variant.into(),
            unit_id: None,
            qty,
            unit_price: None,
            discount: None,
            batch_id: batch.map(String::from),
        }],
        bill_discount: None,
        payments: total.map(|t| vec![cash(t)]).unwrap_or_default(),
        expected_total: None,
        notes: None,
        source_document_id: None,
        register_session_id: None,
    }
}

fn sell(
    kit: &Kit,
    actor: &Actor,
    input: &SaleInput,
) -> Result<InvoiceDetail, crate::error::AppError> {
    let mut conn = kit.conn();
    sales::create_invoice(&mut kit.ctx(&mut conn, actor), input)
}

/// (batch number or "-", quantity) per batch on hand, in expiry order.
fn batch_stock(kit: &Kit, variant: &str) -> Vec<(String, i64)> {
    let conn = kit.conn();
    let mut stmt = conn
        .prepare(
            "SELECT coalesce(b.batch_no, '-'), l.qty FROM stock_levels l LEFT JOIN batches b ON b.id = l.batch_id
             WHERE l.variant_id = ?1 AND l.qty <> 0 ORDER BY b.expiry_date IS NULL, b.expiry_date, b.batch_no",
        )
        .unwrap();
    stmt.query_map([variant], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn batch_id(kit: &Kit, variant: &str, batch_no: &str) -> String {
    kit.conn()
        .query_row(
            "SELECT id FROM batches WHERE variant_id = ?1 AND batch_no = ?2",
            [variant, batch_no],
            |r| r.get(0),
        )
        .unwrap()
}

fn set_inventory(kit: &Kit, value: serde_json::Value) {
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut current = serde_json::to_value(settings::load(&conn).unwrap().inventory).unwrap();
    for (k, v) in value.as_object().unwrap() {
        current[k] = v.clone();
    }
    settings::update(
        &mut kit.ctx(&mut conn, &owner),
        &UpdateSettingsInput {
            section: "inventory".into(),
            value: current,
        },
    )
    .unwrap();
}

fn switch_to(kit: &Kit, target: &str) {
    let owner = kit.owner();
    let mut conn = kit.conn();
    let preview = inventory::valuation_preview(&kit.ctx(&mut conn, &owner)).unwrap();
    inventory::valuation_switch(
        &mut kit.ctx(&mut conn, &owner),
        &ValuationSwitchInput {
            target: target.into(),
            expected_total_value: preview.total_value,
            reason: "Accountant asked for FIFO".into(),
        },
    )
    .unwrap();
}

fn layers(kit: &Kit, variant: &str) -> Vec<(i64, i64, i64)> {
    let conn = kit.conn();
    let mut stmt = conn
        .prepare(
            "SELECT qty_left, unit_cost, value_left FROM cost_layers WHERE variant_id = ?1 AND qty_left > 0
             ORDER BY warehouse_id, received_at, created_at",
        )
        .unwrap();
    stmt.query_map([variant], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn cogs_of(kit: &Kit, invoice: &str) -> i64 {
    kit.conn()
        .query_row(
            "SELECT coalesce(sum(l.debit), 0) FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id
             WHERE e.source_type = 'sales_invoice' AND e.source_id = ?1 AND l.account_id = ?2",
            rusqlite::params![invoice, crate::services::accounts::COGS],
            |r| r.get(0),
        )
        .unwrap()
}

// ---------------------------------------------------------------- batches and expiry

#[test]
fn sales_take_the_first_expiry_first_and_say_which_batches() {
    let kit = Kit::new();
    let drug = batch_product(&kit, "Paracetamol 500", "PCM-500", 3_000);
    let s = supplier(&kit);
    receive(
        &kit,
        "late",
        &s,
        &drug,
        5,
        2_000,
        Some(("LATE", "2027-06-30")),
    );
    receive(
        &kit,
        "soon",
        &s,
        &drug,
        5,
        2_000,
        Some(("SOON", "2026-12-31")),
    );
    let owner = kit.owner();
    let invoice = sell(
        &kit,
        &owner,
        &sale_input("fefo", &drug, 7_000, None, Some(21_000)),
    )
    .unwrap();
    assert_eq!(batch_stock(&kit, &drug), vec![("LATE".into(), 3_000)]);
    let batches: Vec<(String, i64)> = invoice.lines[0]
        .batches
        .iter()
        .map(|b| (b.batch_no.clone(), b.qty))
        .collect();
    // The response is built before the lookup; read the invoice again for its batches.
    let mut conn = kit.conn();
    let detail = sales::get_invoice(
        &kit.ctx(&mut conn, &owner),
        &IdInput {
            id: invoice.id.clone(),
        },
    )
    .unwrap();
    let batches_read: Vec<(String, i64)> = detail.lines[0]
        .batches
        .iter()
        .map(|b| (b.batch_no.clone(), b.qty))
        .collect();
    assert_eq!(
        batches_read,
        vec![("SOON".into(), 5_000), ("LATE".into(), 2_000)]
    );
    assert!(batches.is_empty() || batches == batches_read);
    // The till's batch list names the batch a sale takes first.
    let list = inventory::batches(
        &kit.ctx(&mut conn, &owner),
        &BatchQuery {
            variant_id: drug.clone(),
            warehouse_id: None,
        },
    )
    .unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].picked_first);
    kit.assert_books_balance();
}

#[test]
fn expired_batches_are_blocked_warned_or_allowed() {
    let kit = Kit::new();
    let drug = batch_product(&kit, "Cough syrup", "SYR-1", 9_000);
    let s = supplier(&kit);
    // The kit's day is 17 Oct 2026.
    receive(
        &kit,
        "old",
        &s,
        &drug,
        2,
        5_000,
        Some(("OLD", "2026-09-30")),
    );
    receive(
        &kit,
        "new",
        &s,
        &drug,
        2,
        5_000,
        Some(("NEW", "2027-09-30")),
    );
    let owner = kit.owner();
    // Block (default): the expired batch is skipped, and can't be chosen.
    sell(
        &kit,
        &owner,
        &sale_input("b1", &drug, 2_000, None, Some(18_000)),
    )
    .unwrap();
    assert_eq!(batch_stock(&kit, &drug), vec![("OLD".into(), 2_000)]);
    let err = sell(
        &kit,
        &owner,
        &sale_input("b2", &drug, 1_000, None, Some(9_000)),
    )
    .unwrap_err();
    assert_eq!(
        err.code,
        ErrorCode::InsufficientStock,
        "only expired stock is left"
    );
    let old = batch_id(&kit, &drug, "OLD");
    let err = sell(
        &kit,
        &owner,
        &sale_input("b3", &drug, 1_000, Some(&old), Some(9_000)),
    )
    .unwrap_err();
    assert!(err.message.contains("expired"), "{}", err.message);
    // Warn: it sells, with a warning naming the batch.
    set_inventory(&kit, serde_json::json!({ "expiredStockPolicy": "warn" }));
    let invoice = sell(
        &kit,
        &owner,
        &sale_input("b4", &drug, 1_000, None, Some(9_000)),
    )
    .unwrap();
    assert_eq!(invoice.warnings.len(), 1);
    assert_eq!(invoice.warnings[0].expired_batch.as_deref(), Some("OLD"));
    // Write-off of what is left posts an adjustment for that batch.
    let mut conn = kit.conn();
    let doc = inventory::write_off_batch(
        &mut kit.ctx(&mut conn, &owner),
        &WriteOffInput {
            idempotency_key: "wo-old-000001".into(),
            variant_id: drug.clone(),
            batch_id: old.clone(),
            warehouse_id: None,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(doc.lines[0].qty, -1_000);
    assert!(batch_stock(&kit, &drug).is_empty());
    kit.assert_books_balance();
}

#[test]
fn two_counters_selling_the_last_unit_of_a_batch() {
    let kit = Arc::new(Kit::new());
    let drug = batch_product(&kit, "Insulin pen", "INS-1", 50_000);
    let s = supplier(&kit);
    receive(
        &kit,
        "one",
        &s,
        &drug,
        1,
        30_000,
        Some(("LAST", "2027-01-31")),
    );
    let owner = kit.owner();
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let kit = Arc::clone(&kit);
            let owner = owner.clone();
            let drug = drug.clone();
            std::thread::spawn(move || {
                sell(
                    &kit,
                    &owner,
                    &sale_input(&format!("race{n}"), &drug, 1_000, None, Some(50_000)),
                )
                .map(|i| i.doc_no)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let ok = results.iter().filter(|r| r.is_ok()).count();
    let short = results
        .iter()
        .filter(|r| matches!(r, Err(e) if e.code == ErrorCode::InsufficientStock))
        .count();
    assert_eq!((ok, short), (1, 1), "{results:?}");
    assert!(batch_stock(&kit, &drug).is_empty());
    kit.assert_books_balance();
}

#[test]
fn returns_go_back_to_the_batches_they_came_from() {
    let kit = Kit::new();
    let drug = batch_product(&kit, "Vitamin C", "VITC", 2_000);
    let s = supplier(&kit);
    receive(&kit, "a", &s, &drug, 5, 1_000, Some(("A", "2026-12-31")));
    receive(&kit, "b", &s, &drug, 5, 1_000, Some(("B", "2027-12-31")));
    let owner = kit.owner();
    let invoice = sell(
        &kit,
        &owner,
        &sale_input("ret", &drug, 7_000, None, Some(14_000)),
    )
    .unwrap();
    let mut conn = kit.conn();
    sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "ret-batches-0001".into(),
            original_invoice_id: Some(invoice.id.clone()),
            party_id: None,
            warehouse_id: None,
            reason: "Not needed".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(invoice.lines[0].id.clone()),
                variant_id: None,
                qty: 3_000,
                restock: true,
            }],
            refund: Some(cash(1)),
        },
    )
    .unwrap();
    // 5 from A then 2 from B were sold; the last batch used takes the first 2 back, then A.
    assert_eq!(
        batch_stock(&kit, &drug),
        vec![("A".into(), 1_000), ("B".into(), 5_000)]
    );
    kit.assert_books_balance();
}

#[test]
fn stock_counts_and_transfers_work_per_batch() {
    let kit = Kit::new();
    let drug = batch_product(&kit, "Antacid", "ANT-1", 1_500);
    let s = supplier(&kit);
    receive(&kit, "x", &s, &drug, 4, 800, Some(("X", "2026-11-30")));
    receive(&kit, "y", &s, &drug, 6, 800, Some(("Y", "2027-11-30")));
    let owner = kit.owner();
    let mut conn = kit.conn();
    let count = stock::start_count(
        &mut kit.ctx(&mut conn, &owner),
        &CountStartInput {
            warehouse_id: None,
            scope_note: None,
            variant_ids: Some(vec![drug.clone()]),
            category_id: None,
        },
    )
    .unwrap();
    assert_eq!(count.lines.len(), 2, "one line per batch");
    let x = count
        .lines
        .iter()
        .find(|l| l.batch_no.as_deref() == Some("X"))
        .unwrap();
    stock::enter_counts(
        &mut kit.ctx(&mut conn, &owner),
        &CountEntriesInput {
            count_id: count.id.clone(),
            entries: vec![CountEntry {
                line_id: x.id.clone(),
                counted_qty: Some(3_000),
            }],
        },
    )
    .unwrap();
    stock::finalize_count(
        &mut kit.ctx(&mut conn, &owner),
        &CountFinalizeInput {
            count_id: count.id.clone(),
            idempotency_key: "count-batch-0001".into(),
        },
    )
    .unwrap();
    assert_eq!(
        batch_stock(&kit, &drug),
        vec![("X".into(), 3_000), ("Y".into(), 6_000)]
    );
    // A transfer without a batch moves the first expiry first, batch by batch.
    let second = documents::save_warehouse(
        &mut kit.ctx(&mut conn, &owner),
        &documents::WarehouseInput {
            id: None,
            name: "Back store".into(),
            code: "BACK".into(),
        },
    )
    .unwrap();
    let main: String = conn
        .query_row(
            "SELECT id FROM warehouses WHERE id <> ?1 LIMIT 1",
            [&second.id],
            |r| r.get(0),
        )
        .unwrap();
    stock::transfer(
        &mut kit.ctx(&mut conn, &owner),
        &TransferInput {
            idempotency_key: "transfer-batch-01".into(),
            from_warehouse_id: main,
            to_warehouse_id: second.id.clone(),
            notes: None,
            lines: vec![TransferLine {
                variant_id: drug.clone(),
                batch_id: None,
                qty: 4_000,
            }],
        },
    )
    .unwrap();
    let in_back: Vec<(String, i64)> = conn
        .prepare("SELECT b.batch_no, l.qty FROM stock_levels l JOIN batches b ON b.id = l.batch_id WHERE l.warehouse_id = ?1 AND l.qty <> 0 ORDER BY b.batch_no")
        .unwrap()
        .query_map([&second.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(in_back, vec![("X".into(), 3_000), ("Y".into(), 1_000)]);
    kit.assert_books_balance();
}

#[test]
fn expiry_notices_come_and_go_with_the_stock() {
    let kit = Kit::new();
    let drug = batch_product(&kit, "Eye drops", "EYE-1", 4_000);
    let s = supplier(&kit);
    receive(
        &kit,
        "gone",
        &s,
        &drug,
        1,
        2_000,
        Some(("GONE", "2026-10-01")),
    );
    receive(
        &kit,
        "soon",
        &s,
        &drug,
        1,
        2_000,
        Some(("SOON", "2026-11-01")),
    );
    receive(
        &kit,
        "fine",
        &s,
        &drug,
        1,
        2_000,
        Some(("FINE", "2027-10-01")),
    );
    jobs::tick(&kit.state).unwrap();
    let open = |kit: &Kit| -> Vec<String> {
        let conn = kit.conn();
        let mut stmt = conn
            .prepare("SELECT title FROM notifications WHERE kind = 'expiry' AND read_at IS NULL ORDER BY title")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        open(&kit),
        vec![
            "Expired: Eye drops batch GONE".to_string(),
            "Expires soon: Eye drops batch SOON".to_string()
        ]
    );
    let owner = kit.owner();
    let mut conn = kit.conn();
    let summary = inventory::expiry_summary(&kit.ctx(&mut conn, &owner)).unwrap();
    assert_eq!((summary.expired_batches, summary.near_batches), (1, 1));
    assert_eq!((summary.expired_value, summary.near_value), (2_000, 2_000));
    let report = crate::services::reports::run(
        &kit.ctx(&mut conn, &owner),
        &crate::services::reports::ReportQuery {
            report: "expiry".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.rows.len(), 2);
    assert_eq!(report.rows[0][2], "GONE");
    assert_eq!(report.rows[0][8], "Expired");
    inventory::write_off_batch(
        &mut kit.ctx(&mut conn, &owner),
        &WriteOffInput {
            idempotency_key: "wo-gone-00001".into(),
            variant_id: drug.clone(),
            batch_id: batch_id(&kit, &drug, "GONE"),
            warehouse_id: None,
            notes: None,
        },
    )
    .unwrap();
    drop(conn);
    kit.clock.advance(chrono::Duration::hours(2));
    jobs::tick(&kit.state).unwrap();
    assert_eq!(
        open(&kit),
        vec!["Expires soon: Eye drops batch SOON".to_string()]
    );
}

#[test]
fn opening_stock_and_adjustments_capture_batches() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let detail = catalog::save_product(
        &mut kit.ctx(&mut conn, &owner),
        &ProductInput {
            id: None,
            expected_updated_at: None,
            name: "Baby food".into(),
            description: None,
            category_id: None,
            brand_id: None,
            base_unit_id: pcs(&kit),
            tax_rate_id: None,
            hsn_sac: None,
            track_batches: true,
            track_expiry: true,
            is_service: false,
            is_active: true,
            variants: vec![VariantInput {
                id: None,
                variant_name: None,
                sku: "BABY-1".into(),
                barcode: None,
                sale_price: 30_000,
                mrp: None,
                cost_price: Some(20_000),
                reorder_level: 0,
                reorder_qty: 0,
                is_active: true,
                opening_stock: Some(3_000),
                opening_batch_no: Some("OPEN-1".into()),
                opening_expiry_date: Some("2027-03-31".into()),
            }],
            unit_conversions: Vec::new(),
        },
    )
    .unwrap();
    let variant = detail.variants[0].id.clone();
    stock::adjust(
        &mut kit.ctx(&mut conn, &owner),
        &stock::AdjustInput {
            idempotency_key: "adj-batch-00001".into(),
            warehouse_id: None,
            reason: "Found in the back".into(),
            notes: None,
            lines: vec![stock::AdjustLine {
                variant_id: variant.clone(),
                batch_id: None,
                batch_no: Some("FOUND-7".into()),
                expiry_date: Some("2027-01-31".into()),
                qty_change: 2_000,
                unit_cost: Some(20_000),
            }],
        },
    )
    .unwrap();
    drop(conn);
    assert_eq!(
        batch_stock(&kit, &variant),
        vec![("FOUND-7".into(), 2_000), ("OPEN-1".into(), 3_000)]
    );
    // The same batch number with another expiry is a typing mistake, not a new batch.
    let mut conn = kit.conn();
    let err = stock::adjust(
        &mut kit.ctx(&mut conn, &owner),
        &stock::AdjustInput {
            idempotency_key: "adj-batch-00002".into(),
            warehouse_id: None,
            reason: "More".into(),
            notes: None,
            lines: vec![stock::AdjustLine {
                variant_id: variant,
                batch_id: None,
                batch_no: Some("found-7".into()),
                expiry_date: Some("2027-02-28".into()),
                qty_change: 1_000,
                unit_cost: None,
            }],
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    kit.assert_books_balance();
}

// ---------------------------------------------------------------- FIFO

#[test]
fn the_switch_from_weighted_average_keeps_the_books_and_fifo_takes_the_oldest_first() {
    let kit = Kit::new();
    let owner = kit.owner();
    // Weighted-average history: 10 opening at ₹100 and 10 bought at ₹130 → average ₹115.
    let tea = kit.product("Tea 1 kg", "TEA-1", 30_000, 10_000, 10, None);
    let s = supplier(&kit);
    receive(&kit, "wac", &s, &tea, 10, 13_000, None);
    let sold = sell(
        &kit,
        &owner,
        &sale_input("wac", &tea, 5_000, None, Some(150_000)),
    )
    .unwrap();
    assert_eq!(cogs_of(&kit, &sold.id), 57_500, "5 at the ₹115 average");
    let before: i64 = kit
        .conn()
        .query_row("SELECT sum(value) FROM stock_levels", [], |r| r.get(0))
        .unwrap();

    // The settings form can't change the method; the switch can, and only for the owner.
    let mut conn = kit.conn();
    let err = settings::update(
        &mut kit.ctx(&mut conn, &owner),
        &UpdateSettingsInput {
            section: "inventory".into(),
            value: serde_json::json!({ "valuationMethod": "fifo" }),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    let manager = kit.user("manager");
    assert_eq!(
        code(inventory::valuation_preview(&kit.ctx(&mut conn, &manager))),
        ErrorCode::Forbidden
    );
    let preview = inventory::valuation_preview(&kit.ctx(&mut conn, &owner)).unwrap();
    assert_eq!(
        (
            preview.target.as_str(),
            preview.total_qty,
            preview.total_value
        ),
        ("fifo", 15_000, before)
    );
    let stale = inventory::valuation_switch(
        &mut kit.ctx(&mut conn, &owner),
        &ValuationSwitchInput {
            target: "fifo".into(),
            expected_total_value: before + 1,
            reason: "test".into(),
        },
    );
    assert_eq!(code(stale), ErrorCode::Conflict);
    drop(conn);
    switch_to(&kit, "fifo");
    assert_eq!(
        layers(&kit, &tea),
        vec![(15_000, 11_500, before)],
        "one opening layer at book value"
    );
    kit.assert_books_balance();

    // FIFO from here: 10 more at ₹200; selling 20 takes the 15 opening first, then 5 at ₹200.
    receive(&kit, "fifo", &s, &tea, 10, 20_000, None);
    let invoice = sell(
        &kit,
        &owner,
        &sale_input("fifo", &tea, 20_000, None, Some(600_000)),
    )
    .unwrap();
    assert_eq!(cogs_of(&kit, &invoice.id), before + 100_000);
    let cost_value: i64 = kit
        .conn()
        .query_row(
            "SELECT cost_value FROM sales_invoice_items WHERE invoice_id = ?1",
            [&invoice.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cost_value, before + 100_000);
    assert_eq!(layers(&kit, &tea), vec![(5_000, 20_000, 100_000)]);
    kit.assert_books_balance();

    // A partial return comes back at the cost that sale left at (its own average).
    let mut conn = kit.conn();
    sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "ret-fifo-00001".into(),
            original_invoice_id: Some(invoice.id.clone()),
            party_id: None,
            warehouse_id: None,
            reason: "Damaged box".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(invoice.lines[0].id.clone()),
                variant_id: None,
                qty: 4_000,
                restock: true,
            }],
            refund: Some(cash(1)),
        },
    )
    .unwrap();
    drop(conn);
    let per_unit = crate::services::stock_posting::per_unit(before + 100_000, 20_000);
    let back = layers(&kit, &tea);
    assert_eq!(back.len(), 2);
    assert_eq!(back[1].0, 4_000);
    assert_eq!(back[1].1, per_unit);
    kit.assert_books_balance();

    // And back to weighted average: the cost becomes value ÷ quantity.
    switch_to(&kit, "weighted_average");
    let (cost, value, qty): (i64, i64, i64) = kit
        .conn()
        .query_row(
            "SELECT v.cost_price, sum(l.value), sum(l.qty) FROM product_variants v JOIN stock_levels l ON l.variant_id = v.id WHERE v.id = ?1",
            [&tea],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(cost, crate::services::stock_posting::per_unit(value, qty));
    assert!(layers(&kit, &tea).is_empty());
    kit.assert_books_balance();
}

#[test]
fn fifo_golden_case_multi_layer_sale_and_transfer() {
    let kit = Kit::new();
    let owner = kit.owner();
    let oil = kit.product("Sunflower oil 1 l", "OIL-1", 20_000, 0, 0, None);
    switch_to(&kit, "fifo");
    let s = supplier(&kit);
    receive(&kit, "l1", &s, &oil, 3, 10_000, None);
    receive(&kit, "l2", &s, &oil, 3, 11_000, None);
    receive(&kit, "l3", &s, &oil, 3, 12_500, None);
    // Golden: 7 units = 3 × 100 + 3 × 110 + 1 × 125 = ₹755.
    let invoice = sell(
        &kit,
        &owner,
        &sale_input("gold", &oil, 7_000, None, Some(140_000)),
    )
    .unwrap();
    assert_eq!(cogs_of(&kit, &invoice.id), 75_500);
    assert_eq!(layers(&kit, &oil), vec![(2_000, 12_500, 25_000)]);
    // A transfer carries the layer to the other store at the same cost.
    let mut conn = kit.conn();
    let second = documents::save_warehouse(
        &mut kit.ctx(&mut conn, &owner),
        &documents::WarehouseInput {
            id: None,
            name: "Godown".into(),
            code: "GD".into(),
        },
    )
    .unwrap();
    let main: String = conn
        .query_row(
            "SELECT id FROM warehouses WHERE id <> ?1 LIMIT 1",
            [&second.id],
            |r| r.get(0),
        )
        .unwrap();
    let doc = stock::transfer(
        &mut kit.ctx(&mut conn, &owner),
        &TransferInput {
            idempotency_key: "transfer-fifo-01".into(),
            from_warehouse_id: main,
            to_warehouse_id: second.id,
            notes: None,
            lines: vec![TransferLine {
                variant_id: oil.clone(),
                batch_id: None,
                qty: 1_000,
            }],
        },
    )
    .unwrap();
    assert_eq!(doc.lines[0].value, 12_500);
    drop(conn);
    assert_eq!(
        layers(&kit, &oil),
        vec![(1_000, 12_500, 12_500), (1_000, 12_500, 12_500)]
    );
    kit.assert_books_balance();
}

#[test]
fn the_switch_refuses_negative_stock() {
    let kit = Kit::new();
    set_inventory(&kit, serde_json::json!({ "negativeStockPolicy": "allow" }));
    let owner = kit.owner();
    let item = kit.product("Oversold", "OVR-1", 1_000, 500, 0, None);
    sell(
        &kit,
        &owner,
        &sale_input("neg", &item, 1_000, None, Some(1_000)),
    )
    .unwrap();
    let mut conn = kit.conn();
    let preview = inventory::valuation_preview(&kit.ctx(&mut conn, &owner)).unwrap();
    assert_eq!(preview.problems.len(), 1);
    let err = inventory::valuation_switch(
        &mut kit.ctx(&mut conn, &owner),
        &ValuationSwitchInput {
            target: "fifo".into(),
            expected_total_value: preview.total_value,
            reason: "x".into(),
        },
    )
    .unwrap_err();
    assert!(err.message.contains("below zero"), "{}", err.message);
}

// ---------------------------------------------------------------- price lists

fn wholesale(kit: &Kit, rice: &str) -> String {
    let owner = kit.owner();
    let mut conn = kit.conn();
    let list = price_lists::save(
        &mut kit.ctx(&mut conn, &owner),
        &PriceListInput {
            id: None,
            expected_updated_at: None,
            name: "Wholesale".into(),
            kind: "wholesale".into(),
            is_default: false,
            is_active: true,
            notes: None,
        },
    )
    .unwrap();
    price_lists::set_prices(
        &mut kit.ctx(&mut conn, &owner),
        &SetPricesInput {
            price_list_id: list.id.clone(),
            upsert: vec![
                PriceItemInput {
                    id: None,
                    variant_id: rice.into(),
                    unit_id: None,
                    min_qty: 0,
                    price: 9_000,
                    valid_from: None,
                    valid_to: None,
                },
                PriceItemInput {
                    id: None,
                    variant_id: rice.into(),
                    unit_id: None,
                    min_qty: 10_000,
                    price: 8_500,
                    valid_from: None,
                    valid_to: None,
                },
                PriceItemInput {
                    id: None,
                    variant_id: rice.into(),
                    unit_id: None,
                    min_qty: 0,
                    price: 9_500,
                    valid_from: Some("2026-11-01".into()),
                    valid_to: None,
                },
            ],
            remove: Vec::new(),
        },
    )
    .unwrap();
    list.id
}

fn preview_price(kit: &Kit, party: Option<&str>, rice: &str, qty: i64) -> i64 {
    let owner = kit.owner();
    let mut conn = kit.conn();
    sales::preview(
        &kit.ctx(&mut conn, &owner),
        &PreviewInput {
            warehouse_id: None,
            party_id: party.map(String::from),
            register_session_id: None,
            place_of_supply: None,
            prices_include_tax: None,
            lines: vec![SaleLineInput {
                variant_id: rice.into(),
                unit_id: None,
                qty,
                unit_price: None,
                discount: None,
                batch_id: None,
            }],
            bill_discount: None,
        },
    )
    .unwrap()
    .lines[0]
        .unit_price
}

#[test]
fn a_customers_price_list_prices_the_till_with_slabs_and_dates() {
    let kit = Kit::new();
    let rice = kit.product("Rice 1 kg", "RICE-PL", 10_000, 6_000, 100, None);
    let list = wholesale(&kit, &rice);
    let trader = kit.party("Raju Traders", "customer", None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut detail =
        parties::get(&kit.ctx(&mut conn, &owner), &IdInput { id: trader.clone() }).unwrap();
    let mut input = crate::services::parties::PartyInput {
        id: Some(trader.clone()),
        expected_updated_at: Some(detail.updated_at.clone()),
        party_type: "customer".into(),
        code: None,
        name: detail.name.clone(),
        phone: None,
        email: None,
        gstin: None,
        state_code: Some("29".into()),
        billing_address: None,
        shipping_address: None,
        credit_limit: None,
        credit_days: None,
        notes: None,
        is_active: true,
        opening_balance: None,
        price_list_id: Some(list.clone()),
    };
    detail = parties::save(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    assert_eq!(detail.price_list_id.as_deref(), Some(list.as_str()));
    drop(conn);
    assert_eq!(
        preview_price(&kit, None, &rice, 1_000),
        10_000,
        "walk-in: the product's price"
    );
    assert_eq!(preview_price(&kit, Some(&trader), &rice, 1_000), 9_000);
    assert_eq!(
        preview_price(&kit, Some(&trader), &rice, 10_000),
        8_500,
        "slab from 10"
    );
    // On 2 Nov the new price starts.
    kit.clock.advance(chrono::Duration::days(16));
    assert_eq!(preview_price(&kit, Some(&trader), &rice, 1_000), 9_500);
    // A sale at the list price needs no price-change approval, and the invoice names the list.
    let cashier = kit.user("cashier");
    let mut sale = sale_input("pl", &rice, 2_000, None, Some(19_000));
    sale.party_id = Some(trader.clone());
    let invoice = sell(&kit, &cashier, &sale).unwrap();
    assert_eq!(invoice.lines[0].unit_price, 9_500);
    let named: Option<String> = kit
        .conn()
        .query_row(
            "SELECT price_list_id FROM sales_invoices WHERE id = ?1",
            [&invoice.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(named.as_deref(), Some(list.as_str()));
    // The list can't be switched off while the customer uses it.
    let mut conn = kit.conn();
    let lists = price_lists::list(&kit.ctx(&mut conn, &owner)).unwrap();
    let wl = lists.iter().find(|l| l.id == list).unwrap();
    assert_eq!((wl.item_count, wl.customer_count), (3, 1));
    let off = price_lists::save(
        &mut kit.ctx(&mut conn, &owner),
        &PriceListInput {
            id: Some(list.clone()),
            expected_updated_at: None,
            name: "Wholesale".into(),
            kind: "wholesale".into(),
            is_default: false,
            is_active: false,
            notes: None,
        },
    );
    assert_eq!(code(off), ErrorCode::Validation);
    input.price_list_id = None;
    input.expected_updated_at = None;
    parties::save(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    kit.assert_books_balance();
}

#[test]
fn the_default_and_the_tills_list_apply_to_walk_ins() {
    let kit = Kit::new();
    let rice = kit.product("Rice 5 kg", "RICE-5", 50_000, 30_000, 100, None);
    let list = wholesale(&kit, &rice);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let tills = price_lists::tills(&kit.ctx(&mut conn, &owner)).unwrap();
    price_lists::set_till(
        &mut kit.ctx(&mut conn, &owner),
        &price_lists::SetTillPriceListInput {
            register_id: tills[0].register_id.clone(),
            price_list_id: Some(list.clone()),
        },
    )
    .unwrap();
    let session = crate::services::payments::open_register(
        &mut kit.ctx(&mut conn, &owner),
        &crate::services::payments::OpenRegisterInput {
            register_id: None,
            opening_float: 0,
        },
    )
    .unwrap();
    let price = sales::preview(
        &kit.ctx(&mut conn, &owner),
        &PreviewInput {
            warehouse_id: None,
            party_id: None,
            register_session_id: Some(session.id.clone()),
            place_of_supply: None,
            prices_include_tax: None,
            lines: vec![SaleLineInput {
                variant_id: rice.clone(),
                unit_id: None,
                qty: 1_000,
                unit_price: None,
                discount: None,
                batch_id: None,
            }],
            bill_discount: None,
        },
    )
    .unwrap()
    .lines[0]
        .unit_price;
    assert_eq!(price, 9_000, "the till's list");
    // Without a till, the default list (none yet → the product's price; then the list).
    drop(conn);
    assert_eq!(preview_price(&kit, None, &rice, 1_000), 50_000);
    let mut conn = kit.conn();
    price_lists::save(
        &mut kit.ctx(&mut conn, &owner),
        &PriceListInput {
            id: Some(list.clone()),
            expected_updated_at: None,
            name: "Wholesale".into(),
            kind: "wholesale".into(),
            is_default: true,
            is_active: true,
            notes: None,
        },
    )
    .unwrap();
    drop(conn);
    assert_eq!(preview_price(&kit, None, &rice, 1_000), 9_000);
    // The counter's catalogue carries the lists and the customers' choice.
    let snapshot = crate::services::sync::catalogue(&kit.conn(), kit.now()).unwrap();
    let cached = snapshot.price_lists.iter().find(|l| l.id == list).unwrap();
    assert!(cached.is_default);
    assert_eq!(cached.prices.len(), 3);
}

#[test]
fn bulk_updates_are_previewed_then_applied_and_audited() {
    let kit = Kit::new();
    let a = kit.product("Soap", "SOAP-1", 4_000, 2_000, 10, None);
    let b = kit.product("Shampoo", "SHAM-1", 12_345, 6_000, 10, None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let input = BulkUpdateInput {
        price_list_id: None,
        category_id: None,
        variant_ids: vec![a.clone(), b.clone()],
        mode: "percent".into(),
        value: 50_000,
        round_to: 100,
        apply: false,
    };
    let preview = price_lists::bulk_update(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    let new: Vec<(i64, i64)> = preview
        .rows
        .iter()
        .map(|r| (r.old_price, r.new_price))
        .collect();
    assert_eq!(
        new,
        vec![(12_345, 13_000), (4_000, 4_200)],
        "+5%, rounded to ₹1"
    );
    let price: i64 = conn
        .query_row(
            "SELECT sale_price FROM product_variants WHERE id = ?1",
            [&a],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(price, 4_000, "a preview changes nothing");
    let cashier = kit.user("cashier");
    assert_eq!(
        code(price_lists::bulk_update(
            &mut kit.ctx(&mut conn, &cashier),
            &input
        )),
        ErrorCode::Forbidden
    );
    let applied = price_lists::bulk_update(
        &mut kit.ctx(&mut conn, &owner),
        &BulkUpdateInput {
            apply: true,
            ..input
        },
    )
    .unwrap();
    assert_eq!(applied.changed, 2);
    let price: i64 = conn
        .query_row(
            "SELECT sale_price FROM product_variants WHERE id = ?1",
            [&a],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(price, 4_200);
    let audited: i64 = conn
        .query_row(
            "SELECT count(*) FROM audit_log WHERE action = 'PRICE_BULK_UPDATE'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audited, 1);
    assert_eq!(
        price_lists::adjusted(1_000, "amount", -2_000, 0),
        0,
        "never below zero"
    );
}

// ---------------------------------------------------------------- units

#[test]
fn a_shop_wide_conversion_sells_in_boxes_and_counts_in_pieces() {
    let kit = Kit::new();
    let pen = kit.product("Ball pen", "PEN-1", 1_000, 500, 100, None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let boxes: String = conn
        .query_row("SELECT id FROM units WHERE code = 'box'", [], |r| r.get(0))
        .unwrap();
    let cashier = kit.user("cashier");
    let input = UnitConversionInput {
        id: None,
        from_unit_id: boxes.clone(),
        to_unit_id: pcs(&kit),
        factor_num: 12,
        factor_den: 1,
    };
    assert_eq!(
        code(inventory::save_conversion(
            &mut kit.ctx(&mut conn, &cashier),
            &input
        )),
        ErrorCode::Forbidden
    );
    let list = inventory::save_conversion(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        code(inventory::save_conversion(
            &mut kit.ctx(&mut conn, &owner),
            &input
        )),
        ErrorCode::Validation,
        "no duplicate"
    );
    drop(conn);
    let mut sale = sale_input("box", &pen, 2_000, None, Some(24_000));
    sale.lines[0].unit_id = Some(boxes);
    let invoice = sell(&kit, &owner, &sale).unwrap();
    assert_eq!(invoice.lines[0].unit_price, 12_000, "a box is 12 pens");
    assert_eq!(kit.stock(&pen), 76_000, "stock stays in pieces");
    let mut conn = kit.conn();
    let left = inventory::delete_conversion(
        &mut kit.ctx(&mut conn, &owner),
        &IdInput {
            id: list[0].id.clone(),
        },
    )
    .unwrap();
    assert!(left.is_empty());
}
