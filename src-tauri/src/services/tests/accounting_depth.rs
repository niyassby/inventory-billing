//! Phase 11: reviewed manual journals (approval, control accounts, reversal), the closed-day
//! lock and reopening, financial-year close, bank statements (import, matching on fixtures,
//! reconciliation), post-dated cheques, till differences, audit export, report grouping and
//! drill-down, saved views with scheduled exports, and the permission matrix of the new actions.
//! Every flow ends with the books in balance.

use serde_json::json;

use crate::error::{AppError, ErrorCode};
use crate::jobs;
use crate::services::accounting::{self, AuditQuery, DayInput};
use crate::services::accounts;
use crate::services::bank::{
    self, AccountInput, BankQuery, FinishInput, MatchInput, ReconcileQuery, StatementImportInput,
    StatementMapping, TickInput, UnmatchInput,
};
use crate::services::cheques::{
    self, ChequeClearInput, ChequeInput, ChequeQuery, ChequeStatusInput,
};
use crate::services::context::Actor;
use crate::services::expenses::{self, ExpenseInput};
use crate::services::manual_journals::{
    self as journals, JournalActionInput, JournalDetail, JournalInput, JournalLineInput,
    ReverseJournalInput,
};
use crate::services::payments::{
    self, CloseRegisterInput, OpenRegisterInput, PaymentEntryInput, VoidPaymentInput,
};
use crate::services::period::{
    self, ReopenDayInput, YearCloseInput, YearPreviewInput, YearReopenInput,
};
use crate::services::report_views::{self, ReportViewInput, ScheduleInput, ViewParams};
use crate::services::reports::{self, ReportQuery, ReportTable};
use crate::services::sales::{
    self, InvoiceDetail, PaymentInput, SaleInput, SaleLineInput, VoidInput,
};
use crate::services::settings::{self, UpdateSettingsInput};
use crate::services::testkit::{code, Kit};
use crate::services::Ctx;

// ---------------------------------------------------------------- helpers

fn line(account: &str, debit: i64, credit: i64) -> JournalLineInput {
    JournalLineInput {
        account_id: account.into(),
        party_id: None,
        debit,
        credit,
        memo: None,
    }
}

fn draft(
    kit: &Kit,
    actor: &Actor,
    key: &str,
    day: Option<&str>,
    lines: Vec<JournalLineInput>,
) -> Result<JournalDetail, AppError> {
    let mut conn = kit.conn();
    journals::save(
        &mut kit.ctx(&mut conn, actor),
        &JournalInput {
            id: None,
            expected_updated_at: None,
            idempotency_key: Some(format!("jv-{key}-00001")),
            business_date: day.map(String::from),
            narration: format!("Journal {key}"),
            lines,
        },
    )
}

fn act(
    kit: &Kit,
    actor: &Actor,
    f: fn(&mut Ctx<'_>, &JournalActionInput) -> Result<JournalDetail, AppError>,
    id: &str,
    note: Option<&str>,
) -> Result<JournalDetail, AppError> {
    let mut conn = kit.conn();
    f(
        &mut kit.ctx(&mut conn, actor),
        &JournalActionInput {
            id: id.into(),
            expected_updated_at: None,
            note: note.map(String::from),
        },
    )
}

/// Draft → submitted (writer) → approved (owner) → posted (writer).
fn post_journal(kit: &Kit, writer: &Actor, id: &str) -> JournalDetail {
    let owner = kit.owner();
    act(kit, writer, journals::submit, id, None).unwrap();
    act(kit, &owner, journals::approve, id, None).unwrap();
    act(kit, writer, journals::post, id, None).unwrap()
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

fn bank_payment(amount: i64, mode: &str, reference: Option<&str>) -> PaymentInput {
    PaymentInput {
        mode: mode.into(),
        amount,
        tendered: None,
        account_id: Some(accounts::BANK.into()),
        reference: reference.map(String::from),
    }
}

fn sale(
    key: &str,
    variant: &str,
    party: Option<&str>,
    day: Option<&str>,
    pay: Option<i64>,
) -> SaleInput {
    SaleInput {
        idempotency_key: format!("sale-{key}-0001"),
        warehouse_id: None,
        party_id: party.map(String::from),
        customer_name: None,
        customer_phone: None,
        customer_gstin: None,
        place_of_supply: None,
        business_date: day.map(String::from),
        prices_include_tax: None,
        lines: vec![SaleLineInput {
            variant_id: variant.into(),
            unit_id: None,
            qty: 1000,
            unit_price: None,
            discount: None,
            batch_id: None,
        }],
        bill_discount: None,
        payments: pay.map(|p| vec![cash(p)]).unwrap_or_default(),
        expected_total: None,
        notes: None,
        source_document_id: None,
        register_session_id: None,
    }
}

fn sell(kit: &Kit, actor: &Actor, input: &SaleInput) -> Result<InvoiceDetail, AppError> {
    let mut conn = kit.conn();
    sales::create_invoice(&mut kit.ctx(&mut conn, actor), input)
}

fn category(kit: &Kit) -> String {
    kit.conn()
        .query_row(
            "SELECT id FROM expense_categories ORDER BY name LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn expense(
    kit: &Kit,
    actor: &Actor,
    key: &str,
    amount: i64,
    day: Option<&str>,
    payment: PaymentInput,
) -> Result<expenses::ExpenseDetail, AppError> {
    let mut conn = kit.conn();
    expenses::create(
        &mut kit.ctx(&mut conn, actor),
        &ExpenseInput {
            idempotency_key: format!("exp-{key}-00001"),
            category_id: category(kit),
            title: format!("Expense {key}"),
            description: None,
            reference_no: None,
            amount,
            tax_rate: 0,
            business_date: day.map(String::from),
            party_id: None,
            payment: Some(payment),
        },
    )
}

fn record(
    kit: &Kit,
    actor: &Actor,
    key: &str,
    party: &str,
    direction: &str,
    payment: PaymentInput,
    day: Option<&str>,
) -> payments::PaymentDetail {
    let mut conn = kit.conn();
    payments::record(
        &mut kit.ctx(&mut conn, actor),
        &PaymentEntryInput {
            idempotency_key: format!("pay-{key}-00001"),
            party_id: party.into(),
            direction: direction.into(),
            payment,
            business_date: day.map(String::from),
            allocations: Vec::new(),
            auto_allocate: false,
            notes: None,
        },
    )
    .unwrap()
}

fn set_accounting(
    kit: &Kit,
    actor: &Actor,
    value: serde_json::Value,
) -> Result<settings::Settings, AppError> {
    let mut conn = kit.conn();
    settings::update(
        &mut kit.ctx(&mut conn, actor),
        &UpdateSettingsInput {
            section: "accounting".into(),
            value,
        },
    )
}

fn run_report(kit: &Kit, q: ReportQuery) -> Result<ReportTable, AppError> {
    let owner = kit.owner();
    let mut conn = kit.conn();
    reports::run(&kit.ctx(&mut conn, &owner), &q)
}

fn audit_count(kit: &Kit, action: &str) -> i64 {
    kit.conn()
        .query_row(
            "SELECT count(*) FROM audit_log WHERE action = ?1",
            [action],
            |r| r.get(0),
        )
        .unwrap()
}

fn account_balance(kit: &Kit, account: &str, until: &str) -> i64 {
    kit.conn()
        .query_row(
            "SELECT coalesce(sum(l.debit - l.credit), 0) FROM journal_lines l JOIN journal_entries e ON e.id = l.entry_id
             WHERE l.account_id = ?1 AND e.business_date <= ?2",
            [account, until],
            |r| r.get(0),
        )
        .unwrap()
}

// ---------------------------------------------------------------- manual journals

#[test]
fn a_manual_journal_is_reviewed_posted_and_reversed_by_a_reviewed_journal() {
    let kit = Kit::new();
    let accountant = kit.user("accountant");
    let owner = kit.owner();

    // Unbalanced: it can be saved as a draft, not submitted.
    let j = draft(
        &kit,
        &accountant,
        "fees",
        None,
        vec![
            line(accounts::GENERAL_EXPENSES, 1_000, 0),
            line(accounts::BANK, 0, 900),
        ],
    )
    .unwrap();
    assert_eq!(j.doc_no, "JV/26-27/00001");
    assert_eq!(j.status, "draft");
    assert_eq!(
        code(act(&kit, &accountant, journals::submit, &j.id, None)),
        ErrorCode::Validation
    );
    kit.clock.advance(chrono::Duration::seconds(1));
    {
        let mut conn = kit.conn();
        journals::save(
            &mut kit.ctx(&mut conn, &accountant),
            &JournalInput {
                id: Some(j.id.clone()),
                expected_updated_at: Some(j.updated_at.clone()),
                idempotency_key: None,
                business_date: None,
                narration: "Bank fees for October".into(),
                lines: vec![
                    line(accounts::GENERAL_EXPENSES, 1_000, 0),
                    line(accounts::BANK, 0, 1_000),
                ],
            },
        )
        .unwrap();
        // A stale edit is refused.
        let stale = journals::save(
            &mut kit.ctx(&mut conn, &accountant),
            &JournalInput {
                id: Some(j.id.clone()),
                expected_updated_at: Some(j.updated_at.clone()),
                idempotency_key: None,
                business_date: None,
                narration: "x".into(),
                lines: vec![
                    line(accounts::GENERAL_EXPENSES, 1, 0),
                    line(accounts::BANK, 0, 1),
                ],
            },
        );
        assert_eq!(code(stale), ErrorCode::Conflict);
    }
    act(&kit, &accountant, journals::submit, &j.id, None).unwrap();
    // The accountant can't approve, and nothing is posted before approval.
    assert_eq!(
        code(act(&kit, &accountant, journals::approve, &j.id, None)),
        ErrorCode::Forbidden
    );
    assert_eq!(
        code(act(&kit, &accountant, journals::post, &j.id, None)),
        ErrorCode::Conflict
    );
    // Sent back with a note, fixed, submitted again, approved, posted.
    let back = act(
        &kit,
        &owner,
        journals::send_back,
        &j.id,
        Some("Attach the bank advice"),
    )
    .unwrap();
    assert_eq!(
        (back.status.as_str(), back.review_note.as_deref()),
        ("draft", Some("Attach the bank advice"))
    );
    let posted = post_journal(&kit, &accountant, &j.id);
    assert_eq!(posted.status, "posted");
    assert_eq!(posted.approved_by_name.as_deref(), Some("Owner"));
    let (source, narration): (String, String) = kit
        .conn()
        .query_row(
            "SELECT source_type, narration FROM journal_entries WHERE source_id = ?1",
            [&j.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(source, "manual_journal");
    assert!(
        narration.starts_with("Manual journal JV/26-27/00001"),
        "tagged manual: {narration}"
    );
    assert_eq!(account_balance(&kit, accounts::BANK, "2026-10-17"), -1_000);
    kit.assert_books_balance();

    // Posted is final: no edits, no deletes, not even in the database.
    {
        let mut conn = kit.conn();
        let deleted = journals::delete(
            &mut kit.ctx(&mut conn, &accountant),
            &JournalActionInput {
                id: j.id.clone(),
                expected_updated_at: None,
                note: None,
            },
        );
        assert_eq!(code(deleted), ErrorCode::Conflict);
    }
    let raw = kit.conn().execute(
        "UPDATE manual_journals SET narration = 'changed' WHERE id = ?1",
        [&j.id],
    );
    assert!(raw.unwrap_err().to_string().contains("DOCUMENT_FINALIZED"));

    // The reversal is a journal of its own: same review, then the original is reversed.
    let rev = {
        let mut conn = kit.conn();
        journals::reverse(
            &mut kit.ctx(&mut conn, &accountant),
            &ReverseJournalInput {
                id: j.id.clone(),
                idempotency_key: "jv-reverse-00001".into(),
                business_date: None,
                reason: "Charged twice".into(),
            },
        )
        .unwrap()
    };
    assert_eq!(
        rev.reversal_of.as_ref().map(|r| r.doc_no.as_str()),
        Some("JV/26-27/00001")
    );
    assert_eq!((rev.lines[0].credit, rev.lines[1].debit), (1_000, 1_000));
    let posted = post_journal(&kit, &accountant, &rev.id);
    assert_eq!(posted.status, "posted");
    let original = {
        let mut conn = kit.conn();
        journals::get(&kit.ctx(&mut conn, &accountant), &j.id).unwrap()
    };
    assert_eq!(original.status, "reversed");
    assert_eq!(original.reversed_by.map(|r| r.id), Some(rev.id.clone()));
    assert_eq!(account_balance(&kit, accounts::BANK, "2026-10-17"), 0);
    let statuses: Vec<String> = kit
        .conn()
        .prepare("SELECT status FROM journal_entries WHERE source_type = 'manual_journal' ORDER BY created_at")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(statuses, ["reversed", "posted"]);
    for action in [
        "JOURNAL_DRAFTED",
        "JOURNAL_SUBMITTED",
        "JOURNAL_APPROVED",
        "JOURNAL_POSTED",
        "JOURNAL_SENT_BACK",
    ] {
        assert!(audit_count(&kit, action) >= 1, "{action} audited");
    }
    kit.assert_books_balance();
}

#[test]
fn control_accounts_need_an_owner_and_keep_parties_in_step() {
    let kit = Kit::new();
    let manager = kit.user("manager");
    let owner = kit.owner();
    let customer = kit.party("Asha Traders", "customer", None);

    // Stock accounts are never posted by hand.
    let stock = draft(
        &kit,
        &manager,
        "stock",
        None,
        vec![
            line(accounts::INVENTORY, 500, 0),
            line(accounts::CASH, 0, 500),
        ],
    );
    assert_eq!(code(stock), ErrorCode::Validation);
    // A receivable line needs its customer.
    let no_party = draft(
        &kit,
        &manager,
        "noparty",
        None,
        vec![
            line(accounts::RECEIVABLE, 2_000, 0),
            line(accounts::SALES, 0, 2_000),
        ],
    );
    assert_eq!(code(no_party), ErrorCode::Validation);
    let mut ar = line(accounts::RECEIVABLE, 2_000, 0);
    ar.party_id = Some(customer.clone());
    let j = draft(
        &kit,
        &manager,
        "ar",
        None,
        vec![ar, line(accounts::SALES, 0, 2_000)],
    )
    .unwrap();
    assert!(j.uses_restricted && !j.allow_restricted);
    assert_eq!(
        code(act(&kit, &manager, journals::submit, &j.id, None)),
        ErrorCode::Validation
    );
    assert_eq!(
        code(act(
            &kit,
            &manager,
            journals::allow_restricted,
            &j.id,
            Some("Old credit sale missed in v1")
        )),
        ErrorCode::Forbidden
    );
    assert_eq!(
        code(act(&kit, &owner, journals::allow_restricted, &j.id, None)),
        ErrorCode::Validation,
        "a reason is required"
    );
    act(
        &kit,
        &owner,
        journals::allow_restricted,
        &j.id,
        Some("Old credit sale missed in v1"),
    )
    .unwrap();
    post_journal(&kit, &manager, &j.id);
    assert_eq!(kit.balance(&customer), 2_000, "the customer owes it now");
    kit.assert_books_balance();

    // The owner may approve a journal of their own; a manager may not approve at all.
    let mine = draft(
        &kit,
        &owner,
        "own",
        None,
        vec![
            line(accounts::GENERAL_EXPENSES, 300, 0),
            line(accounts::CASH, 0, 300),
        ],
    )
    .unwrap();
    act(&kit, &owner, journals::submit, &mine.id, None).unwrap();
    assert_eq!(
        code(act(&kit, &manager, journals::approve, &mine.id, None)),
        ErrorCode::Forbidden
    );
    act(&kit, &owner, journals::approve, &mine.id, None).unwrap();
    act(&kit, &owner, journals::post, &mine.id, None).unwrap();
    kit.assert_books_balance();
}

// ---------------------------------------------------------------- locked days and years

#[test]
fn the_closed_day_lock_refuses_back_dated_documents() {
    let kit = Kit::new();
    let owner = kit.owner();
    let manager = kit.user("manager");
    let cashier = kit.user("cashier");
    let accountant = kit.user("accountant");
    let rice = kit.product("Rice", "RICE", 10_000, 6_000, 50, None);

    // Only the owner turns the lock on or off.
    assert_eq!(
        code(set_accounting(
            &kit,
            &manager,
            json!({ "lockClosedDays": true })
        )),
        ErrorCode::Forbidden
    );
    set_accounting(&kit, &owner, json!({ "lockClosedDays": true })).unwrap();
    let early = sell(
        &kit,
        &owner,
        &sale("early", &rice, None, Some("2026-10-16"), Some(10_000)),
    )
    .unwrap();
    {
        let mut conn = kit.conn();
        accounting::close_day(
            &mut kit.ctx(&mut conn, &owner),
            &DayInput {
                business_date: Some("2026-10-16".into()),
                notes: None,
            },
        )
        .unwrap();
    }
    let mut conn = kit.conn();
    let status = period::status(&kit.ctx(&mut conn, &owner)).unwrap();
    assert_eq!(status.locked_through.as_deref(), Some("2026-10-16"));
    drop(conn);

    // Today is open; the closed day needs period.override (with a manager's approval).
    sell(
        &kit,
        &cashier,
        &sale("today", &rice, None, None, Some(10_000)),
    )
    .unwrap();
    let back = sale("back", &rice, None, Some("2026-10-16"), Some(10_000));
    assert_eq!(
        code(sell(&kit, &cashier, &back)),
        ErrorCode::ApprovalRequired
    );
    {
        let approver = kit.approver(&manager);
        let mut conn = kit.conn();
        let mut ctx = kit.ctx(&mut conn, &cashier);
        ctx.approver = Some(&approver);
        sales::create_invoice(&mut ctx, &back).unwrap();
    }
    let (count, approved_by): (i64, Option<String>) = kit
        .conn()
        .query_row(
            "SELECT count(*), max(approved_by) FROM audit_log WHERE action = 'PERIOD_OVERRIDE'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((count, approved_by), (1, Some(manager.user_id.clone())));

    // Every kind of document is held back the same way.
    assert_eq!(
        code(expense(
            &kit,
            &accountant,
            "back",
            500,
            Some("2026-10-16"),
            cash(500)
        )),
        ErrorCode::ApprovalRequired
    );
    let jv = draft(
        &kit,
        &accountant,
        "back",
        Some("2026-10-16"),
        vec![
            line(accounts::GENERAL_EXPENSES, 100, 0),
            line(accounts::CASH, 0, 100),
        ],
    )
    .unwrap();
    act(&kit, &accountant, journals::submit, &jv.id, None).unwrap();
    act(&kit, &owner, journals::approve, &jv.id, None).unwrap();
    assert_eq!(
        code(act(&kit, &accountant, journals::post, &jv.id, None)),
        ErrorCode::ApprovalRequired
    );
    {
        let mut conn = kit.conn();
        let void = sales::void_invoice(
            &mut kit.ctx(&mut conn, &accountant),
            &VoidInput {
                id: early.id.clone(),
                reason: "Wrong customer".into(),
            },
        );
        assert!(matches!(
            code(void),
            ErrorCode::ApprovalRequired | ErrorCode::Forbidden
        ));
    }
    // The owner holds period.override: allowed, and audited.
    expense(
        &kit,
        &owner,
        "owner-back",
        500,
        Some("2026-10-16"),
        cash(500),
    )
    .unwrap();
    assert_eq!(audit_count(&kit, "PERIOD_OVERRIDE"), 2);

    // Reopened by the owner (with a reason), the day takes documents again; then it closes again.
    {
        let mut conn = kit.conn();
        let reopen = period::reopen_day(
            &mut kit.ctx(&mut conn, &manager),
            &ReopenDayInput {
                business_date: "2026-10-16".into(),
                reason: "Missed a supplier bill".into(),
            },
        );
        assert_eq!(code(reopen), ErrorCode::Forbidden);
        period::reopen_day(
            &mut kit.ctx(&mut conn, &owner),
            &ReopenDayInput {
                business_date: "2026-10-16".into(),
                reason: "Missed a supplier bill".into(),
            },
        )
        .unwrap();
    }
    act(&kit, &accountant, journals::post, &jv.id, None).unwrap();
    assert_eq!(
        audit_count(&kit, "PERIOD_OVERRIDE"),
        2,
        "no override needed in a reopened day"
    );
    {
        let mut conn = kit.conn();
        let summary = accounting::close_day(
            &mut kit.ctx(&mut conn, &owner),
            &DayInput {
                business_date: Some("2026-10-16".into()),
                notes: Some("Closed again".into()),
            },
        )
        .unwrap();
        assert!(summary.closed_at.is_some() && !summary.reopened && summary.locked);
    }
    assert_eq!(audit_count(&kit, "DAY_REOPENED"), 1);
    kit.assert_books_balance();
}

#[test]
fn closing_a_year_moves_its_profit_to_retained_earnings_and_locks_it() {
    let kit = Kit::new();
    let owner = kit.owner();
    let accountant = kit.user("accountant");
    // Last year (FY 25-26): ₹500 of income, ₹200 of expenses.
    let income = draft(
        &kit,
        &accountant,
        "income",
        Some("2026-03-10"),
        vec![
            line(accounts::BANK, 50_000, 0),
            line(accounts::SALES, 0, 50_000),
        ],
    )
    .unwrap();
    post_journal(&kit, &accountant, &income.id);
    expense(
        &kit,
        &owner,
        "march",
        20_000,
        Some("2026-03-20"),
        cash(20_000),
    )
    .unwrap();

    let mut conn = kit.conn();
    let years = period::years(&kit.ctx(&mut conn, &owner)).unwrap();
    let last = years.iter().find(|y| y.fy_label == "25-26").unwrap();
    assert!(last.can_close && last.status == "open" && last.net_profit == 30_000);
    let this = years.iter().find(|y| y.fy_label == "26-27").unwrap();
    assert!(!this.can_close, "the current year hasn't ended");
    let preview = period::year_close_preview(
        &kit.ctx(&mut conn, &owner),
        &YearPreviewInput {
            end_date: "2026-03-31".into(),
        },
    )
    .unwrap();
    assert_eq!((preview.net_profit, preview.blockers.len()), (30_000, 0));
    let close = |expected: i64, conn: &mut crate::app::Conn| {
        period::close_year(
            &mut kit.ctx(conn, &owner),
            &YearCloseInput {
                end_date: "2026-03-31".into(),
                expected_net_profit: expected,
                reason: Some("Books audited".into()),
            },
        )
    };
    assert_eq!(code(close(29_000, &mut conn)), ErrorCode::Conflict);
    assert_eq!(
        code(period::close_year(
            &mut kit.ctx(&mut conn, &accountant),
            &YearCloseInput {
                end_date: "2026-03-31".into(),
                expected_net_profit: 30_000,
                reason: None,
            },
        )),
        ErrorCode::Forbidden
    );
    let closed = close(30_000, &mut conn).unwrap();
    assert_eq!(closed.status, "closed");
    drop(conn);

    // The year's own profit and loss still shows its profit; its income and expense accounts are
    // empty at the year end and the profit sits in Retained Earnings.
    let pl = run_report(
        &kit,
        ReportQuery {
            report: "profit_loss".into(),
            from: Some("2025-04-01".into()),
            to: Some("2026-03-31".into()),
            ..ReportQuery::default()
        },
    )
    .unwrap();
    let net = pl
        .rows
        .iter()
        .find(|r| r[2] == json!("Net profit"))
        .unwrap();
    assert_eq!(net[3], json!(30_000));
    assert_eq!(account_balance(&kit, accounts::SALES, "2026-03-31"), 0);
    assert_eq!(
        account_balance(&kit, accounts::RETAINED_EARNINGS, "2026-03-31"),
        -30_000
    );
    kit.assert_books_balance();

    // The whole year is locked, closed-day lock or not.
    assert_eq!(
        code(expense(
            &kit,
            &accountant,
            "late",
            1_000,
            Some("2026-03-25"),
            cash(1_000)
        )),
        ErrorCode::ApprovalRequired
    );
    // Reopened: the closing entry is reversed and documents can go in again.
    let mut conn = kit.conn();
    let reopened = period::reopen_year(
        &mut kit.ctx(&mut conn, &owner),
        &YearReopenInput {
            end_date: "2026-03-31".into(),
            reason: "Late supplier bill".into(),
        },
    )
    .unwrap();
    assert_eq!(reopened.status, "reopened");
    drop(conn);
    assert_eq!(
        account_balance(&kit, accounts::RETAINED_EARNINGS, "2026-03-31"),
        0
    );
    expense(
        &kit,
        &accountant,
        "late",
        1_000,
        Some("2026-03-25"),
        cash(1_000),
    )
    .unwrap();
    let mut conn = kit.conn();
    let again = close(29_000, &mut conn).unwrap();
    assert_eq!(
        (again.status.as_str(), again.net_profit),
        ("closed", 29_000)
    );
    drop(conn);
    assert_eq!(audit_count(&kit, "YEAR_CLOSED"), 2);
    assert_eq!(audit_count(&kit, "YEAR_REOPENED"), 1);
    kit.assert_books_balance();
}

// ---------------------------------------------------------------- bank reconciliation

const STATEMENT: &str = "Statement of account no. 50100012345\r\n\
Txn Date,Description,Ref No,Withdrawal,Deposit,Balance\r\n\
16/10/2026,NEFT-UTR123456-ASHA TRADERS,UTR123456,,\"25,000.00\",\"25,000.00\"\r\n\
17/10/2026,CHQ PAID 000451,000451,75.00,,\"24,925.00\"\r\n\
17/10/2026,SMS CHARGES,,5.90,,\"24,919.10\"\r\n";

fn mapping() -> StatementMapping {
    StatementMapping {
        skip_rows: 1,
        has_header: true,
        date_column: 0,
        date_format: "DD/MM/YYYY".into(),
        description_column: 1,
        reference_column: Some(2),
        amount_column: None,
        withdrawal_column: Some(3),
        deposit_column: Some(4),
        balance_column: Some(5),
    }
}

fn import(
    kit: &Kit,
    actor: &Actor,
    csv: &str,
    mapping: Option<StatementMapping>,
    apply: bool,
) -> bank::StatementImportResult {
    let mut conn = kit.conn();
    bank::import(
        &mut kit.ctx(&mut conn, actor),
        &StatementImportInput {
            account_id: accounts::BANK.into(),
            file_name: "october.csv".into(),
            csv: csv.into(),
            mapping,
            save_mapping: true,
            apply,
        },
    )
    .unwrap()
}

fn reconcile(kit: &Kit, actor: &Actor, day: &str) -> bank::Reconciliation {
    let mut conn = kit.conn();
    bank::reconciliation(
        &kit.ctx(&mut conn, actor),
        &ReconcileQuery {
            account_id: accounts::BANK.into(),
            statement_date: day.into(),
            statement_balance: None,
        },
    )
    .unwrap()
}

fn view(kit: &Kit, actor: &Actor) -> bank::BankView {
    let mut conn = kit.conn();
    bank::view(
        &kit.ctx(&mut conn, actor),
        &BankQuery {
            account_id: accounts::BANK.into(),
            from: None,
            to: None,
        },
    )
    .unwrap()
}

#[test]
fn a_bank_statement_is_imported_matched_and_reconciled() {
    let kit = Kit::new();
    let owner = kit.owner();
    let accountant = kit.user("accountant");
    let customer = kit.party("Asha Traders", "customer", None);
    let supplier = kit.party("Kumar Wholesale", "supplier", None);
    record(
        &kit,
        &owner,
        "utr",
        &customer,
        "in",
        bank_payment(2_500_000, "bank_transfer", Some("UTR123456")),
        Some("2026-10-16"),
    );
    record(
        &kit,
        &owner,
        "utr2",
        &customer,
        "in",
        bank_payment(1_000_000, "bank_transfer", Some("UTR777")),
        None,
    );
    record(
        &kit,
        &owner,
        "chq",
        &supplier,
        "out",
        bank_payment(7_500, "cheque", Some("000451")),
        None,
    );

    // Without a mapping only the sample comes back; with it, a preview; then the import.
    let sample = import(&kit, &accountant, STATEMENT, None, false);
    assert_eq!(sample.sample.len(), 5);
    assert!(sample.rows.is_empty() && !sample.imported);
    let preview = import(&kit, &accountant, STATEMENT, Some(mapping()), false);
    assert_eq!(
        (preview.total_rows, preview.new_rows, preview.errors),
        (3, 3, 0)
    );
    assert_eq!(preview.columns[1], "Description");
    assert_eq!(preview.rows[0].amount, Some(2_500_000));
    assert_eq!(preview.rows[2].amount, Some(-590));
    assert_eq!(preview.rows[2].balance, Some(2_491_910));
    let done = import(&kit, &accountant, STATEMENT, Some(mapping()), true);
    assert!(done.imported);
    // The same file again adds nothing; the saved mapping is used when none is given.
    let again = import(&kit, &accountant, STATEMENT, None, true);
    assert_eq!((again.new_rows, again.duplicates), (0, 3));

    // Auto-match: the deposit by its UTR and amount, the cheque by number; the charges are not
    // in the books yet.
    let matched = {
        let mut conn = kit.conn();
        bank::auto_match(
            &mut kit.ctx(&mut conn, &accountant),
            &AccountInput {
                account_id: accounts::BANK.into(),
            },
        )
        .unwrap()
    };
    assert_eq!(
        (matched.matched, matched.ambiguous, matched.unmatched),
        (2, 0, 1)
    );
    let r = reconcile(&kit, &accountant, "2026-10-17");
    assert_eq!(r.book_balance, 3_492_500);
    assert_eq!(
        (
            r.uncleared_in_total,
            r.uncleared_out_total,
            r.not_in_books_total
        ),
        (1_000_000, 0, -590)
    );
    assert_eq!(r.statement_balance, Some(2_491_910));
    assert_eq!(r.difference, Some(0));

    // The charges are booked and matched by hand; the statement still agrees.
    expense(
        &kit,
        &owner,
        "sms",
        590,
        None,
        bank_payment(590, "bank_transfer", None),
    )
    .unwrap();
    let v = view(&kit, &accountant);
    let charge_line = v
        .statement_lines
        .iter()
        .find(|l| l.amount == -590)
        .unwrap()
        .id
        .clone();
    let book_line = v
        .book_lines
        .iter()
        .find(|b| b.amount == -590 && b.cleared_date.is_none())
        .unwrap()
        .journal_line_id
        .clone();
    {
        let mut conn = kit.conn();
        let wrong = bank::match_lines(
            &mut kit.ctx(&mut conn, &accountant),
            &MatchInput {
                statement_line_id: charge_line.clone(),
                journal_line_ids: vec![v
                    .book_lines
                    .iter()
                    .find(|b| b.amount == 1_000_000)
                    .unwrap()
                    .journal_line_id
                    .clone()],
            },
        );
        assert_eq!(code(wrong), ErrorCode::Validation);
        bank::match_lines(
            &mut kit.ctx(&mut conn, &accountant),
            &MatchInput {
                statement_line_id: charge_line,
                journal_line_ids: vec![book_line],
            },
        )
        .unwrap();
    }
    let r = reconcile(&kit, &accountant, "2026-10-17");
    assert_eq!((r.not_in_books_total, r.difference), (0, Some(0)));
    let record = {
        let mut conn = kit.conn();
        bank::finish(
            &mut kit.ctx(&mut conn, &accountant),
            &FinishInput {
                account_id: accounts::BANK.into(),
                statement_date: "2026-10-17".into(),
                statement_balance: 2_491_910,
                notes: None,
            },
        )
        .unwrap()
    };
    assert_eq!((record.difference, record.uncleared_in), (0, 1_000_000));

    // Unmatching the deposit moves it to both sides at once: still reconciled.
    let deposit = view(&kit, &accountant)
        .statement_lines
        .into_iter()
        .find(|l| l.amount == 2_500_000)
        .unwrap();
    {
        let mut conn = kit.conn();
        bank::unmatch(
            &mut kit.ctx(&mut conn, &accountant),
            &UnmatchInput {
                statement_line_id: Some(deposit.id.clone()),
                journal_line_id: None,
            },
        )
        .unwrap();
    }
    let r = reconcile(&kit, &accountant, "2026-10-17");
    assert_eq!(
        (r.uncleared_in_total, r.not_in_books_total, r.difference),
        (3_500_000, 2_500_000, Some(0))
    );

    // Ticking a book line clears it without a statement line.
    let utr777 = view(&kit, &accountant)
        .book_lines
        .into_iter()
        .find(|b| b.reference.as_deref() == Some("UTR777"))
        .unwrap();
    {
        let mut conn = kit.conn();
        bank::tick(
            &mut kit.ctx(&mut conn, &accountant),
            &TickInput {
                account_id: accounts::BANK.into(),
                journal_line_ids: vec![utr777.journal_line_id.clone()],
                cleared_date: "2026-10-17".into(),
            },
        )
        .unwrap();
    }
    let ticked = view(&kit, &accountant)
        .book_lines
        .into_iter()
        .find(|b| b.journal_line_id == utr777.journal_line_id)
        .unwrap();
    assert_eq!(ticked.method.as_deref(), Some("ticked"));
    kit.assert_books_balance();
}

#[test]
fn matching_leaves_ties_to_a_person_and_voided_pairs_out() {
    let kit = Kit::new();
    let owner = kit.owner();
    let customer = kit.party("Asha Traders", "customer", None);
    record(
        &kit,
        &owner,
        "a",
        &customer,
        "in",
        bank_payment(300_000, "bank_transfer", None),
        None,
    );
    record(
        &kit,
        &owner,
        "b",
        &customer,
        "in",
        bank_payment(300_000, "bank_transfer", None),
        None,
    );
    let voided = record(
        &kit,
        &owner,
        "v",
        &customer,
        "in",
        bank_payment(123_400, "upi", None),
        None,
    );
    {
        let mut conn = kit.conn();
        payments::void(
            &mut kit.ctx(&mut conn, &owner),
            &VoidPaymentInput {
                id: voided.id,
                reason: "Entered twice".into(),
            },
        )
        .unwrap();
    }
    let csv = "Date,Narration,Amount\n17/10/2026,IMPS IN,3000.00\n";
    let m = StatementMapping {
        skip_rows: 0,
        has_header: true,
        date_column: 0,
        date_format: "DD/MM/YYYY".into(),
        description_column: 1,
        reference_column: None,
        amount_column: Some(2),
        withdrawal_column: None,
        deposit_column: None,
        balance_column: None,
    };
    import(&kit, &owner, csv, Some(m), true);
    let result = {
        let mut conn = kit.conn();
        bank::auto_match(
            &mut kit.ctx(&mut conn, &owner),
            &AccountInput {
                account_id: accounts::BANK.into(),
            },
        )
        .unwrap()
    };
    assert_eq!((result.matched, result.ambiguous), (0, 1));
    let v = view(&kit, &owner);
    assert_eq!(
        v.book_lines.len(),
        2,
        "the voided payment and its reversal stay out"
    );
    assert!(v.book_lines.iter().all(|b| b.amount == 300_000));
    // A bad file is refused line by line before anything is imported.
    let mut conn = kit.conn();
    let bad = bank::import(
        &mut kit.ctx(&mut conn, &owner),
        &StatementImportInput {
            account_id: accounts::BANK.into(),
            file_name: "bad.csv".into(),
            csv: "Date,Narration,Amount\n32/10/2026,X,1\n17/10/2026,Y,abc\n".into(),
            mapping: None,
            save_mapping: false,
            apply: true,
        },
    );
    assert_eq!(code(bad), ErrorCode::Validation);
    let cash_account = bank::import(
        &mut kit.ctx(&mut conn, &owner),
        &StatementImportInput {
            account_id: accounts::CASH.into(),
            file_name: "x.csv".into(),
            csv: csv.into(),
            mapping: None,
            save_mapping: false,
            apply: false,
        },
    );
    assert_eq!(
        code(cash_account),
        ErrorCode::Validation,
        "only bank accounts"
    );
}

// ---------------------------------------------------------------- cheques

#[test]
fn post_dated_cheques_wait_clear_and_bounce() {
    let kit = Kit::new();
    let owner = kit.owner();
    let cashier = kit.user("cashier");
    let rice = kit.product("Rice", "RICE", 10_000, 6_000, 50, None);
    let customer = kit.party("Asha Traders", "customer", None);
    let invoice = sell(
        &kit,
        &owner,
        &sale("credit", &rice, Some(&customer), None, None),
    )
    .unwrap();
    let total = invoice.total;
    let save = |actor: &Actor, key: &str, day: &str| {
        let mut conn = kit.conn();
        cheques::save(
            &mut kit.ctx(&mut conn, actor),
            &ChequeInput {
                id: None,
                expected_updated_at: None,
                idempotency_key: Some(format!("chq-{key}-00001")),
                direction: "in".into(),
                party_id: customer.clone(),
                account_id: accounts::BANK.into(),
                cheque_no: format!("{key}0451"),
                bank_name: Some("Canara Bank".into()),
                cheque_date: day.into(),
                amount: total,
                notes: None,
            },
        )
    };
    let chq = save(&cashier, "a", "2026-10-20").unwrap();
    assert_eq!((chq.status.as_str(), chq.days_left), ("pending", 3));
    assert_eq!(
        kit.balance(&customer),
        total,
        "a pending cheque is not money"
    );
    let other = save(&owner, "b", "2026-10-25").unwrap();
    let clear = |actor: &Actor, id: &str| {
        let mut conn = kit.conn();
        cheques::clear(
            &mut kit.ctx(&mut conn, actor),
            &ChequeClearInput {
                id: id.into(),
                cleared_date: None,
                auto_allocate: true,
            },
        )
    };
    assert_eq!(
        code(clear(&owner, &chq.id)),
        ErrorCode::Validation,
        "not before its date"
    );

    // On the 21st it is due: a notice, then the deposit clears.
    kit.clock.advance(chrono::Duration::days(4));
    jobs::tick(&kit.state).unwrap();
    let notices: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM notifications WHERE kind = 'cheque_due'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(notices, 1);
    let mut conn = kit.conn();
    let due = cheques::list(
        &kit.ctx(&mut conn, &owner),
        &ChequeQuery {
            due_by: Some("2026-10-21".into()),
            ..ChequeQuery::default()
        },
    )
    .unwrap();
    assert_eq!(due.items.len(), 1);
    drop(conn);
    let cleared = clear(&cashier, &chq.id).unwrap();
    assert_eq!(cleared.status, "cleared");
    assert_eq!(kit.balance(&customer), 0);
    let (mode, reference): (String, String) = kit
        .conn()
        .query_row(
            "SELECT mode, reference_no FROM payments WHERE id = ?1",
            [cleared.payment_id.as_deref().unwrap()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((mode.as_str(), reference.as_str()), ("cheque", "a0451"));
    kit.assert_books_balance();

    // It bounces: the payment is voided and the customer owes the bill again.
    let mut conn = kit.conn();
    let bounced = cheques::bounce(
        &mut kit.ctx(&mut conn, &owner),
        &ChequeStatusInput {
            id: chq.id.clone(),
            reason: "Insufficient funds".into(),
        },
    )
    .unwrap();
    assert_eq!(bounced.status, "bounced");
    drop(conn);
    assert_eq!(kit.balance(&customer), total);
    let mut conn = kit.conn();
    cheques::cancel(
        &mut kit.ctx(&mut conn, &owner),
        &ChequeStatusInput {
            id: other.id,
            reason: "Customer paid in cash".into(),
        },
    )
    .unwrap();
    let again = cheques::cancel(
        &mut kit.ctx(&mut conn, &owner),
        &ChequeStatusInput {
            id: chq.id,
            reason: "x".into(),
        },
    );
    assert_eq!(code(again), ErrorCode::Conflict);
    drop(conn);
    kit.assert_books_balance();
}

// ---------------------------------------------------------------- till differences

#[test]
fn a_till_difference_needs_a_reason_and_a_manager_above_the_limit() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let manager = kit.user("manager");
    let owner = kit.owner();
    let open = || {
        let mut conn = kit.conn();
        payments::open_register(
            &mut kit.ctx(&mut conn, &cashier),
            &OpenRegisterInput {
                register_id: None,
                opening_float: 0,
            },
        )
        .unwrap()
    };
    let close = |session: &str,
                 counted: i64,
                 reason: Option<&str>,
                 notes: Option<&str>,
                 approver: Option<&Actor>| {
        let approver = approver.map(|a| kit.approver(a));
        let mut conn = kit.conn();
        let mut ctx = kit.ctx(&mut conn, &cashier);
        ctx.approver = approver.as_ref();
        payments::close_register(
            &mut ctx,
            &CloseRegisterInput {
                session_id: session.into(),
                counted_cash: counted,
                notes: notes.map(String::from),
                difference_reason: reason.map(String::from),
            },
        )
    };
    let s = open();
    assert_eq!(
        code(close(&s.id, 50_000, None, None, None)),
        ErrorCode::Validation
    );
    assert_eq!(
        code(close(&s.id, 50_000, Some("other"), None, None)),
        ErrorCode::Validation
    );
    assert_eq!(
        code(close(&s.id, 50_000, Some("counting_error"), None, None)),
        ErrorCode::ApprovalRequired,
        "₹500 over is above the ₹100 limit"
    );
    let closed = close(&s.id, 50_000, Some("counting_error"), None, Some(&manager)).unwrap();
    assert_eq!(closed.difference_reason.as_deref(), Some("counting_error"));
    assert_eq!(closed.difference_approved_by.as_deref(), Some("manager"));
    // A small difference needs only the reason.
    let s = open();
    let small = close(&s.id, 9_000, Some("wrong_change"), None, None).unwrap();
    assert_eq!(small.difference, Some(9_000));
    let mut conn = kit.conn();
    let day = accounting::day_summary(&kit.ctx(&mut conn, &owner), &DayInput::default()).unwrap();
    assert_eq!(day.tills.len(), 2);
    assert_eq!(day.tills[0].approved_by.as_deref(), Some("manager"));
    assert_eq!(
        day.tills[1].difference_reason.as_deref(),
        Some("wrong_change")
    );
}

// ---------------------------------------------------------------- audit export

#[test]
fn the_audit_trail_exports_with_the_viewers_filters() {
    let kit = Kit::new();
    let accountant = kit.user("accountant");
    let cashier = kit.user("cashier");
    let rice = kit.product("Rice", "RICE", 10_000, 6_000, 50, None);
    sell(
        &kit,
        &cashier,
        &sale("one", &rice, None, None, Some(10_000)),
    )
    .unwrap();
    sell(
        &kit,
        &cashier,
        &sale("two", &rice, None, None, Some(10_000)),
    )
    .unwrap();
    let mut conn = kit.conn();
    let filtered = accounting::export_audit(
        &mut kit.ctx(&mut conn, &accountant),
        &AuditQuery {
            action: Some("SALE_FINALIZED".into()),
            ..AuditQuery::default()
        },
    )
    .unwrap();
    assert_eq!(filtered.rows.len(), 2);
    assert!(!filtered.truncated);
    assert!(filtered.rows.iter().all(|r| r.action == "SALE_FINALIZED"));
    assert_eq!(
        code(accounting::export_audit(
            &mut kit.ctx(&mut conn, &cashier),
            &AuditQuery::default()
        )),
        ErrorCode::Forbidden
    );
    drop(conn);
    assert_eq!(audit_count(&kit, "AUDIT_EXPORTED"), 1);
}

// ---------------------------------------------------------------- reports

#[test]
fn reports_group_and_drill_down_to_the_invoices() {
    let kit = Kit::new();
    let owner = kit.owner();
    let rice = kit.product("Rice", "RICE", 10_000, 6_000, 50, None);
    let asha = kit.party("Asha Traders", "customer", None);
    let ravi = kit.party("Ravi Kirana", "customer", None);
    sell(
        &kit,
        &owner,
        &sale("a1", &rice, Some(&asha), None, Some(10_000)),
    )
    .unwrap();
    sell(
        &kit,
        &owner,
        &sale("a2", &rice, Some(&asha), None, Some(10_000)),
    )
    .unwrap();
    sell(
        &kit,
        &owner,
        &sale("r1", &rice, Some(&ravi), None, Some(10_000)),
    )
    .unwrap();
    sell(&kit, &owner, &sale("w1", &rice, None, None, Some(10_000))).unwrap();
    let q = |report: &str, group: Option<&str>| ReportQuery {
        report: report.into(),
        from: Some("2026-10-01".into()),
        to: Some("2026-10-17".into()),
        group_by: group.map(String::from),
        ..ReportQuery::default()
    };
    let by_customer = run_report(&kit, q("sales_summary", Some("customer"))).unwrap();
    assert_eq!(by_customer.group_by.as_deref(), Some("customer"));
    assert_eq!(by_customer.group_options.len(), 6);
    assert_eq!(by_customer.rows.len(), 3);
    assert_eq!(by_customer.rows[0][0], json!("Asha Traders"));
    assert_eq!(by_customer.rows[0][1], json!(2));
    let walk_in = by_customer
        .links
        .iter()
        .flatten()
        .find(|l| l.party_id.as_deref() == Some("walk_in"))
        .unwrap();
    assert_eq!(walk_in.report.as_deref(), Some("sales_invoices"));
    let invoices = run_report(
        &kit,
        ReportQuery {
            party_id: walk_in.party_id.clone(),
            ..q("sales_invoices", None)
        },
    )
    .unwrap();
    assert_eq!(invoices.rows.len(), 1);
    assert_eq!(
        invoices.links[0].as_ref().map(|l| l.kind.as_str()),
        Some("sales_invoice")
    );
    let by_day = run_report(&kit, q("sales_summary", None)).unwrap();
    assert_eq!(by_day.links.len(), by_day.rows.len());
    let by_month = run_report(&kit, q("sales_summary", Some("month"))).unwrap();
    assert_eq!(by_month.rows[0][0], json!("2026-10"));
    let link = by_month.links[0].as_ref().unwrap();
    assert_eq!(
        (link.from.as_deref(), link.to.as_deref()),
        (Some("2026-10-01"), Some("2026-10-17"))
    );
    let by_item = run_report(&kit, q("sales_summary", Some("item"))).unwrap();
    let item_link = by_item.links[0].as_ref().unwrap();
    let rice_bills = run_report(
        &kit,
        ReportQuery {
            variant_id: item_link.variant_id.clone(),
            ..q("sales_invoices", None)
        },
    )
    .unwrap();
    assert_eq!(rice_bills.rows.len(), 4);
    let categories = run_report(&kit, q("sales_by_item", Some("category"))).unwrap();
    assert_eq!(
        categories.links[0]
            .as_ref()
            .and_then(|l| l.group_by.as_deref()),
        Some("item")
    );
    assert_eq!(
        code(run_report(&kit, q("sales_summary", Some("supplier")))),
        ErrorCode::Validation
    );
    let by_cashier = run_report(&kit, q("sales_summary", Some("user"))).unwrap();
    assert_eq!(by_cashier.rows[0][1], json!(4));
}

#[test]
fn saved_views_export_on_schedule_as_their_owner() {
    let kit = Kit::new();
    let owner = kit.owner();
    let cashier = kit.user("cashier");
    let accountant = kit.user("accountant");
    let rice = kit.product("Rice", "RICE", 10_000, 6_000, 50, None);
    sell(
        &kit,
        &owner,
        &sale("y", &rice, None, Some("2026-10-16"), Some(10_000)),
    )
    .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut conn = kit.conn();
    let save = |conn: &mut crate::app::Conn, name: &str| {
        report_views::save(
            &mut kit.ctx(conn, &owner),
            &ReportViewInput {
                id: None,
                expected_updated_at: None,
                report: "sales_summary".into(),
                name: name.into(),
                params: ViewParams {
                    period: "month".into(),
                    group_by: Some("customer".into()),
                    ..ViewParams::default()
                },
            },
        )
    };
    let view = save(&mut conn, "Daily sales by customer").unwrap();
    assert_eq!(
        code(save(&mut conn, "daily sales by customer")),
        ErrorCode::Validation
    );
    assert_eq!(
        report_views::list(&kit.ctx(&mut conn, &accountant))
            .unwrap()
            .len(),
        0,
        "views are per user"
    );
    let schedule = |conn: &mut crate::app::Conn, actor: &Actor| {
        report_views::set_schedule(
            &mut kit.ctx(conn, actor),
            &ScheduleInput {
                view_id: view.id.clone(),
                frequency: Some("daily".into()),
                folder: Some(folder.path().to_string_lossy().into_owned()),
                is_active: true,
            },
        )
    };
    assert!(matches!(
        code(schedule(&mut conn, &cashier)),
        ErrorCode::Forbidden
    ));
    schedule(&mut conn, &owner).unwrap();
    let settings = settings::load(&conn).unwrap();
    let now = kit.now().to_rfc3339();
    assert_eq!(
        report_views::run_due(&conn, &settings, &now, "2026-10-17").unwrap(),
        (1, 0)
    );
    assert_eq!(
        report_views::run_due(&conn, &settings, &now, "2026-10-17").unwrap(),
        (0, 0),
        "once per period"
    );
    let file = folder.path().join("Daily sales by customer 2026-10-16.csv");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains("Customer,Bills,Taxable value"), "{text}");
    assert!(text.contains("Walk-in customers,1,100,"), "{text}");
    // A folder that disappears fails that run, says so and stops nothing else.
    let gone = folder.path().to_path_buf();
    drop(folder);
    assert!(!gone.exists());
    assert_eq!(
        report_views::run_due(&conn, &settings, &now, "2026-10-18").unwrap(),
        (0, 1)
    );
    let notices: i64 = conn
        .query_row(
            "SELECT count(*) FROM notifications WHERE kind = 'report_export'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(notices, 1);
}

// ---------------------------------------------------------------- permission matrix

#[test]
fn every_new_action_checks_its_permission() {
    let kit = Kit::new();
    let roles = ["owner", "manager", "cashier", "stock_keeper", "accountant"];
    type Action = (
        &'static str,
        &'static [&'static str],
        fn(&mut Ctx<'_>) -> Result<(), AppError>,
    );
    let actions: Vec<Action> = vec![
        ("journals.save", &["journal.manage"], |ctx| {
            journals::save(
                ctx,
                &JournalInput {
                    id: Some("01920000-0000-7000-9999-000000000000".into()),
                    expected_updated_at: None,
                    idempotency_key: None,
                    business_date: None,
                    narration: "x".into(),
                    lines: vec![line(accounts::CASH, 1, 0), line(accounts::BANK, 0, 1)],
                },
            )
            .map(drop)
        }),
        ("journals.approve", &["journal.approve"], |ctx| {
            journals::approve(
                ctx,
                &JournalActionInput {
                    id: "01920000-0000-7000-9999-000000000000".into(),
                    expected_updated_at: None,
                    note: None,
                },
            )
            .map(drop)
        }),
        (
            "journals.allow_restricted",
            &["journal.restricted"],
            |ctx| {
                journals::allow_restricted(
                    ctx,
                    &JournalActionInput {
                        id: "01920000-0000-7000-9999-000000000000".into(),
                        expected_updated_at: None,
                        note: Some("x".into()),
                    },
                )
                .map(drop)
            },
        ),
        (
            "journals.list",
            &["journal.manage", "journal.approve", "accounting.view"],
            |ctx| journals::list(ctx, &Default::default()).map(drop),
        ),
        ("day.reopen", &["period.manage"], |ctx| {
            period::reopen_day(
                ctx,
                &ReopenDayInput {
                    business_date: "2026-10-01".into(),
                    reason: "x".into(),
                },
            )
        }),
        ("year.preview", &["period.manage"], |ctx| {
            period::year_close_preview(
                ctx,
                &YearPreviewInput {
                    end_date: "2026-03-31".into(),
                },
            )
            .map(drop)
        }),
        ("audit.export", &["audit.export"], |ctx| {
            accounting::export_audit(ctx, &AuditQuery::default()).map(drop)
        }),
        ("bank.view", &["bank.reconcile"], |ctx| {
            bank::view(
                ctx,
                &BankQuery {
                    account_id: accounts::BANK.into(),
                    from: None,
                    to: None,
                },
            )
            .map(drop)
        }),
        ("bank.history", &["bank.reconcile"], |ctx| {
            bank::history(
                ctx,
                &AccountInput {
                    account_id: accounts::BANK.into(),
                },
            )
            .map(drop)
        }),
        (
            "cheques.list",
            &["payments.receive", "payments.pay"],
            |ctx| cheques::list(ctx, &ChequeQuery::default()).map(drop),
        ),
        ("report_views.list", &["reports.view"], |ctx| {
            report_views::list(ctx).map(drop)
        }),
    ];
    for role in roles {
        let actor = kit.user(role);
        for (name, needs, action) in &actions {
            let mut conn = kit.conn();
            let mut ctx = kit.ctx(&mut conn, &actor);
            let result = action(&mut ctx);
            let forbidden = matches!(&result, Err(e) if e.code == ErrorCode::Forbidden);
            let allowed = needs.iter().any(|p| actor.can(p));
            assert_eq!(forbidden, !allowed, "{role} → {name}: {result:?}");
        }
    }
    // The period override and schedules are owner/manager matters.
    let manager = kit.user("manager");
    let accountant = kit.user("accountant");
    assert!(manager.can("period.override") && !accountant.can("period.override"));
    assert!(!manager.can("journal.approve") && !manager.can("period.manage"));
}
