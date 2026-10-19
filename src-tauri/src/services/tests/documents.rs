//! The lists and look-ups behind the screens: stock documents, stores, receipts, returns.

use crate::error::ErrorCode;
use crate::services::documents::{
    self, ReceiptListQuery, ReturnListQuery, StockDocQuery, StockDocRef, WarehouseInput,
};
use crate::services::purchases::{
    self, GoodsReceiptInput, PurchaseBillInput, PurchaseLineInput, PurchaseReturnInput,
    PurchaseReturnLineInput,
};
use crate::services::sales::{
    self, PaymentInput, ReturnInput, ReturnLineInput, SaleInput, SaleLineInput,
};
use crate::services::stock::{
    self, AdjustInput, AdjustLine, CountStartInput, TransferInput, TransferLine,
};
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

fn line(variant: &str, units: i64, unit_cost: i64) -> PurchaseLineInput {
    PurchaseLineInput {
        variant_id: variant.into(),
        unit_id: None,
        qty: units * 1000,
        unit_cost,
        discount: None,
        batch_no: None,
        expiry_date: None,
        mfg_date: None,
        po_item_id: None,
    }
}

#[test]
fn stock_documents_and_stores_are_listed_and_opened() {
    let kit = Kit::new();
    let rice = kit.product("Rice", "RICE-1", 6_000, 4_000, 20, None);
    let owner = kit.owner();
    let mut conn = kit.conn();

    // A second store, then an adjustment, a transfer to it and a count.
    let shop = documents::save_warehouse(
        &mut kit.ctx(&mut conn, &owner),
        &WarehouseInput {
            id: None,
            code: "shop".into(),
            name: "Front shop".into(),
        },
    )
    .unwrap();
    assert_eq!((shop.code.as_str(), shop.is_default), ("SHOP", false));
    let renamed = documents::save_warehouse(
        &mut kit.ctx(&mut conn, &owner),
        &WarehouseInput {
            id: Some(shop.id.clone()),
            code: "SHOP".into(),
            name: "Shop floor".into(),
        },
    )
    .unwrap();
    assert_eq!(renamed.name, "Shop floor");
    let adj = stock::adjust(
        &mut kit.ctx(&mut conn, &owner),
        &AdjustInput {
            idempotency_key: "adj-doc-0001".into(),
            warehouse_id: None,
            reason: "Damaged".into(),
            notes: None,
            lines: vec![AdjustLine {
                variant_id: rice.clone(),
                batch_id: None,
                qty_change: -2_000,
                unit_cost: None,
            }],
        },
    )
    .unwrap();
    let main = crate::services::setup::default_location(&conn).unwrap().1;
    let tr = stock::transfer(
        &mut kit.ctx(&mut conn, &owner),
        &TransferInput {
            idempotency_key: "trf-doc-0001".into(),
            from_warehouse_id: main,
            to_warehouse_id: shop.id.clone(),
            notes: Some("Morning refill".into()),
            lines: vec![TransferLine {
                variant_id: rice.clone(),
                batch_id: None,
                qty: 5_000,
            }],
        },
    )
    .unwrap();
    stock::start_count(
        &mut kit.ctx(&mut conn, &owner),
        &CountStartInput {
            warehouse_id: None,
            scope_note: Some("Shelf A".into()),
            variant_ids: None,
            category_id: None,
        },
    )
    .unwrap();

    let list = |kind: &str| {
        documents::list_stock_documents(
            &kit.ctx(&mut kit.conn(), &owner),
            &StockDocQuery {
                kind: kind.into(),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let adjustments = list("adjustment");
    assert_eq!(
        (adjustments.total, adjustments.items[0].note.as_deref()),
        (1, Some("Damaged"))
    );
    let transfers = list("transfer");
    assert_eq!(
        transfers.items[0].to_warehouse_name.as_deref(),
        Some("Shop floor")
    );
    let counts = list("count");
    assert_eq!(
        (counts.total, counts.items[0].note.as_deref()),
        (1, Some("Shelf A"))
    );
    assert_eq!(
        code(documents::list_stock_documents(
            &kit.ctx(&mut conn, &owner),
            &StockDocQuery {
                kind: "other".into(),
                ..Default::default()
            }
        )),
        ErrorCode::Validation
    );

    let opened = documents::get_stock_document(
        &kit.ctx(&mut conn, &owner),
        &StockDocRef {
            kind: "adjustment".into(),
            id: adj.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        (opened.doc_no, opened.lines[0].qty, opened.lines[0].value),
        (adj.doc_no, -2_000, -8_000)
    );
    let moved = documents::get_stock_document(
        &kit.ctx(&mut conn, &owner),
        &StockDocRef {
            kind: "transfer".into(),
            id: tr.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(moved.lines[0].qty, 5_000);

    // Stores are a settings matter; cashiers can't change them.
    let cashier = kit.user("cashier");
    assert_eq!(
        code(documents::save_warehouse(
            &mut kit.ctx(&mut conn, &cashier),
            &WarehouseInput {
                id: None,
                code: "x".into(),
                name: "x".into()
            }
        )),
        ErrorCode::Forbidden
    );
}

#[test]
fn receipts_and_returns_are_listed_and_opened() {
    let kit = Kit::new();
    let rice = kit.product("Rice", "RICE-1", 10_000, 4_000, 10, None);
    let supplier = kit.party("Agro Traders", "supplier", None);
    let owner = kit.owner();
    let mut conn = kit.conn();

    // A receipt not billed yet, then billed.
    let grn = purchases::receive_goods(
        &mut kit.ctx(&mut conn, &owner),
        &GoodsReceiptInput {
            idempotency_key: "grn-doc-0001".into(),
            party_id: supplier.clone(),
            warehouse_id: None,
            purchase_order_id: None,
            supplier_reference: Some("DC-9".into()),
            lines: vec![line(&rice, 10, 4_000)],
            notes: None,
        },
    )
    .unwrap();
    let unbilled = |kit: &Kit| {
        documents::list_goods_receipts(
            &kit.ctx(&mut kit.conn(), &owner),
            &ReceiptListQuery {
                unbilled_only: true,
                ..Default::default()
            },
        )
        .unwrap()
        .total
    };
    assert_eq!(unbilled(&kit), 1);
    let opened =
        documents::get_goods_receipt(&kit.ctx(&mut conn, &owner), &IdInput { id: grn.id.clone() })
            .unwrap();
    assert_eq!(
        (opened.value, opened.lines[0].unit_id.is_some()),
        (40_000, true)
    );
    let bill = purchases::create_bill(
        &mut kit.ctx(&mut conn, &owner),
        &PurchaseBillInput {
            idempotency_key: "bill-doc-0001".into(),
            party_id: supplier.clone(),
            supplier_invoice_no: Some("AT-1".into()),
            supplier_invoice_date: None,
            business_date: None,
            due_date: None,
            warehouse_id: None,
            prices_include_tax: None,
            purchase_order_id: None,
            goods_receipt_id: Some(grn.id.clone()),
            lines: vec![line(&rice, 10, 4_000)],
            bill_discount: None,
            charges: Vec::new(),
            payments: Vec::new(),
            expected_total: None,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(unbilled(&kit), 0);
    let all = documents::list_goods_receipts(
        &kit.ctx(&mut conn, &owner),
        &ReceiptListQuery {
            search: Some("DC-9".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        all.items[0].bill_doc_no.as_deref(),
        Some(bill.doc_no.as_str())
    );

    // A purchase return opens with its lines.
    let pret = purchases::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &PurchaseReturnInput {
            idempotency_key: "pret-doc-0001".into(),
            original_bill_id: Some(bill.id.clone()),
            party_id: supplier,
            warehouse_id: None,
            reason: "Wet bags".into(),
            lines: vec![PurchaseReturnLineInput {
                original_item_id: Some(bill.lines[0].id.clone()),
                variant_id: None,
                qty: 2_000,
                unit_cost: None,
            }],
            refund: None,
        },
    )
    .unwrap();
    let back = documents::get_purchase_return(
        &kit.ctx(&mut conn, &owner),
        &IdInput {
            id: pret.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        (back.total, back.lines[0].qty, back.notes.as_deref()),
        (pret.total, 2_000, Some("Wet bags"))
    );

    // A sale, a return on it: listed, and found from the invoice.
    let invoice = sales::create_invoice(
        &mut kit.ctx(&mut conn, &owner),
        &SaleInput {
            idempotency_key: "sale-doc-0001".into(),
            warehouse_id: None,
            party_id: None,
            customer_name: Some("Asha".into()),
            customer_phone: None,
            customer_gstin: None,
            place_of_supply: None,
            business_date: None,
            prices_include_tax: None,
            lines: vec![SaleLineInput {
                variant_id: rice.clone(),
                unit_id: None,
                qty: 2_000,
                unit_price: None,
                discount: None,
                batch_id: None,
            }],
            bill_discount: None,
            payments: vec![PaymentInput {
                mode: "cash".into(),
                amount: 20_000,
                tendered: None,
                account_id: None,
                reference: None,
            }],
            expected_total: None,
            notes: None,
            source_document_id: None,
            register_session_id: None,
        },
    )
    .unwrap();
    sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "ret-doc-0001".into(),
            original_invoice_id: Some(invoice.id.clone()),
            party_id: None,
            warehouse_id: None,
            reason: "Torn bag".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(invoice.lines[0].id.clone()),
                variant_id: None,
                qty: 1_000,
                restock: true,
            }],
            refund: Some(PaymentInput {
                mode: "cash".into(),
                amount: 1,
                tendered: None,
                account_id: None,
                reference: None,
            }),
        },
    )
    .unwrap();
    let returns = documents::list_sales_returns(
        &kit.ctx(&mut conn, &owner),
        &ReturnListQuery {
            search: Some(invoice.doc_no.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        (
            returns.total,
            returns.items[0].invoice_doc_no.as_deref(),
            returns.items[0].amount_refunded
        ),
        (1, Some(invoice.doc_no.as_str()), 10_000)
    );
    let of_invoice =
        documents::returns_of_invoice(&kit.ctx(&mut conn, &owner), &IdInput { id: invoice.id })
            .unwrap();
    assert_eq!(of_invoice.len(), 1);
    drop(conn);
    kit.assert_books_balance();
}
