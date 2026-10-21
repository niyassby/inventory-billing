//! Accounts audit, part two: returns, purchases, expenses, discounts, rounding, and the figures
//! the reports show against the database.

use serde_json::Value;

use super::accounts_audit::{scalar, shop, Shop};
use super::reporting::{cash, sale};
use crate::domain::pricing::Discount;
use crate::error::ErrorCode;
use crate::services::expenses::{self, ExpenseInput};
use crate::services::payments::{self, PaymentEntryInput};
use crate::services::purchases::{
    self, PurchaseBillInput, PurchaseDocDetail, PurchaseLineInput, PurchaseReturnInput,
    PurchaseReturnLineInput,
};
use crate::services::reports::{self, ReportQuery, ReportTable};
use crate::services::sales::{
    self, InvoiceDetail, PaymentInput, ReturnDetail, ReturnInput, ReturnLineInput, VoidInput,
};
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

fn return_of(
    s: &Shop,
    key: &str,
    invoice: &InvoiceDetail,
    units: i64,
    refund: Option<PaymentInput>,
) -> Result<ReturnDetail, crate::error::AppError> {
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    sales::create_return(
        &mut s.kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: key.into(),
            original_invoice_id: Some(invoice.id.clone()),
            party_id: invoice.party_id.clone(),
            warehouse_id: None,
            reason: "Damaged".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(invoice.lines[0].id.clone()),
                variant_id: None,
                qty: units * 1000,
                restock: true,
            }],
            refund,
        },
    )
}

fn report(kit: &Kit, name: &str) -> ReportTable {
    let owner = kit.owner();
    let mut conn = kit.conn();
    reports::run(
        &kit.ctx(&mut conn, &owner),
        &ReportQuery {
            report: name.into(),
            from: Some("2026-10-01".into()),
            to: Some("2026-10-31".into()),
            ..Default::default()
        },
    )
    .unwrap()
}

fn total_of(table: &ReportTable, key: &str) -> i64 {
    let i = table.columns.iter().position(|c| c.key == key).unwrap();
    table
        .totals
        .as_ref()
        .map_or(0, |t| t[i].as_i64().unwrap_or(0))
}

fn bill(
    kit: &Kit,
    key: &str,
    supplier: &str,
    variant: &str,
    units: i64,
    cost: i64,
    paid: i64,
) -> PurchaseDocDetail {
    let owner = kit.owner();
    let mut conn = kit.conn();
    purchases::create_bill(
        &mut kit.ctx(&mut conn, &owner),
        &PurchaseBillInput {
            idempotency_key: key.into(),
            party_id: supplier.into(),
            supplier_invoice_no: Some(format!("SUP-{key}")),
            supplier_invoice_date: None,
            business_date: None,
            due_date: None,
            warehouse_id: None,
            prices_include_tax: None,
            purchase_order_id: None,
            goods_receipt_id: None,
            lines: vec![PurchaseLineInput {
                variant_id: variant.into(),
                unit_id: None,
                qty: units * 1000,
                unit_cost: cost,
                discount: None,
                batch_no: None,
                expiry_date: None,
                mfg_date: None,
                po_item_id: None,
            }],
            bill_discount: None,
            charges: Vec::new(),
            payments: if paid > 0 {
                vec![cash(paid)]
            } else {
                Vec::new()
            },
            expected_total: None,
            notes: None,
        },
    )
    .unwrap()
}

#[test]
fn a_customer_who_is_also_a_supplier_keeps_the_control_accounts_in_step() {
    let s = shop();
    let both = s.kit.party("Ravi Traders", "both", None);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    // We sell them goods on credit (they owe us ₹118) ...
    let mut input = sale("aud-both-sale", &s.variant, 1, 0);
    input.party_id = Some(both.clone());
    input.payments = Vec::new();
    sales::create_invoice(&mut s.kit.ctx(&mut conn, &owner), &input).unwrap();
    drop(conn);
    // ... and buy goods from them on credit (we owe them ₹59).
    bill(&s.kit, "aud-both-bill", &both, &s.variant, 1, 5_000, 0);
    s.kit.assert_books_balance();
}

#[test]
fn returns_reduce_what_is_owed_and_can_not_exceed_what_was_sold() {
    let s = shop();
    let inv = s.credit_sale("aud-ret-sale", 2, 0);
    assert_eq!(s.balance(), 23_600);

    // One unit back as credit on the account (no refund): the customer owes less.
    let first = return_of(&s, "aud-ret-0001", &inv, 1, None).unwrap();
    assert_eq!(first.total, 11_800);
    assert_eq!(s.balance(), 11_800);
    s.kit.assert_books_balance();

    // Only one unit is left to return.
    assert_eq!(
        code(return_of(&s, "aud-ret-0002", &inv, 2, None)),
        ErrorCode::Validation
    );
    assert_eq!(
        code(return_of(&s, "aud-ret-0003", &inv, 0, None)),
        ErrorCode::Validation
    );
    assert_eq!(
        code(return_of(&s, "aud-ret-0004", &inv, -1, None)),
        ErrorCode::Validation
    );

    return_of(&s, "aud-ret-0005", &inv, 1, None).unwrap();
    assert_eq!(s.balance(), 0);
    assert_eq!(
        scalar(
            &s.kit.conn(),
            "SELECT coalesce(sum(qty), 0) FROM stock_levels"
        ),
        100_000
    );
    s.kit.assert_books_balance();
}

#[test]
fn voiding_an_invoice_that_had_a_return_does_not_give_the_stock_back_twice() {
    let s = shop();
    let inv = s.credit_sale("aud-void-ret", 2, 0);
    return_of(&s, "aud-void-ret-1", &inv, 1, None).unwrap();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let voided = sales::void_invoice(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidInput {
            id: inv.id.clone(),
            reason: "Mistake".into(),
        },
    );
    drop(conn);
    // Refused (cancel the return first) or done correctly: never done twice over.
    if voided.is_ok() {
        assert_eq!(
            scalar(
                &s.kit.conn(),
                "SELECT coalesce(sum(qty), 0) FROM stock_levels"
            ),
            100_000,
            "stock after voiding an invoice that had a return"
        );
        assert_eq!(
            s.balance(),
            0,
            "balance after voiding an invoice that had a return"
        );
        assert!(return_of(&s, "aud-void-ret-2", &inv, 1, None).is_err());
    }
    s.kit.assert_books_balance();
}

#[test]
fn purchases_payments_and_returns_keep_what_we_owe_exact() {
    let s = shop();
    let supplier = s.kit.party("Agro Traders", "supplier", None);
    let b = bill(&s.kit, "aud-bill-0001", &supplier, &s.variant, 10, 4_000, 0);
    assert_eq!(b.total, 47_200); // 10 × ₹40 + 18 % GST
    assert_eq!(s.kit.balance(&supplier), -47_200);

    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let paid = payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &PaymentEntryInput {
            idempotency_key: "aud-pay-0001".into(),
            party_id: supplier.clone(),
            direction: "out".into(),
            payment: cash(20_000),
            business_date: None,
            allocations: vec![],
            auto_allocate: true,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(paid.unallocated, 0);
    assert_eq!(s.kit.balance(&supplier), -27_200);

    // Two units go back: the debit note reduces what we owe.
    let line = purchases::get_bill(&s.kit.ctx(&mut conn, &owner), &IdInput { id: b.id.clone() })
        .unwrap()
        .lines[0]
        .id
        .clone();
    purchases::create_return(
        &mut s.kit.ctx(&mut conn, &owner),
        &PurchaseReturnInput {
            idempotency_key: "aud-pret-0001".into(),
            original_bill_id: Some(b.id.clone()),
            party_id: supplier.clone(),
            warehouse_id: None,
            reason: "Wrong brand".into(),
            lines: vec![PurchaseReturnLineInput {
                original_item_id: Some(line),
                variant_id: None,
                qty: 2_000,
                unit_cost: None,
            }],
            refund: None,
        },
    )
    .unwrap();
    assert_eq!(s.kit.balance(&supplier), -27_200 + 9_440);
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn an_expense_on_credit_is_owed_then_paid_and_zero_is_refused() {
    let s = shop();
    let supplier = s.kit.party("Landlord", "supplier", None);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let category: String = conn
        .query_row("SELECT id FROM expense_categories LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let make = |key: &str, amount: i64| ExpenseInput {
        idempotency_key: key.into(),
        category_id: category.clone(),
        title: "Rent".into(),
        description: None,
        reference_no: None,
        amount,
        tax_rate: 0,
        business_date: None,
        party_id: Some(supplier.clone()),
        payment: None,
    };
    for (n, amount) in [0, -500].into_iter().enumerate() {
        let r = expenses::create(
            &mut s.kit.ctx(&mut conn, &owner),
            &make(&format!("aud-exp-bad{n}"), amount),
        );
        assert_eq!(code(r), ErrorCode::Validation, "expense of {amount}");
    }
    let e = expenses::create(
        &mut s.kit.ctx(&mut conn, &owner),
        &make("aud-exp-0001", 8_000),
    )
    .unwrap();
    assert_eq!((e.total, s.kit.balance(&supplier)), (8_000, -8_000));
    payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &PaymentEntryInput {
            idempotency_key: "aud-pay-exp".into(),
            party_id: supplier.clone(),
            direction: "out".into(),
            payment: cash(8_000),
            business_date: None,
            allocations: vec![],
            auto_allocate: true,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(s.kit.balance(&supplier), 0);
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn discounts_can_not_make_a_bill_negative() {
    let s = shop();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    for (n, discount) in [
        Discount::Percent { rate: 1_500_000 },
        Discount::Percent { rate: -10_000 },
        Discount::Amount {
            amount: 999_999_999,
        },
        Discount::Amount { amount: -100 },
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = sale(&format!("aud-disc-{n:04}"), &s.variant, 1, 0);
        input.payments = Vec::new();
        input.party_id = Some(s.customer.clone());
        input.bill_discount = Some(discount);
        let r = sales::create_invoice(&mut s.kit.ctx(&mut conn, &owner), &input);
        assert!(r.is_err(), "discount #{n} was accepted");
    }
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn many_small_lines_with_different_tax_rates_add_up_exactly() {
    let kit = Kit::new();
    let (g5, g12, g18) = (kit.gst_rate(5), kit.gst_rate(12), kit.gst_rate(18));
    let variants = [
        kit.product("Salt", "AUD-1", 2_233, 1_000, 500, Some(&g5)),
        kit.product("Pen", "AUD-2", 1_949, 900, 500, Some(&g12)),
        kit.product("Soap", "AUD-3", 3_347, 1_500, 500, Some(&g18)),
        kit.product("Tea", "AUD-4", 12_999, 8_000, 500, Some(&g5)),
    ];
    let customer = kit.party("Rounding Stores", "customer", None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut input = sale("aud-round-0001", &variants[0], 1, 0);
    input.lines = variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let mut l = sale("x", v, 1, 0).lines.remove(0);
            l.qty = [333, 1_750, 2_999, 125][i];
            l
        })
        .collect();
    input.payments = Vec::new();
    input.party_id = Some(customer);
    input.bill_discount = Some(Discount::Percent { rate: 33_333 });
    let inv = sales::create_invoice(&mut kit.ctx(&mut conn, &owner), &input).unwrap();

    // The header is exactly what its lines say: no paisa appears or vanishes.
    let lines_tax: i64 = inv.lines.iter().map(|l| l.tax_total).sum();
    assert_eq!(inv.taxable + inv.tax_total + inv.round_off, inv.total);
    assert!(
        (inv.tax_total - lines_tax).abs() <= 1,
        "tax {} vs lines {}",
        inv.tax_total,
        lines_tax
    );
    drop(conn);
    kit.assert_books_balance();
}

#[test]
fn the_figures_on_the_screens_match_the_database() {
    let s = shop();
    let supplier = s.kit.party("Agro Traders", "supplier", None);
    bill(
        &s.kit,
        "aud-rep-bill",
        &supplier,
        &s.variant,
        20,
        4_000,
        20_000,
    );
    let a = s.credit_sale("aud-rep-sale1", 2, 5_000);
    s.credit_sale("aud-rep-sale2", 3, 35_400);
    return_of(&s, "aud-rep-ret1", &a, 1, None).unwrap();
    s.receive("aud-rep-rcpt", 4_000, vec![], true).unwrap();

    let conn = s.kit.conn();
    let receivables = report(&s.kit, "party_balances");
    assert_eq!(
        total_of(&receivables, "balance"),
        scalar(
            &conn,
            "SELECT coalesce(sum(balance), 0) FROM parties WHERE party_type IN ('customer', 'both') AND balance > 0"
        ),
        "receivables report vs customer balances"
    );
    let summary = report(&s.kit, "sales_summary");
    assert_eq!(
        total_of(&summary, "total"),
        scalar(
            &conn,
            "SELECT coalesce(sum(total), 0) FROM sales_invoices WHERE status = 'finalized'"
        ),
        "sales summary vs invoices"
    );
    let valuation = report(&s.kit, "stock_valuation");
    assert_eq!(
        total_of(&valuation, "value"),
        scalar(&conn, "SELECT coalesce(sum(value), 0) FROM stock_levels"),
        "stock valuation vs stock levels"
    );
    let trial = report(&s.kit, "trial_balance");
    assert_eq!(total_of(&trial, "debit"), total_of(&trial, "credit"));
    // The trial balance lists each account's net balance on one side.
    assert_eq!(
        total_of(&trial, "debit"),
        scalar(
            &conn,
            "SELECT coalesce(sum(CASE WHEN net > 0 THEN net ELSE 0 END), 0) FROM (SELECT sum(debit - credit) AS net FROM journal_lines GROUP BY account_id)"
        ),
        "trial balance vs the journal"
    );
    let cash_book = report(&s.kit, "cash_book");
    let closing = cash_book
        .rows
        .last()
        .and_then(|r| r.last())
        .and_then(Value::as_i64)
        .unwrap_or(0);
    assert_eq!(
        closing,
        scalar(
            &conn,
            "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines l JOIN accounts a ON a.id = l.account_id WHERE a.subtype = 'cash'"
        ),
        "cash book closing vs the cash accounts"
    );
    drop(conn);
    s.kit.assert_books_balance();
}
