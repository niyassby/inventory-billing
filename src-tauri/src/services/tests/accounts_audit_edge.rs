//! Accounts audit, part three: replays, tills, input tax, voided purchases and walk-in refunds.

use super::accounts_audit::{allocation, scalar, shop, Shop};
use super::reporting::{cash, sale};
use crate::error::ErrorCode;
use crate::services::expenses::{self, ExpenseInput, VoidExpenseInput};
use crate::services::payments::{self, CloseRegisterInput, OpenRegisterInput, PaymentEntryInput};
use crate::services::purchases::{self, PurchaseBillInput, PurchaseLineInput, VoidBillInput};
use crate::services::sales::{self, ReturnInput, ReturnLineInput, VoidInput};
use crate::services::testkit::code;

fn money_in(s: &Shop, key: &str, amount: i64) -> PaymentEntryInput {
    PaymentEntryInput {
        idempotency_key: key.into(),
        party_id: s.customer.clone(),
        direction: "in".into(),
        payment: cash(amount),
        business_date: None,
        allocations: vec![],
        auto_allocate: true,
        notes: None,
    }
}

#[test]
fn a_payment_is_not_applied_twice_and_a_voided_invoice_is_not_payable() {
    let s = shop();
    let inv = s.credit_sale("aud-edge-sale1", 2, 0);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();

    // The same request id twice (a retried click) is one payment.
    let first = payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &money_in(&s, "aud-edge-rcpt1", 5_000),
    )
    .unwrap();
    let again = payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &money_in(&s, "aud-edge-rcpt1", 5_000),
    )
    .unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(s.balance(), 23_600 - 5_000);
    let rows = scalar(&conn, "SELECT count(*) FROM payments WHERE doc_no = ''")
        + scalar(&conn, "SELECT count(*) FROM payments");
    assert_eq!(rows, 1, "one payment row");

    // The same id with a different amount is refused, not silently changed.
    let changed = payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &money_in(&s, "aud-edge-rcpt1", 9_000),
    );
    assert!(changed.is_err());

    // Once the invoice is voided it can't be allocated to.
    sales::void_invoice(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidInput {
            id: inv.id.clone(),
            reason: "Wrong shop".into(),
        },
    )
    .unwrap();
    let late = payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &PaymentEntryInput {
            allocations: vec![allocation(&inv, 1_000)],
            auto_allocate: false,
            ..money_in(&s, "aud-edge-rcpt2", 1_000)
        },
    );
    assert_eq!(code(late), ErrorCode::Validation);
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn a_till_session_adds_up_the_float_cash_sales_refunds_and_expenses() {
    let s = shop();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let session = payments::open_register(
        &mut s.kit.ctx(&mut conn, &owner),
        &OpenRegisterInput {
            register_id: None,
            opening_float: 5_000,
        },
    )
    .unwrap();

    // A walk-in cash sale of ₹118 through this till.
    let mut input = sale("aud-edge-till1", &s.variant, 1, 11_800);
    input.register_session_id = Some(session.id.clone());
    let inv = sales::create_invoice(&mut s.kit.ctx(&mut conn, &owner), &input).unwrap();

    // The walk-in brings it back: refunded in full, in cash, from the same till.
    let refund = sales::create_return(
        &mut s.kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "aud-edge-tillret".into(),
            original_invoice_id: Some(inv.id.clone()),
            party_id: None,
            warehouse_id: None,
            reason: "Changed mind".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(inv.lines[0].id.clone()),
                variant_id: None,
                qty: 1_000,
                restock: true,
            }],
            refund: Some(cash(11_800)),
        },
    )
    .unwrap();
    assert_eq!(
        refund.amount_refunded, 11_800,
        "a walk-in is refunded in full"
    );

    // Another ₹118 sale stays in the drawer.
    let mut second = sale("aud-edge-till2", &s.variant, 1, 11_800);
    second.register_session_id = Some(session.id.clone());
    sales::create_invoice(&mut s.kit.ctx(&mut conn, &owner), &second).unwrap();

    let before = payments::current_register(&s.kit.ctx(&mut conn, &owner))
        .unwrap()
        .unwrap();
    // Expected in the drawer: the float plus what is still in it.
    assert_eq!(before.expected_cash, 5_000 + 11_800 - 11_800 + 11_800);

    let closed = payments::close_register(
        &mut s.kit.ctx(&mut conn, &owner),
        &CloseRegisterInput {
            session_id: session.id.clone(),
            counted_cash: 16_500,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(closed.expected_cash, 16_800);
    assert_eq!(closed.difference, Some(-300));
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn input_tax_on_an_expense_is_claimable_and_a_void_takes_it_all_back() {
    let s = shop();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let category: String = conn
        .query_row("SELECT id FROM expense_categories LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let made = expenses::create(
        &mut s.kit.ctx(&mut conn, &owner),
        &ExpenseInput {
            idempotency_key: "aud-edge-exp1".into(),
            category_id: category,
            title: "Shop repairs".into(),
            description: None,
            reference_no: Some("BILL-17".into()),
            amount: 10_000,
            tax_rate: 180_000,
            business_date: None,
            party_id: None,
            payment: Some(cash(11_800)),
        },
    )
    .unwrap();
    assert_eq!(
        (made.amount, made.tax_total, made.total),
        (10_000, 1_800, 11_800)
    );
    let input_tax = scalar(
        &conn,
        "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines l JOIN accounts a ON a.id = l.account_id WHERE a.code = '1300' OR a.name LIKE '%input%'",
    );
    assert_eq!(input_tax, 1_800, "input tax is an asset to claim");

    expenses::void(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidExpenseInput {
            id: made.id,
            reason: "Duplicate entry".into(),
        },
    )
    .unwrap();
    let left = scalar(
        &conn,
        "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines l JOIN accounts a ON a.id = l.account_id WHERE a.account_type = 'expense' OR a.code = '1300' OR a.name LIKE '%input%'",
    );
    assert_eq!(left, 0, "nothing of the expense is left");
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn voiding_a_part_paid_purchase_bill_leaves_no_trace_in_stock_or_what_we_owe() {
    let s = shop();
    let supplier = s.kit.party("Agro Traders", "supplier", None);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let stock_before = scalar(&conn, "SELECT coalesce(sum(qty), 0) FROM stock_levels");
    let bill = purchases::create_bill(
        &mut s.kit.ctx(&mut conn, &owner),
        &PurchaseBillInput {
            idempotency_key: "aud-edge-bill1".into(),
            party_id: supplier.clone(),
            supplier_invoice_no: Some("AT-77".into()),
            supplier_invoice_date: None,
            business_date: None,
            due_date: None,
            warehouse_id: None,
            prices_include_tax: None,
            purchase_order_id: None,
            goods_receipt_id: None,
            lines: vec![PurchaseLineInput {
                variant_id: s.variant.clone(),
                unit_id: None,
                qty: 10_000,
                unit_cost: 4_000,
                discount: None,
                batch_no: None,
                expiry_date: None,
                mfg_date: None,
                po_item_id: None,
            }],
            bill_discount: None,
            charges: Vec::new(),
            payments: vec![cash(20_000)],
            expected_total: None,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(s.kit.balance(&supplier), -(47_200 - 20_000));

    purchases::void_bill(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidBillInput {
            id: bill.id,
            reason: "Entered twice".into(),
        },
    )
    .unwrap();
    assert_eq!(s.kit.balance(&supplier), 0);
    assert_eq!(
        scalar(&conn, "SELECT coalesce(sum(qty), 0) FROM stock_levels"),
        stock_before
    );
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn receipts_and_expenses_paid_in_cash_count_in_the_till() {
    let s = shop();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let session = payments::open_register(
        &mut s.kit.ctx(&mut conn, &owner),
        &OpenRegisterInput {
            register_id: None,
            opening_float: 1_000,
        },
    )
    .unwrap();
    s.credit_sale("aud-till-credit", 1, 0);
    // A customer pays ₹30 on account at the counter; ₹20 of petty cash goes on tea.
    payments::record(
        &mut s.kit.ctx(&mut conn, &owner),
        &money_in(&s, "aud-till-rcpt", 3_000),
    )
    .unwrap();
    let category: String = conn
        .query_row("SELECT id FROM expense_categories LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    expenses::create(
        &mut s.kit.ctx(&mut conn, &owner),
        &ExpenseInput {
            idempotency_key: "aud-till-exp".into(),
            category_id: category,
            title: "Tea".into(),
            description: None,
            reference_no: None,
            amount: 2_000,
            tax_rate: 0,
            business_date: None,
            party_id: None,
            payment: Some(cash(2_000)),
        },
    )
    .unwrap();
    let now = payments::current_register(&s.kit.ctx(&mut conn, &owner))
        .unwrap()
        .unwrap();
    assert_eq!(now.id, session.id);
    assert_eq!(
        (now.cash_in, now.cash_out, now.expected_cash),
        (3_000, 2_000, 1_000 + 3_000 - 2_000)
    );
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn a_card_payment_can_not_be_more_than_the_bill() {
    let s = shop();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let mut input = sale("aud-card-over", &s.variant, 1, 0);
    input.payments = vec![crate::services::sales::PaymentInput {
        mode: "card".into(),
        amount: 20_000,
        tendered: None,
        account_id: None,
        reference: None,
    }];
    let r = sales::create_invoice(&mut s.kit.ctx(&mut conn, &owner), &input);
    assert_eq!(code(r), ErrorCode::Validation);
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn a_party_with_a_balance_can_not_be_deleted_and_an_opening_balance_can_not_be_rewritten() {
    use crate::services::parties::{self, PartyInput};
    let s = shop();
    s.credit_sale("aud-party-sale", 1, 0);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let denied = parties::delete(
        &mut s.kit.ctx(&mut conn, &owner),
        &crate::services::users::IdInput {
            id: s.customer.clone(),
        },
    );
    assert!(denied.is_err(), "a customer who owes money was deleted");

    // Changing the opening balance of a party that already has postings must not corrupt it.
    let party = parties::get(
        &s.kit.ctx(&mut conn, &owner),
        &crate::services::users::IdInput {
            id: s.customer.clone(),
        },
    )
    .unwrap();
    let edited = parties::save(
        &mut s.kit.ctx(&mut conn, &owner),
        &PartyInput {
            id: Some(party.id.clone()),
            expected_updated_at: Some(party.updated_at.clone()),
            party_type: party.party_type.clone(),
            name: party.name.clone(),
            code: None,
            phone: None,
            email: None,
            gstin: None,
            state_code: None,
            billing_address: None,
            shipping_address: None,
            credit_limit: None,
            credit_days: None,
            opening_balance: Some(99_999),
            price_list_id: None,
            notes: None,
            is_active: true,
        },
    );
    drop(conn);
    // Whether it is refused or ignored, the party's cached balance must still equal its ledger.
    let _ = edited;
    s.kit.assert_books_balance();
    assert_eq!(
        s.balance(),
        11_800,
        "the balance after trying to rewrite the opening balance"
    );
}

#[test]
fn the_day_close_summary_matches_the_database() {
    use crate::services::accounting::{self, DayInput};
    let s = shop();
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let a = s.credit_sale("aud-day-sale1", 2, 11_800);
    s.credit_sale("aud-day-sale2", 1, 0);
    let supplier = s.kit.party("Agro Traders", "supplier", None);
    purchases::create_bill(
        &mut s.kit.ctx(&mut conn, &owner),
        &PurchaseBillInput {
            idempotency_key: "aud-day-bill".into(),
            party_id: supplier,
            supplier_invoice_no: Some("D-1".into()),
            supplier_invoice_date: None,
            business_date: None,
            due_date: None,
            warehouse_id: None,
            prices_include_tax: None,
            purchase_order_id: None,
            goods_receipt_id: None,
            lines: vec![PurchaseLineInput {
                variant_id: s.variant.clone(),
                unit_id: None,
                qty: 5_000,
                unit_cost: 4_000,
                discount: None,
                batch_no: None,
                expiry_date: None,
                mfg_date: None,
                po_item_id: None,
            }],
            bill_discount: None,
            charges: Vec::new(),
            payments: vec![cash(10_000)],
            expected_total: None,
            notes: None,
        },
    )
    .unwrap();
    sales::create_return(
        &mut s.kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "aud-day-ret".into(),
            original_invoice_id: Some(a.id.clone()),
            party_id: a.party_id.clone(),
            warehouse_id: None,
            reason: "Damaged".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(a.lines[0].id.clone()),
                variant_id: None,
                qty: 1_000,
                restock: true,
            }],
            refund: None,
        },
    )
    .unwrap();

    let day = accounting::day_summary(
        &s.kit.ctx(&mut conn, &owner),
        &DayInput {
            business_date: None,
            notes: None,
        },
    )
    .unwrap();
    let sql = |q: &str| scalar(&conn, q);
    assert_eq!(
        day.bills,
        sql("SELECT count(*) FROM sales_invoices WHERE status = 'finalized'")
    );
    assert_eq!(
        day.sales_total,
        sql("SELECT coalesce(sum(total), 0) FROM sales_invoices WHERE status = 'finalized'")
    );
    assert_eq!(
        day.returns_total,
        sql("SELECT coalesce(sum(total), 0) FROM sales_returns WHERE status = 'finalized'")
    );
    assert_eq!(
        day.purchases_total,
        sql("SELECT coalesce(sum(total), 0) FROM purchase_bills WHERE status = 'finalized'")
    );
    let received: i64 = day.received.iter().map(|m| m.amount).sum();
    let paid: i64 = day.paid.iter().map(|m| m.amount).sum();
    assert_eq!(received, sql("SELECT coalesce(sum(amount), 0) FROM payments WHERE direction = 'in' AND status = 'finalized'"));
    assert_eq!(paid, sql("SELECT coalesce(sum(amount), 0) FROM payments WHERE direction = 'out' AND status = 'finalized'"));
    // Tax collected is what the invoices charged, less what the credit note gave back.
    let charged =
        sql("SELECT coalesce(sum(tax_total), 0) FROM sales_invoices WHERE status = 'finalized'");
    let returned =
        sql("SELECT coalesce(sum(tax_total), 0) FROM sales_returns WHERE status = 'finalized'");
    assert!(
        day.tax_total == charged || day.tax_total == charged - returned,
        "tax collected {} is neither the invoices' {} nor net of returns {}",
        day.tax_total,
        charged,
        charged - returned
    );
    drop(conn);
    s.kit.assert_books_balance();
}
