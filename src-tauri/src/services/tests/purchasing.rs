//! Purchasing (orders, receipts, bills, landed cost, returns, voids), supplier payments, expenses.

use crate::error::ErrorCode;
use crate::services::accounts;
use crate::services::expenses::{self, ExpenseInput, VoidExpenseInput};
use crate::services::payments::{self, PaymentEntryInput};
use crate::services::purchases::{
    self, ChargeInput, GoodsReceiptInput, PurchaseBillInput, PurchaseLineInput, PurchaseOrderInput,
    PurchaseReturnInput, PurchaseReturnLineInput, VoidBillInput,
};
use crate::services::sales::PaymentInput;
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

fn pline(variant: &str, units: i64, unit_cost: i64) -> PurchaseLineInput {
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

fn bill(key: &str, supplier: &str, lines: Vec<PurchaseLineInput>) -> PurchaseBillInput {
    PurchaseBillInput {
        idempotency_key: key.into(),
        party_id: supplier.into(),
        supplier_invoice_no: None,
        supplier_invoice_date: None,
        business_date: None,
        due_date: None,
        warehouse_id: None,
        prices_include_tax: None,
        purchase_order_id: None,
        goods_receipt_id: None,
        lines,
        bill_discount: None,
        charges: Vec::new(),
        payments: Vec::new(),
        expected_total: None,
        notes: None,
    }
}

fn cost_of(kit: &Kit, variant: &str) -> i64 {
    kit.conn()
        .query_row(
            "SELECT cost_price FROM product_variants WHERE id = ?1",
            [variant],
            |r| r.get(0),
        )
        .unwrap()
}

fn account_balance(kit: &Kit, account: &str) -> i64 {
    kit.conn()
        .query_row(
            "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines WHERE account_id = ?1",
            [account],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn order_receive_in_parts_then_bill_against_the_receipt() {
    let kit = Kit::new();
    let gst = kit.gst_rate(5);
    let dal = kit.product("Toor Dal", "DAL-1", 15_000, 10_000, 10, Some(&gst));
    let supplier = kit.party("Metro Wholesale", "supplier", None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let order = purchases::save_order(
        &mut kit.ctx(&mut conn, &owner),
        &PurchaseOrderInput {
            id: None,
            party_id: supplier.clone(),
            warehouse_id: None,
            expected_date: None,
            lines: vec![pline(&dal, 20, 12_000)],
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(
        (order.doc_no.as_str(), order.total),
        ("PO/26-27/00001", 252_000)
    );
    let po_item = order.lines[0].id.clone();
    let receipt = purchases::receive_goods(
        &mut kit.ctx(&mut conn, &owner),
        &GoodsReceiptInput {
            idempotency_key: "grn-000001".into(),
            party_id: supplier.clone(),
            warehouse_id: None,
            purchase_order_id: Some(order.id.clone()),
            supplier_reference: Some("DC-77".into()),
            lines: vec![PurchaseLineInput {
                po_item_id: Some(po_item),
                batch_no: Some("B-1".into()),
                expiry_date: Some("2027-06-30".into()),
                ..pline(&dal, 10, 12_000)
            }],
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(receipt.value, 120_000);
    assert_eq!(
        purchases::get_order(
            &kit.ctx(&mut conn, &owner),
            &IdInput {
                id: order.id.clone()
            }
        )
        .unwrap()
        .status,
        "partially_received"
    );
    assert_eq!(kit.stock(&dal), 20_000);
    assert_eq!(
        cost_of(&kit, &dal),
        11_000,
        "weighted average of 10 @ 100 and 10 @ 120"
    );

    let mut against_receipt = bill("bill-000001", &supplier, vec![pline(&dal, 10, 12_500)]);
    against_receipt.goods_receipt_id = Some(receipt.id.clone());
    against_receipt.supplier_invoice_no = Some("MW/4411".into());
    let b = purchases::create_bill(&mut kit.ctx(&mut conn, &owner), &against_receipt).unwrap();
    assert_eq!(b.total, 131_300);
    assert_eq!(
        kit.stock(&dal),
        20_000,
        "billing a receipt doesn't receive the goods again"
    );
    assert_eq!(
        account_balance(&kit, accounts::GOODS_RECEIVED_NOT_INVOICED),
        0,
        "receipt cleared"
    );
    assert_eq!(kit.balance(&supplier), -131_300);
    let mut duplicate = bill("bill-000002", &supplier, vec![pline(&dal, 1, 100)]);
    duplicate.supplier_invoice_no = Some("mw/4411".into());
    assert_eq!(
        code(purchases::create_bill(
            &mut kit.ctx(&mut conn, &owner),
            &duplicate
        )),
        ErrorCode::Conflict
    );
    let mut twice = bill("bill-000003", &supplier, vec![pline(&dal, 10, 12_500)]);
    twice.goods_receipt_id = Some(receipt.id);
    assert_eq!(
        code(purchases::create_bill(
            &mut kit.ctx(&mut conn, &owner),
            &twice
        )),
        ErrorCode::Conflict
    );
    kit.assert_books_balance();
}

#[test]
fn landed_costs_raise_stock_cost_and_bills_can_be_paid_and_voided() {
    let kit = Kit::new();
    let rice = kit.product("Rice", "RICE-1", 6_000, 4_000, 0, None);
    let oil = kit.product("Oil", "OIL-1", 20_000, 15_000, 0, None);
    let supplier = kit.party("Agro Traders", "supplier", None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut input = bill(
        "bill-landed-1",
        &supplier,
        vec![pline(&rice, 100, 4_000), pline(&oil, 10, 16_000)],
    );
    input.charges = vec![ChargeInput {
        description: "Freight".into(),
        amount: 5_600,
        allocation: "value".into(),
    }];
    input.payments = vec![PaymentInput {
        mode: "bank_transfer".into(),
        amount: 200_000,
        tendered: None,
        account_id: None,
        reference: Some("NEFT1".into()),
    }];
    let b = purchases::create_bill(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    assert_eq!(
        (b.taxable, b.charges_total, b.total, b.amount_paid),
        (560_000, 5_600, 565_600, 200_000)
    );
    // Freight split 400_000 : 160_000 → 4_000 + 1_600.
    assert_eq!((cost_of(&kit, &rice), cost_of(&kit, &oil)), (4_040, 16_160));
    assert_eq!(kit.balance(&supplier), -365_600);
    kit.assert_books_balance();

    let keeper = kit.user("stock_keeper");
    let v = VoidBillInput {
        id: b.id.clone(),
        reason: "Entered twice".into(),
    };
    assert_eq!(
        code(purchases::void_bill(&mut kit.ctx(&mut conn, &keeper), &v)),
        ErrorCode::Forbidden
    );
    let voided = purchases::void_bill(&mut kit.ctx(&mut conn, &owner), &v).unwrap();
    assert_eq!(voided.status, "voided");
    assert_eq!((kit.stock(&rice), kit.balance(&supplier)), (0, 0));
    kit.assert_books_balance();
}

#[test]
fn purchase_returns_and_supplier_payments() {
    let kit = Kit::new();
    let rice = kit.product("Rice", "RICE-1", 6_000, 4_000, 0, None);
    let supplier = kit.party("Agro Traders", "supplier", None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let b = purchases::create_bill(
        &mut kit.ctx(&mut conn, &owner),
        &bill("bill-ret-001", &supplier, vec![pline(&rice, 50, 4_000)]),
    )
    .unwrap();
    let item = b.lines[0].id.clone();
    let ret = purchases::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &PurchaseReturnInput {
            idempotency_key: "pret-000001".into(),
            original_bill_id: Some(b.id.clone()),
            party_id: supplier.clone(),
            warehouse_id: None,
            reason: "Wet bags".into(),
            lines: vec![PurchaseReturnLineInput {
                original_item_id: Some(item.clone()),
                variant_id: None,
                qty: 5_000,
                unit_cost: None,
            }],
            refund: None,
        },
    )
    .unwrap();
    assert_eq!((ret.doc_no.as_str(), ret.total), ("DN/26-27/00001", 20_000));
    assert_eq!(
        (kit.stock(&rice), kit.balance(&supplier)),
        (45_000, -180_000)
    );
    let too_many = PurchaseReturnInput {
        idempotency_key: "pret-000002".into(),
        original_bill_id: Some(b.id.clone()),
        party_id: supplier.clone(),
        warehouse_id: None,
        reason: "x".into(),
        lines: vec![PurchaseReturnLineInput {
            original_item_id: Some(item),
            variant_id: None,
            qty: 46_000,
            unit_cost: None,
        }],
        refund: None,
    };
    assert_eq!(
        code(purchases::create_return(
            &mut kit.ctx(&mut conn, &owner),
            &too_many
        )),
        ErrorCode::Validation
    );

    let paid = payments::record(
        &mut kit.ctx(&mut conn, &owner),
        &PaymentEntryInput {
            idempotency_key: "pay-supp-01".into(),
            party_id: supplier.clone(),
            direction: "out".into(),
            payment: PaymentInput {
                mode: "cheque".into(),
                amount: 180_000,
                tendered: None,
                account_id: None,
                reference: Some("CHQ 1001".into()),
            },
            business_date: None,
            allocations: vec![],
            auto_allocate: true,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(paid.unallocated, 0);
    assert_eq!(kit.balance(&supplier), 0);
    kit.assert_books_balance();
}

#[test]
fn expenses_paid_now_or_on_credit() {
    let kit = Kit::new();
    let owner = kit.owner();
    let landlord = kit.party("Landlord", "supplier", None);
    let mut conn = kit.conn();
    let rent: String = conn
        .query_row(
            "SELECT id FROM expense_categories WHERE name = 'Rent'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let base = ExpenseInput {
        idempotency_key: "exp-000001".into(),
        category_id: rent.clone(),
        title: "October rent".into(),
        description: None,
        reference_no: None,
        amount: 2_500_000,
        tax_rate: 180_000,
        business_date: None,
        party_id: None,
        payment: None,
    };
    assert_eq!(
        code(expenses::create(&mut kit.ctx(&mut conn, &owner), &base)),
        ErrorCode::Validation,
        "unpaid without a supplier"
    );
    let on_credit = expenses::create(
        &mut kit.ctx(&mut conn, &owner),
        &ExpenseInput {
            party_id: Some(landlord.clone()),
            ..base.clone()
        },
    )
    .unwrap();
    assert_eq!(
        (on_credit.total, kit.balance(&landlord)),
        (2_950_000, -2_950_000)
    );
    let paid_now = expenses::create(
        &mut kit.ctx(&mut conn, &owner),
        &ExpenseInput {
            idempotency_key: "exp-000002".into(),
            tax_rate: 0,
            amount: 50_000,
            title: "Tea".into(),
            payment: Some(PaymentInput {
                mode: "cash".into(),
                amount: 50_000,
                tendered: None,
                account_id: None,
                reference: None,
            }),
            ..base.clone()
        },
    )
    .unwrap();
    assert_eq!(paid_now.amount_paid, 50_000);
    expenses::void(
        &mut kit.ctx(&mut conn, &owner),
        &VoidExpenseInput {
            id: on_credit.id,
            reason: "Duplicate".into(),
        },
    )
    .unwrap();
    assert_eq!(kit.balance(&landlord), 0);
    let cashier = kit.user("cashier");
    assert_eq!(
        code(expenses::list(
            &kit.ctx(&mut conn, &cashier),
            &Default::default()
        )),
        ErrorCode::Forbidden
    );
    assert_eq!(
        expenses::list(&kit.ctx(&mut conn, &owner), &Default::default())
            .unwrap()
            .total,
        2
    );
    kit.assert_books_balance();
}
