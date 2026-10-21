//! Audit of the accounts: every scenario ends by checking that the books still agree with
//! themselves (balanced journal, control accounts against party balances, inventory against
//! stock) and that the figures a screen would show match the database.

use rusqlite::Connection;

use super::reporting::{cash, sale};
use crate::error::ErrorCode;
use crate::services::payments::{
    self, AllocateInput, AllocationInput, PaymentEntryInput, VoidPaymentInput,
};
use crate::services::sales::{self, InvoiceDetail, VoidInput};
use crate::services::testkit::{code, Kit};

pub(super) struct Shop {
    pub(super) kit: Kit,
    pub(super) variant: String,
    pub(super) customer: String,
}

/// ₹100 + 18 % GST (₹118) rice, 100 in stock, and one customer without a credit limit.
pub(super) fn shop() -> Shop {
    let kit = Kit::new();
    let gst = kit.gst_rate(18);
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 100, Some(&gst));
    let customer = kit.party("Hotel Annapurna", "customer", None);
    Shop {
        kit,
        variant,
        customer,
    }
}

impl Shop {
    /// A sale to the customer; `paid` is taken at the till (0 = all on credit).
    pub(super) fn credit_sale(&self, key: &str, units: i64, paid: i64) -> InvoiceDetail {
        let owner = self.kit.owner();
        let mut conn = self.kit.conn();
        let mut input = sale(key, &self.variant, units, paid);
        input.party_id = Some(self.customer.clone());
        if paid == 0 {
            input.payments = Vec::new();
        }
        sales::create_invoice(&mut self.kit.ctx(&mut conn, &owner), &input).unwrap()
    }

    pub(super) fn receive(
        &self,
        key: &str,
        amount: i64,
        allocations: Vec<AllocationInput>,
        auto: bool,
    ) -> Result<payments::PaymentDetail, crate::error::AppError> {
        let owner = self.kit.owner();
        let mut conn = self.kit.conn();
        payments::record(
            &mut self.kit.ctx(&mut conn, &owner),
            &PaymentEntryInput {
                idempotency_key: key.into(),
                party_id: self.customer.clone(),
                direction: "in".into(),
                payment: cash(amount),
                business_date: None,
                allocations,
                auto_allocate: auto,
                notes: None,
            },
        )
    }

    pub(super) fn invoice(&self, id: &str) -> InvoiceDetail {
        sales::invoice_detail(&self.kit.conn(), id).unwrap()
    }

    pub(super) fn balance(&self) -> i64 {
        self.kit.balance(&self.customer)
    }
}

pub(super) fn allocation(invoice: &InvoiceDetail, amount: i64) -> AllocationInput {
    AllocationInput {
        target_type: "sales_invoice".into(),
        target_id: invoice.id.clone(),
        amount,
    }
}

pub(super) fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn part_payments_settle_an_invoice_step_by_step() {
    let s = shop();
    let inv = s.credit_sale("aud-sale-0001", 1, 0);
    assert_eq!(
        (inv.total, inv.balance_due, s.balance()),
        (11_800, 11_800, 11_800)
    );

    s.receive("aud-rcpt-0001", 3_000, vec![], true).unwrap();
    let after = s.invoice(&inv.id);
    assert_eq!(
        (
            after.amount_paid,
            after.balance_due,
            after.payment_status.as_str(),
            s.balance()
        ),
        (3_000, 8_800, "partial", 8_800)
    );

    s.receive("aud-rcpt-0002", 8_800, vec![], true).unwrap();
    let paid = s.invoice(&inv.id);
    assert_eq!(
        (paid.balance_due, paid.payment_status.as_str(), s.balance()),
        (0, "paid", 0)
    );
    s.kit.assert_books_balance();
}

#[test]
fn an_overpayment_becomes_an_advance_that_can_be_applied_later() {
    let s = shop();
    let first = s.credit_sale("aud-sale-0002", 1, 0);
    let receipt = s.receive("aud-rcpt-0003", 15_000, vec![], true).unwrap();
    assert_eq!((receipt.unallocated, s.balance()), (3_200, -3_200));
    assert_eq!(s.invoice(&first.id).payment_status, "paid");

    // The next sale is covered by the advance when it is applied.
    let second = s.credit_sale("aud-sale-0003", 1, 0);
    assert_eq!(s.balance(), 8_600);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    let applied = payments::allocate(
        &mut s.kit.ctx(&mut conn, &owner),
        &AllocateInput {
            payment_id: receipt.id.clone(),
            allocations: vec![allocation(&second, 3_200)],
        },
    )
    .unwrap();
    assert_eq!(applied.unallocated, 0);
    // Applying an advance changes who is paid, not what is owed in total.
    assert_eq!(s.balance(), 8_600);
    assert_eq!(s.invoice(&second.id).balance_due, 8_600);
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn one_document_can_not_be_paid_twice_in_the_same_request() {
    let s = shop();
    let inv = s.credit_sale("aud-sale-0004", 1, 0);
    let twice = s.receive(
        "aud-rcpt-0004",
        23_600,
        vec![allocation(&inv, 11_800), allocation(&inv, 11_800)],
        false,
    );
    assert_eq!(code(twice), ErrorCode::Validation);
    assert_eq!(s.invoice(&inv.id).balance_due, 11_800);
    s.kit.assert_books_balance();
}

#[test]
fn voiding_a_payment_puts_the_invoice_back() {
    let s = shop();
    let inv = s.credit_sale("aud-sale-0005", 1, 0);
    let receipt = s.receive("aud-rcpt-0005", 11_800, vec![], true).unwrap();
    assert_eq!(s.invoice(&inv.id).payment_status, "paid");

    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    payments::void(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidPaymentInput {
            id: receipt.id.clone(),
            reason: "Cheque bounced".into(),
        },
    )
    .unwrap();
    let back = s.invoice(&inv.id);
    assert_eq!(
        (back.balance_due, back.payment_status.as_str(), s.balance()),
        (11_800, "unpaid", 11_800)
    );
    let twice = payments::void(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidPaymentInput {
            id: receipt.id,
            reason: "again".into(),
        },
    );
    assert_eq!(code(twice), ErrorCode::Conflict);
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn zero_and_negative_amounts_are_refused() {
    let s = shop();
    let inv = s.credit_sale("aud-sale-0006", 1, 0);
    for (n, amount) in [0, -100].into_iter().enumerate() {
        let r = s.receive(&format!("aud-rcpt-neg{n}"), amount, vec![], true);
        assert_eq!(code(r), ErrorCode::Validation, "payment of {amount}");
    }
    let bad_allocation = s.receive("aud-rcpt-bad1", 100, vec![allocation(&inv, 0)], false);
    assert_eq!(code(bad_allocation), ErrorCode::Validation);
    let negative_allocation = s.receive("aud-rcpt-bad2", 100, vec![allocation(&inv, -5)], false);
    assert_eq!(code(negative_allocation), ErrorCode::Validation);
    let too_much = s.receive(
        "aud-rcpt-bad3",
        20_000,
        vec![allocation(&inv, 20_000)],
        false,
    );
    assert_eq!(code(too_much), ErrorCode::Validation);

    // A sale of nothing, or of a negative quantity, is refused too.
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    for (n, units) in [0, -1].into_iter().enumerate() {
        let mut input = sale(&format!("aud-sale-neg{n}"), &s.variant, 1, 0);
        input.lines[0].qty = units * 1000;
        input.payments = Vec::new();
        let r = sales::create_invoice(&mut s.kit.ctx(&mut conn, &owner), &input);
        assert_eq!(code(r), ErrorCode::Validation, "quantity {units}");
    }
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn posted_documents_can_not_be_edited_or_deleted_underneath_the_app() {
    let s = shop();
    let inv = s.credit_sale("aud-sale-0007", 1, 11_800);
    let conn = s.kit.conn();
    for (what, sql) in [
        (
            "change an invoice total",
            format!(
                "UPDATE sales_invoices SET total = 1 WHERE id = '{}'",
                inv.id
            ),
        ),
        (
            "delete an invoice",
            format!("DELETE FROM sales_invoices WHERE id = '{}'", inv.id),
        ),
        (
            "change a journal line",
            "UPDATE journal_lines SET debit = debit + 1".to_string(),
        ),
        (
            "delete a journal line",
            "DELETE FROM journal_lines".to_string(),
        ),
        (
            "delete a journal entry",
            "DELETE FROM journal_entries".to_string(),
        ),
        (
            "change a party ledger row",
            "UPDATE party_ledger SET debit = debit + 1".to_string(),
        ),
        (
            "delete a stock movement",
            "DELETE FROM stock_movements".to_string(),
        ),
        ("delete an audit entry", "DELETE FROM audit_log".to_string()),
    ] {
        assert!(
            conn.execute(&sql, []).is_err(),
            "the database allowed: {what}"
        );
    }
    drop(conn);
    s.kit.assert_books_balance();
}

#[test]
fn voiding_an_invoice_reverses_everything_it_posted() {
    let s = shop();
    let inv = s.credit_sale("aud-sale-0008", 2, 10_000);
    assert_eq!(s.balance(), 13_600);
    let owner = s.kit.owner();
    let mut conn = s.kit.conn();
    sales::void_invoice(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidInput {
            id: inv.id.clone(),
            reason: "Wrong customer".into(),
        },
    )
    .unwrap();
    assert_eq!(s.balance(), 0);
    assert_eq!(
        scalar(&conn, "SELECT coalesce(sum(qty), 0) FROM stock_levels"),
        100_000
    );
    // Nothing is left in the revenue and tax accounts.
    let net: i64 = scalar(
        &conn,
        "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines l JOIN accounts a ON a.id = l.account_id
         WHERE a.account_type IN ('income', 'liability') AND a.subtype NOT IN ('payable')",
    );
    assert_eq!(net, 0);
    let again = sales::void_invoice(
        &mut s.kit.ctx(&mut conn, &owner),
        &VoidInput {
            id: inv.id,
            reason: "again".into(),
        },
    );
    assert_eq!(code(again), ErrorCode::Conflict);
    drop(conn);
    s.kit.assert_books_balance();
}
