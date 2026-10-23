//! Reports, books, day close, audit log and cash/bank accounts. The figures are checked against
//! the documents that made them, and the books must balance after everything.

use serde_json::{json, Value};

use crate::error::ErrorCode;
use crate::services::accounting::{self, AuditQuery, DayInput, MoneyAccountInput};
use crate::services::expenses::{self, ExpenseInput};
use crate::services::payments::{self, CloseRegisterInput, OpenRegisterInput};
use crate::services::purchases::{self, PurchaseBillInput, PurchaseLineInput};
use crate::services::reports::{self, ReportQuery, ReportTable};
use crate::services::sales::{
    self, PaymentInput, ReturnInput, ReturnLineInput, SaleInput, SaleLineInput,
};
use crate::services::testkit::{code, Kit};

pub(super) fn cash(amount: i64) -> PaymentInput {
    PaymentInput {
        mode: "cash".into(),
        amount,
        tendered: None,
        account_id: None,
        reference: None,
    }
}

pub(super) fn sale(key: &str, variant: &str, units: i64, pay: i64) -> SaleInput {
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
        lines: vec![SaleLineInput {
            variant_id: variant.into(),
            unit_id: None,
            qty: units * 1000,
            unit_price: None,
            discount: None,
            batch_id: None,
        }],
        bill_discount: None,
        payments: vec![cash(pay)],
        expected_total: None,
        notes: None,
        source_document_id: None,
        register_session_id: None,
    }
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

fn cell(t: &ReportTable, row: usize, key: &str) -> Value {
    let i = t
        .columns
        .iter()
        .position(|c| c.key == key)
        .unwrap_or_else(|| panic!("no column {key}"));
    t.rows[row][i].clone()
}

/// A shop with a purchase (20 × ₹40, no tax), two sales (₹100 + 18 % GST each, cost ₹40), one
/// return and a rent expense.
fn trading_shop() -> Kit {
    let kit = Kit::new();
    let gst = kit.gst_rate(18);
    let rice = kit.product("Rice", "RICE-1", 10_000, 4_000, 0, Some(&gst));
    let supplier = kit.party("Agro Traders", "supplier", None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    purchases::create_bill(
        &mut kit.ctx(&mut conn, &owner),
        &PurchaseBillInput {
            idempotency_key: "bill-rep-001".into(),
            party_id: supplier,
            supplier_invoice_no: Some("AT-9".into()),
            supplier_invoice_date: None,
            business_date: None,
            due_date: None,
            warehouse_id: None,
            prices_include_tax: None,
            purchase_order_id: None,
            goods_receipt_id: None,
            lines: vec![PurchaseLineInput {
                variant_id: rice.clone(),
                unit_id: None,
                qty: 20_000,
                unit_cost: 4_000,
                discount: None,
                batch_no: None,
                expiry_date: None,
                mfg_date: None,
                po_item_id: None,
            }],
            bill_discount: None,
            charges: Vec::new(),
            payments: Vec::new(),
            expected_total: None,
            notes: None,
        },
    )
    .unwrap();
    let a = sales::create_invoice(
        &mut kit.ctx(&mut conn, &owner),
        &sale("sale-rep-0001", &rice, 1, 11_800),
    )
    .unwrap();
    sales::create_invoice(
        &mut kit.ctx(&mut conn, &owner),
        &sale("sale-rep-0002", &rice, 1, 11_800),
    )
    .unwrap();
    sales::create_return(
        &mut kit.ctx(&mut conn, &owner),
        &ReturnInput {
            idempotency_key: "ret-rep-0001".into(),
            original_invoice_id: Some(a.id.clone()),
            party_id: None,
            warehouse_id: None,
            reason: "Torn".into(),
            lines: vec![ReturnLineInput {
                original_item_id: Some(a.lines[0].id.clone()),
                variant_id: None,
                qty: 1_000,
                restock: true,
            }],
            refund: Some(cash(1)),
        },
    )
    .unwrap();
    let rent: String = conn
        .query_row(
            "SELECT id FROM expense_categories WHERE name = 'Rent'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    expenses::create(
        &mut kit.ctx(&mut conn, &owner),
        &ExpenseInput {
            idempotency_key: "exp-rep-0001".into(),
            category_id: rent,
            title: "October rent".into(),
            description: None,
            reference_no: None,
            amount: 3_000,
            tax_rate: 0,
            business_date: None,
            party_id: None,
            payment: Some(cash(3_000)),
        },
    )
    .unwrap();
    drop(conn);
    kit
}

#[test]
fn sales_profit_tax_and_stock_reports_add_up() {
    let kit = trading_shop();

    // Two bills of ₹118, one ₹118 credit note.
    let s = report(&kit, "sales_summary");
    assert_eq!(s.rows.len(), 1);
    assert_eq!(
        (
            cell(&s, 0, "bills"),
            cell(&s, 0, "total"),
            cell(&s, 0, "returns"),
            cell(&s, 0, "net")
        ),
        (json!(2), json!(23_600), json!(11_800), json!(11_800))
    );
    assert_eq!(s.totals.as_ref().unwrap()[7], json!(11_800));

    // One unit net: ₹100 sold for ₹40 cost.
    let p = report(&kit, "sales_by_item");
    assert_eq!(
        (
            cell(&p, 0, "qty"),
            cell(&p, 0, "revenue"),
            cell(&p, 0, "cost"),
            cell(&p, 0, "profit"),
            cell(&p, 0, "margin")
        ),
        (
            json!(1_000),
            json!(10_000),
            json!(4_000),
            json!(6_000),
            json!(600_000)
        )
    );

    // Output tax ₹18 net of the credit note; input tax ₹144 on the ₹800 bill: ₹126 to claim.
    let t = report(&kit, "tax_summary");
    let out = t
        .rows
        .iter()
        .find(|r| r[0] == json!("Output (sales)"))
        .unwrap();
    assert_eq!(
        (out[2].clone(), out[3].clone(), out[4].clone()),
        (json!(10_000), json!(900), json!(900))
    );
    let net = t.rows.last().unwrap();
    assert_eq!(
        (net[0].clone(), net[7].clone()),
        (json!("Net tax payable"), json!(-12_600))
    );

    let v = report(&kit, "stock_valuation");
    assert_eq!(
        (
            cell(&v, 0, "qty"),
            cell(&v, 0, "avgCost"),
            cell(&v, 0, "value")
        ),
        (json!(19_000), json!(4_000), json!(76_000))
    );
    let m = report(&kit, "stock_movement");
    assert_eq!(
        (
            cell(&m, 0, "opening"),
            cell(&m, 0, "in"),
            cell(&m, 0, "out"),
            cell(&m, 0, "closing")
        ),
        (json!(0), json!(21_000), json!(2_000), json!(19_000))
    );
    let pu = report(&kit, "purchases");
    assert_eq!((pu.rows.len(), cell(&pu, 0, "theirNo")), (1, json!("AT-9")));
    let ex = report(&kit, "expenses");
    assert_eq!(
        (cell(&ex, 0, "category"), cell(&ex, 0, "total")),
        (json!("Rent"), json!(3_000))
    );
}

#[test]
fn the_books_agree_with_each_other() {
    let kit = trading_shop();

    // Profit: sales ₹100 (net of the return), cost ₹40, rent ₹30 = ₹30.
    let pl = report(&kit, "profit_loss");
    let amount = |name: &str| {
        pl.rows
            .iter()
            .find(|r| r[2] == json!(name))
            .map(|r| r[3].as_i64().unwrap())
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(amount("Gross profit"), 6_000);
    assert_eq!(amount("Total expenses"), 3_000);
    assert_eq!(amount("Net profit"), 3_000);

    // The trial balance is in balance and cash in hand holds what came in less what went out.
    let tb = report(&kit, "trial_balance");
    let totals = tb.totals.as_ref().unwrap();
    assert_eq!(totals[3], totals[4]);
    let cb = report(&kit, "cash_book");
    // +118 +118 −118 (refund) −30 rent = ₹88.
    assert_eq!(cb.rows.last().unwrap()[5], json!(8_800));
    assert_eq!(cb.totals.as_ref().unwrap()[5], json!(8_800));
    assert_eq!(
        report(&kit, "bank_book").rows.len(),
        1,
        "only the opening line"
    );

    // The day book lists every posting; customers owe nothing; the supplier is owed the ₹944 bill.
    assert!(report(&kit, "day_book").rows.len() >= 5);
    assert!(report(&kit, "party_balances").rows.is_empty());
    let owner = kit.owner();
    let mut conn = kit.conn();
    let payables = reports::run(
        &kit.ctx(&mut conn, &owner),
        &ReportQuery {
            report: "party_balances".into(),
            party_type: Some("supplier".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        (cell(&payables, 0, "name"), cell(&payables, 0, "balance")),
        (json!("Agro Traders"), json!(94_400))
    );
    drop(conn);
    kit.assert_books_balance();
}

#[test]
fn report_access_and_input_are_checked() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let accountant = kit.user("accountant");
    let mut conn = kit.conn();
    let q = |r: &str| ReportQuery {
        report: r.into(),
        ..Default::default()
    };
    assert_eq!(
        code(reports::run(
            &kit.ctx(&mut conn, &cashier),
            &q("sales_summary")
        )),
        ErrorCode::Forbidden
    );
    assert!(reports::run(&kit.ctx(&mut conn, &accountant), &q("sales_summary")).is_ok());
    assert_eq!(
        code(reports::run(&kit.ctx(&mut conn, &accountant), &q("nope"))),
        ErrorCode::Validation
    );
    let backwards = ReportQuery {
        report: "expenses".into(),
        from: Some("2026-10-20".into()),
        to: Some("2026-10-01".into()),
        ..Default::default()
    };
    assert_eq!(
        code(reports::run(&kit.ctx(&mut conn, &accountant), &backwards)),
        ErrorCode::Validation
    );
    for name in reports::REPORTS {
        reports::run(&kit.ctx(&mut conn, &accountant), &q(name))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn closing_the_day_needs_closed_tills_and_happens_once() {
    let kit = trading_shop();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let session = payments::open_register(
        &mut kit.ctx(&mut conn, &owner),
        &OpenRegisterInput {
            register_id: None,
            opening_float: 1_000,
        },
    )
    .unwrap();
    let summary =
        accounting::day_summary(&kit.ctx(&mut conn, &owner), &DayInput::default()).unwrap();
    assert_eq!(
        (
            summary.bills,
            summary.sales_total,
            summary.returns_total,
            summary.expenses_total,
            summary.open_tills
        ),
        (2, 23_600, 11_800, 3_000, 1)
    );
    assert_eq!(summary.received[0].amount, 23_600);
    assert_eq!(
        code(accounting::close_day(
            &mut kit.ctx(&mut conn, &owner),
            &DayInput::default()
        )),
        ErrorCode::Conflict
    );

    payments::close_register(
        &mut kit.ctx(&mut conn, &owner),
        &CloseRegisterInput {
            session_id: session.id,
            counted_cash: 1_000,
            notes: None,
            difference_reason: Some("counting_error".into()),
        },
    )
    .unwrap();
    let closed = accounting::close_day(
        &mut kit.ctx(&mut conn, &owner),
        &DayInput {
            business_date: None,
            notes: Some("Quiet day".into()),
        },
    )
    .unwrap();
    assert!(closed.closed_at.is_some());
    assert_eq!((closed.tills.len(), closed.tills[0].difference), (1, 0));
    assert_eq!(
        code(accounting::close_day(
            &mut kit.ctx(&mut conn, &owner),
            &DayInput::default()
        )),
        ErrorCode::Conflict
    );
    let future = DayInput {
        business_date: Some("2026-12-31".into()),
        notes: None,
    };
    assert_eq!(
        code(accounting::close_day(
            &mut kit.ctx(&mut conn, &owner),
            &future
        )),
        ErrorCode::Validation
    );
    let list = accounting::list_day_closes(&kit.ctx(&mut conn, &owner)).unwrap();
    assert_eq!(
        (list.len(), list[0].sales_total, list[0].bills),
        (1, 23_600, 2)
    );
    let cashier = kit.user("cashier");
    assert_eq!(
        code(accounting::close_day(
            &mut kit.ctx(&mut conn, &cashier),
            &DayInput::default()
        )),
        ErrorCode::Forbidden
    );
}

#[test]
fn bank_accounts_and_the_audit_log() {
    let kit = Kit::new();
    let owner = kit.owner();
    let accountant = kit.user("accountant");
    let mut conn = kit.conn();
    let input = |name: &str| MoneyAccountInput {
        id: None,
        kind: "bank".into(),
        name: name.into(),
        bank_name: Some("HDFC".into()),
        bank_account_no: Some("5010".into()),
        bank_ifsc: None,
        is_active: true,
    };
    let first = accounting::save_money_account(
        &mut kit.ctx(&mut conn, &accountant),
        &input("HDFC Current"),
    )
    .unwrap();
    let second =
        accounting::save_money_account(&mut kit.ctx(&mut conn, &accountant), &input("SBI Savings"))
            .unwrap();
    assert_eq!(
        (
            first.code.as_str(),
            second.code.as_str(),
            first.subtype.as_str()
        ),
        ("1011", "1012", "bank")
    );
    let renamed = accounting::save_money_account(
        &mut kit.ctx(&mut conn, &accountant),
        &MoneyAccountInput {
            id: Some(first.id.clone()),
            is_active: false,
            ..input("HDFC Main")
        },
    )
    .unwrap();
    assert_eq!(
        (renamed.name.as_str(), renamed.is_active),
        ("HDFC Main", false)
    );
    let cashier = kit.user("cashier");
    assert_eq!(
        code(accounting::save_money_account(
            &mut kit.ctx(&mut conn, &cashier),
            &input("x")
        )),
        ErrorCode::Forbidden
    );
    assert!(accounting::list_accounts(&kit.ctx(&mut conn, &accountant))
        .unwrap()
        .iter()
        .any(|a| a.name == "SBI Savings"));

    // Every change above is in the log, newest first, filterable; only auditors read it.
    let log = accounting::list_audit(
        &kit.ctx(&mut conn, &owner),
        &AuditQuery {
            action: Some("ACCOUNT_SAVE".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(log.total, 3);
    assert_eq!(log.items[0].user_name.as_deref(), Some("accountant"));
    let by_text = accounting::list_audit(
        &kit.ctx(&mut conn, &owner),
        &AuditQuery {
            search: Some("SBI".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(by_text.total, 1);
    assert!(accounting::audit_actions(&kit.ctx(&mut conn, &owner))
        .unwrap()
        .contains(&"ACCOUNT_SAVE".to_string()));
    assert_eq!(
        code(accounting::list_audit(
            &kit.ctx(&mut conn, &cashier),
            &AuditQuery::default()
        )),
        ErrorCode::Forbidden
    );
}
