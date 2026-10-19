//! Sales: invoices, approvals, voids, returns, quotations, held bills, receipts, tills, concurrency.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use serde_json::json;

use crate::domain::pricing::Discount;
use crate::error::ErrorCode;
use crate::services::context::Actor;
use crate::services::payments::{
    self, AllocateInput, AllocationInput, CloseRegisterInput, OpenRegisterInput, PaymentEntryInput,
    VoidPaymentInput,
};
use crate::services::sales::{
    self, ConvertInput, HoldInput, InvoiceDetail, PaymentInput, ReturnInput, ReturnLineInput,
    SaleInput, SaleLineInput, SalesDocInput, VoidInput,
};
use crate::services::settings::{self, UpdateSettingsInput};
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

fn line(variant: &str, units: i64) -> SaleLineInput {
    SaleLineInput {
        variant_id: variant.into(),
        unit_id: None,
        qty: units * 1000,
        unit_price: None,
        discount: None,
        batch_id: None,
    }
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

fn sale(key: &str, lines: Vec<SaleLineInput>, payments: Vec<PaymentInput>) -> SaleInput {
    SaleInput {
        idempotency_key: key.into(),
        warehouse_id: None,
        party_id: None,
        customer_name: None,
        customer_phone: None,
        customer_gstin: None,
        place_of_supply: None,
        business_date: None,
        prices_include_tax: None,
        lines,
        bill_discount: None,
        payments,
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

/// Kit with one 18% GST product: ₹100.00 (exclusive), cost ₹70.00, 10 in stock.
fn shop() -> (Kit, String) {
    let kit = Kit::new();
    let gst = kit.gst_rate(18);
    let rice = kit.product("Basmati Rice", "RICE-1", 10_000, 7_000, 10, Some(&gst));
    (kit, rice)
}

#[test]
fn cash_sale_with_gst_and_change() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier");
    let mut input = sale(
        "sale-000001",
        vec![line(&rice, 2)],
        vec![PaymentInput {
            tendered: Some(25_000),
            ..cash(23_600)
        }],
    );
    input.expected_total = Some(23_600);
    let invoice = sell(&kit, &cashier, &input).unwrap();
    assert_eq!(invoice.doc_no, "INV/26-27/00001");
    assert_eq!(
        (invoice.taxable, invoice.tax_total, invoice.total),
        (20_000, 3_600, 23_600)
    );
    assert_eq!(
        (
            invoice.lines[0].cgst,
            invoice.lines[0].sgst,
            invoice.lines[0].igst
        ),
        (1_800, 1_800, 0)
    );
    assert_eq!(
        (invoice.payment_status.as_str(), invoice.amount_paid),
        ("paid", 23_600)
    );
    assert_eq!(invoice.payments[0].change_given, 1_400);
    assert_eq!(invoice.lines[0].item_name, "Basmati Rice");
    assert_eq!(kit.stock(&rice), 8_000);
    kit.assert_books_balance();

    // The snapshot survives a later product rename.
    kit.conn().execute("UPDATE products SET name = 'Renamed' WHERE id = (SELECT product_id FROM product_variants WHERE id = ?1)", [&rice]).unwrap();
    let again = sales::invoice_detail(&kit.conn(), &invoice.id).unwrap();
    assert_eq!(again.lines[0].item_name, "Basmati Rice");
}

#[test]
fn backend_totals_win_over_the_client() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier");
    let mut input = sale("sale-000002", vec![line(&rice, 1)], vec![cash(11_800)]);
    input.expected_total = Some(10_000);
    assert_eq!(
        code(sell(&kit, &cashier, &input)),
        ErrorCode::TotalsMismatch
    );
    let overpaid = sale("sale-000003", vec![line(&rice, 1)], vec![cash(20_000)]);
    assert_eq!(
        code(sell(&kit, &cashier, &overpaid)),
        ErrorCode::Validation,
        "overpayment must be entered as tendered"
    );
    assert_eq!(kit.stock(&rice), 10_000, "nothing was written");
}

#[test]
fn credit_sales_need_a_customer_and_respect_the_credit_limit() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier");
    let walk_in_credit = sale("sale-000004", vec![line(&rice, 1)], vec![]);
    assert_eq!(
        code(sell(&kit, &cashier, &walk_in_credit)),
        ErrorCode::Validation
    );

    let customer = kit.party("Hotel Annapurna", "customer", Some(15_000));
    let mut on_credit = sale("sale-000005", vec![line(&rice, 1)], vec![]);
    on_credit.party_id = Some(customer.clone());
    let invoice = sell(&kit, &cashier, &on_credit).unwrap();
    assert_eq!(
        (invoice.payment_status.as_str(), kit.balance(&customer)),
        ("unpaid", 11_800)
    );

    let mut over_limit = sale("sale-000006", vec![line(&rice, 1)], vec![]);
    over_limit.party_id = Some(customer.clone());
    assert_eq!(
        code(sell(&kit, &cashier, &over_limit)),
        ErrorCode::ApprovalRequired
    );
    let manager = kit.user("manager");
    let approver = kit.approver(&manager);
    let mut conn = kit.conn();
    let mut ctx = kit.ctx(&mut conn, &cashier);
    ctx.approver = Some(&approver);
    sales::create_invoice(&mut ctx, &over_limit).unwrap();
    assert_eq!(kit.balance(&customer), 23_600);
    kit.assert_books_balance();
}

#[test]
fn discounts_and_price_overrides_need_approval_beyond_limits() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier"); // 5% limit
                                       // 4% off ₹100 + 18% = ₹113.28, rounded to ₹113 (default round-off to the nearest rupee).
    let mut small = sale("sale-000007", vec![line(&rice, 1)], vec![cash(11_300)]);
    small.lines[0].discount = Some(Discount::Percent { rate: 40_000 });
    let invoice = sell(&kit, &cashier, &small).unwrap();
    assert_eq!((invoice.total, invoice.round_off), (11_300, -28));

    let mut big = sale("sale-000008", vec![line(&rice, 1)], vec![cash(9_440)]);
    big.bill_discount = Some(Discount::Percent { rate: 200_000 });
    assert_eq!(
        code(sell(&kit, &cashier, &big)),
        ErrorCode::ApprovalRequired
    );

    let mut cheaper = sale("sale-000009", vec![line(&rice, 1)], vec![cash(5_900)]);
    cheaper.lines[0].unit_price = Some(5_000);
    assert_eq!(
        code(sell(&kit, &cashier, &cheaper)),
        ErrorCode::ApprovalRequired
    );
    let owner = kit.owner();
    assert_eq!(sell(&kit, &owner, &cheaper).unwrap().total, 5_900);
    let audited: i64 = kit
        .conn()
        .query_row("SELECT count(*) FROM audit_log WHERE action = 'SALE_FINALIZED' AND summary LIKE '%discount 400'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(audited, 1);
}

#[test]
fn negative_stock_policy_and_backdating() {
    let (kit, rice) = shop();
    let owner = kit.owner();
    let too_many = sale("sale-000010", vec![line(&rice, 11)], vec![cash(129_800)]);
    assert_eq!(
        code(sell(&kit, &owner, &too_many)),
        ErrorCode::InsufficientStock
    );
    {
        let mut conn = kit.conn();
        settings::update(
            &mut kit.ctx(&mut conn, &owner),
            &UpdateSettingsInput {
                section: "inventory".into(),
                value: json!({ "negativeStockPolicy": "warn" }),
            },
        )
        .unwrap();
    }
    let warned = sell(&kit, &owner, &too_many).unwrap();
    assert_eq!(warned.warnings.len(), 1);
    assert_eq!(kit.stock(&rice), -1_000);

    let cashier = kit.user("cashier");
    let mut backdated = sale("sale-000011", vec![line(&rice, 1)], vec![cash(11_800)]);
    backdated.business_date = Some("2026-10-01".into());
    assert_eq!(
        code(sell(&kit, &cashier, &backdated)),
        ErrorCode::ApprovalRequired
    );
    backdated.business_date = Some("2026-12-01".into());
    assert_eq!(code(sell(&kit, &owner, &backdated)), ErrorCode::Validation);
    kit.assert_books_balance();
}

#[test]
fn retries_are_idempotent() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier");
    let input = sale("sale-retry-1", vec![line(&rice, 1)], vec![cash(11_800)]);
    let first = sell(&kit, &cashier, &input).unwrap();
    let second = sell(&kit, &cashier, &input).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(kit.stock(&rice), 9_000);
    let changed = sale("sale-retry-1", vec![line(&rice, 2)], vec![cash(23_600)]);
    assert_eq!(code(sell(&kit, &cashier, &changed)), ErrorCode::Conflict);
}

#[test]
fn inter_state_sales_use_igst_and_packs_convert_to_base_units() {
    let kit = Kit::new();
    let gst = kit.gst_rate(18);
    let soap = kit.product("Soap", "SOAP-1", 1_000, 600, 30, Some(&gst));
    let owner = kit.owner();
    let mut conn = kit.conn();
    let product_id: String = conn
        .query_row(
            "SELECT product_id FROM product_variants WHERE id = ?1",
            [&soap],
            |r| r.get(0),
        )
        .unwrap();
    let base_unit: String = conn
        .query_row(
            "SELECT base_unit_id FROM products WHERE id = ?1",
            [&product_id],
            |r| r.get(0),
        )
        .unwrap();
    let box_unit = crate::services::catalog::save_unit(
        &mut kit.ctx(&mut conn, &owner),
        &crate::services::catalog::UnitInput {
            id: None,
            code: "ctn".into(),
            name: "Carton".into(),
            decimals: 0,
        },
    )
    .unwrap()
    .id;
    conn.execute(
        "INSERT INTO unit_conversions (id, product_id, from_unit_id, to_unit_id, factor_num, factor_den, created_at, updated_at) VALUES ('c1', ?1, ?2, ?3, 12, 1, ?4, ?4)",
        rusqlite::params![product_id, box_unit, base_unit, kit.now().to_rfc3339()],
    )
    .unwrap();
    let customer = kit.party("Chennai Traders", "customer", None);
    conn.execute(
        "UPDATE parties SET state_code = '33' WHERE id = ?1",
        [&customer],
    )
    .unwrap();
    let mut input = sale(
        "sale-000012",
        vec![SaleLineInput {
            unit_id: Some(box_unit),
            ..line(&soap, 1)
        }],
        vec![],
    );
    input.party_id = Some(customer);
    let invoice = sales::create_invoice(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    assert!(invoice.is_inter_state);
    assert_eq!(
        (
            invoice.lines[0].igst,
            invoice.lines[0].cgst,
            invoice.taxable
        ),
        (2_160, 0, 12_000)
    );
    assert_eq!(kit.stock(&soap), 18_000);
    kit.assert_books_balance();
}

#[test]
fn voids_restore_stock_balances_and_payments() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier");
    let customer = kit.party("Ravi", "customer", None);
    let mut input = sale("sale-000013", vec![line(&rice, 3)], vec![cash(10_000)]);
    input.party_id = Some(customer.clone());
    let invoice = sell(&kit, &cashier, &input).unwrap();
    assert_eq!(kit.balance(&customer), 35_400 - 10_000);

    let mut conn = kit.conn();
    let void = VoidInput {
        id: invoice.id.clone(),
        reason: "Wrong customer".into(),
    };
    assert_eq!(
        code(sales::void_invoice(
            &mut kit.ctx(&mut conn, &cashier),
            &void
        )),
        ErrorCode::ApprovalRequired
    );
    let manager = kit.user("manager");
    let voided = sales::void_invoice(&mut kit.ctx(&mut conn, &manager), &void).unwrap();
    assert_eq!(
        (voided.status.as_str(), voided.payment_status.as_str()),
        ("voided", "void")
    );
    assert_eq!(voided.payments[0].status, "voided");
    assert_eq!((kit.stock(&rice), kit.balance(&customer)), (10_000, 0));
    assert_eq!(
        code(sales::void_invoice(
            &mut kit.ctx(&mut conn, &manager),
            &void
        )),
        ErrorCode::Conflict
    );
    kit.assert_books_balance();
}

#[test]
fn returns_never_exceed_what_was_sold_and_add_up_exactly() {
    let (kit, rice) = shop();
    let owner = kit.owner();
    let customer = kit.party("Ravi", "customer", None);
    // 3 × ₹100 − ₹10 = ₹290 + 18% = ₹342.20, rounded to ₹342; paid in cash.
    let mut input = sale("sale-000014", vec![line(&rice, 3)], vec![cash(34_200)]);
    input.party_id = Some(customer.clone());
    input.bill_discount = Some(Discount::Amount { amount: 1_000 });
    let invoice = sell(&kit, &owner, &input).unwrap();
    assert_eq!((invoice.total, invoice.round_off), (34_200, -20));
    let item = invoice.lines[0].id.clone();
    let mut conn = kit.conn();
    let ret = |key: &str, qty: i64, refund: Option<PaymentInput>| ReturnInput {
        idempotency_key: key.into(),
        original_invoice_id: Some(invoice.id.clone()),
        party_id: None,
        warehouse_id: None,
        reason: "Damaged packet".into(),
        lines: vec![ReturnLineInput {
            original_item_id: Some(item.clone()),
            variant_id: None,
            qty,
            restock: true,
        }],
        refund,
    };
    let first = sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ret("ret-000001", 1_000, None),
    )
    .unwrap();
    let second = sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ret("ret-000002", 2_000, Some(cash(5_000))),
    )
    .unwrap();
    assert_eq!(first.total + second.total, invoice.total);
    assert_eq!(first.taxable + second.taxable, invoice.taxable);
    assert_eq!(
        code(sales::create_return(
            &mut kit.ctx(&mut conn, &owner),
            &ret("ret-000003", 1_000, None)
        )),
        ErrorCode::Validation
    );
    assert_eq!(kit.stock(&rice), 10_000);
    assert_eq!(
        kit.balance(&customer),
        -(invoice.total - 5_000),
        "store credit left after the refund"
    );
    let cashier = kit.user("cashier");
    assert_eq!(
        code(sales::create_return(
            &mut kit.ctx(&mut conn, &cashier),
            &ret("ret-000004", 1_000, None)
        )),
        ErrorCode::ApprovalRequired
    );
    // The invoice can no longer be voided once goods came back.
    let void = VoidInput {
        id: invoice.id.clone(),
        reason: "x".into(),
    };
    assert_eq!(
        code(sales::void_invoice(&mut kit.ctx(&mut conn, &owner), &void)),
        ErrorCode::Conflict
    );
    kit.assert_books_balance();
}

#[test]
fn quotations_convert_once_at_quoted_prices() {
    let (kit, rice) = shop();
    let owner = kit.owner();
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let mut quote_line = line(&rice, 2);
    quote_line.unit_price = Some(9_500);
    let quote = sales::save_sales_doc(
        &mut kit.ctx(&mut conn, &owner),
        &SalesDocInput {
            id: None,
            doc_type: "quotation".into(),
            party_id: None,
            customer_name: Some("Walk-in".into()),
            customer_phone: None,
            valid_until: Some("2026-10-31".into()),
            prices_include_tax: None,
            lines: vec![quote_line],
            bill_discount: None,
            notes: None,
            terms: None,
        },
    )
    .unwrap();
    assert_eq!(
        (quote.doc_no.as_str(), quote.total),
        ("QT/26-27/00001", 22_400)
    );
    let convert = ConvertInput {
        document_id: quote.id.clone(),
        idempotency_key: "conv-000001".into(),
        payments: vec![cash(22_400)],
        expected_total: Some(22_400),
        register_session_id: None,
    };
    let invoice = sales::convert_sales_doc(&mut kit.ctx(&mut conn, &cashier), &convert).unwrap();
    assert_eq!(invoice.total, 22_400);
    let again = ConvertInput {
        idempotency_key: "conv-000002".into(),
        ..convert
    };
    assert_eq!(
        code(sales::convert_sales_doc(
            &mut kit.ctx(&mut conn, &cashier),
            &again
        )),
        ErrorCode::Conflict
    );
    assert_eq!(
        sales::get_sales_doc(&kit.ctx(&mut conn, &owner), &IdInput { id: quote.id })
            .unwrap()
            .status,
        "converted"
    );
}

#[test]
fn held_bills_can_be_recalled_only_once() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let bill = sales::hold(
        &mut kit.ctx(&mut conn, &cashier),
        &HoldInput {
            label: "Table 4".into(),
            party_id: None,
            cart: json!({ "lines": [] }),
        },
    )
    .unwrap();
    assert_eq!(
        sales::list_held(&kit.ctx(&mut conn, &cashier))
            .unwrap()
            .len(),
        1
    );
    sales::recall(
        &mut kit.ctx(&mut conn, &cashier),
        &IdInput {
            id: bill.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        code(sales::recall(
            &mut kit.ctx(&mut conn, &cashier),
            &IdInput { id: bill.id }
        )),
        ErrorCode::Conflict
    );
}

#[test]
fn receipts_allocate_oldest_first_and_advances_can_be_applied_later() {
    let (kit, rice) = shop();
    let owner = kit.owner();
    let customer = kit.party("Ravi", "customer", None);
    let mut ids = Vec::new();
    for key in ["sale-cr-0001", "sale-cr-0002"] {
        let mut input = sale(key, vec![line(&rice, 1)], vec![]);
        input.party_id = Some(customer.clone());
        ids.push(sell(&kit, &owner, &input).unwrap().id);
    }
    let mut conn = kit.conn();
    let receipt = payments::record(
        &mut kit.ctx(&mut conn, &owner),
        &PaymentEntryInput {
            idempotency_key: "rcpt-000001".into(),
            party_id: customer.clone(),
            direction: "in".into(),
            payment: PaymentInput {
                mode: "upi".into(),
                amount: 15_000,
                tendered: None,
                account_id: None,
                reference: Some("UTR123".into()),
            },
            business_date: None,
            allocations: vec![],
            auto_allocate: true,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(
        (receipt.allocations[0].amount, receipt.allocations[1].amount),
        (11_800, 3_200)
    );
    let first = sales::invoice_detail(&conn, &ids[0]).unwrap();
    assert_eq!(first.payment_status, "paid");

    let advance = payments::record(
        &mut kit.ctx(&mut conn, &owner),
        &PaymentEntryInput {
            idempotency_key: "rcpt-000002".into(),
            party_id: customer.clone(),
            direction: "in".into(),
            payment: cash(10_000),
            business_date: None,
            allocations: vec![],
            auto_allocate: false,
            notes: Some("Advance".into()),
        },
    )
    .unwrap();
    assert_eq!(advance.unallocated, 10_000);
    let applied = payments::allocate(
        &mut kit.ctx(&mut conn, &owner),
        &AllocateInput {
            payment_id: advance.id.clone(),
            allocations: vec![AllocationInput {
                target_type: "sales_invoice".into(),
                target_id: ids[1].clone(),
                amount: 8_600,
            }],
        },
    )
    .unwrap();
    assert_eq!(applied.unallocated, 1_400);
    assert_eq!(kit.balance(&customer), 23_600 - 25_000);
    payments::void(
        &mut kit.ctx(&mut conn, &owner),
        &VoidPaymentInput {
            id: advance.id,
            reason: "Bounced".into(),
        },
    )
    .unwrap();
    assert_eq!(
        sales::invoice_detail(&conn, &ids[1]).unwrap().amount_paid,
        3_200
    );
    kit.assert_books_balance();
}

#[test]
fn till_cash_up() {
    let (kit, rice) = shop();
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let session = payments::open_register(
        &mut kit.ctx(&mut conn, &cashier),
        &OpenRegisterInput {
            register_id: None,
            opening_float: 50_000,
        },
    )
    .unwrap();
    assert_eq!(
        code(payments::open_register(
            &mut kit.ctx(&mut conn, &cashier),
            &OpenRegisterInput {
                register_id: None,
                opening_float: 0
            }
        )),
        ErrorCode::Conflict
    );
    let mut input = sale("sale-till-01", vec![line(&rice, 1)], vec![cash(11_800)]);
    input.register_session_id = Some(session.id.clone());
    sales::create_invoice(&mut kit.ctx(&mut conn, &cashier), &input).unwrap();
    let closed = payments::close_register(
        &mut kit.ctx(&mut conn, &cashier),
        &CloseRegisterInput {
            session_id: session.id,
            counted_cash: 61_700,
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(
        (closed.expected_cash, closed.difference),
        (61_800, Some(-100))
    );
}

#[test]
fn numbers_restart_each_financial_year() {
    let (kit, rice) = shop();
    let owner = kit.owner();
    assert_eq!(
        sell(
            &kit,
            &owner,
            &sale("fy-sale-0001", vec![line(&rice, 1)], vec![cash(11_800)])
        )
        .unwrap()
        .doc_no,
        "INV/26-27/00001"
    );
    assert_eq!(
        sell(
            &kit,
            &owner,
            &sale("fy-sale-0002", vec![line(&rice, 1)], vec![cash(11_800)])
        )
        .unwrap()
        .doc_no,
        "INV/26-27/00002"
    );
    kit.clock
        .set(Utc.with_ymd_and_hms(2027, 3, 31, 19, 0, 0).unwrap()); // 1 April 00:30 in India
    let owner = kit.owner();
    assert_eq!(
        sell(
            &kit,
            &owner,
            &sale("fy-sale-0003", vec![line(&rice, 1)], vec![cash(11_800)])
        )
        .unwrap()
        .doc_no,
        "INV/27-28/00001"
    );
}

/// Two counters sell the last unit at the same moment: exactly one sale succeeds.
#[test]
fn concurrent_sales_of_the_last_unit() {
    let kit = Arc::new(Kit::new());
    let last = kit.product("Last item", "LAST-1", 10_000, 5_000, 1, None);
    let owner = kit.owner();
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let kit = Arc::clone(&kit);
            let owner = owner.clone();
            let last = last.clone();
            std::thread::spawn(move || {
                let input = sale(
                    &format!("race-sale-{n:04}"),
                    vec![line(&last, 1)],
                    vec![cash(10_000)],
                );
                sell(&kit, &owner, &input).map(|i| i.doc_no)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let ok = results.iter().filter(|r| r.is_ok()).count();
    let insufficient = results
        .iter()
        .filter(|r| matches!(r, Err(e) if e.code == ErrorCode::InsufficientStock))
        .count();
    assert_eq!((ok, insufficient), (1, 1), "{results:?}");
    assert_eq!(kit.stock(&last), 0);
    kit.assert_books_balance();
}

#[test]
fn walk_in_returns_are_refunded_in_full_whatever_amount_was_sent() {
    let (kit, rice) = shop();
    let owner = kit.owner();
    let invoice = sell(
        &kit,
        &owner,
        &sale("sale-walkin-1", vec![line(&rice, 2)], vec![cash(23_600)]),
    )
    .unwrap();
    let mut conn = kit.conn();
    let ret = sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "ret-walkin-1".into(),
            original_invoice_id: Some(invoice.id.clone()),
            party_id: None,
            warehouse_id: None,
            reason: "Wrong item".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(invoice.lines[0].id.clone()),
                variant_id: None,
                qty: 1_000,
                restock: true,
            }],
            // The screen only estimates; the backend refunds the exact total.
            refund: Some(cash(1)),
        },
    )
    .unwrap();
    assert_eq!((ret.total, ret.amount_refunded), (11_800, 11_800));
    drop(conn);
    kit.assert_books_balance();
}
