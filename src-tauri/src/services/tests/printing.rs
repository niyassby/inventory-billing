//! Printing on the hub: the print queue (with a fake network printer), retries, who may print
//! what, the drawer approval, label templates and items, and in-store barcodes.

use std::io::Read;
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use crate::error::ErrorCode;
use crate::printing::barcode::{self, Symbology};
use crate::printing::config::{PrinterConfig, PrinterKind, PrinterProfile};
use crate::printing::labels::LabelTemplate;
use crate::printing::{self, PrintDocument};
use crate::services::catalog::{self, ProductInput, UnitConversionInput, VariantInput};
use crate::services::printing::{
    self as queue, DrawerInput, GenerateBarcodesInput, LabelPreviewInput, LabelSourceInput,
    PrintJobQuery, SubmitPrintJobInput,
};
use crate::services::purchases::{self, GoodsReceiptInput, PurchaseLineInput};
use crate::services::sales::{self, InvoiceDetail, PaymentInput, SaleInput, SaleLineInput};
use crate::services::settings;
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

/// A printer on 127.0.0.1 that hands every job it receives to the test.
struct FakePrinter {
    port: u16,
    jobs: mpsc::Receiver<Vec<u8>>,
}

impl FakePrinter {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, jobs) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut job = Vec::new();
                let _ = stream.read_to_end(&mut job);
                if tx.send(job).is_err() {
                    break;
                }
            }
        });
        Self { port, jobs }
    }

    fn next_job(&self) -> Option<Vec<u8>> {
        self.jobs.recv_timeout(Duration::from_secs(3)).ok()
    }

    fn nothing_more(&self) -> bool {
        self.jobs.recv_timeout(Duration::from_millis(300)).is_err()
    }
}

fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn hub_printer(port: u16, shared: bool) -> PrinterProfile {
    serde_json::from_value(serde_json::json!({
        "id": "hub-front", "name": "Hub front", "kind": "receipt",
        "connection": { "type": "network", "host": "127.0.0.1", "port": port },
        "paper": "mm80", "rupee": "text", "pictureFallback": false, "shared": shared
    }))
    .unwrap()
}

fn configure(kit: &Kit, printer: PrinterProfile) {
    PrinterConfig {
        printers: vec![printer],
        ..PrinterConfig::default()
    }
    .validated()
    .unwrap()
    .save(&queue::data_dir(&kit.state))
    .unwrap();
}

fn cash_sale(kit: &Kit, key: &str) -> InvoiceDetail {
    let gst = kit.gst_rate(18);
    let rice = kit.product(
        "Basmati Rice",
        &format!("RICE-{key}"),
        10_000,
        7_000,
        10,
        Some(&gst),
    );
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    sales::create_invoice(
        &mut kit.ctx(&mut conn, &cashier),
        &SaleInput {
            idempotency_key: format!("sale-{key}-000001"),
            warehouse_id: None,
            party_id: None,
            customer_name: None,
            customer_phone: None,
            customer_gstin: None,
            place_of_supply: None,
            business_date: None,
            prices_include_tax: None,
            lines: vec![SaleLineInput {
                variant_id: rice,
                unit_id: None,
                qty: 2_000,
                unit_price: None,
                discount: None,
                batch_id: None,
            }],
            bill_discount: None,
            payments: vec![PaymentInput {
                mode: "cash".into(),
                amount: 23_600,
                tendered: Some(25_000),
                account_id: None,
                reference: None,
            }],
            expected_total: Some(23_600),
            notes: None,
            source_document_id: None,
            register_session_id: None,
        },
    )
    .unwrap()
}

fn submit_input(key: &str, invoice: &InvoiceDetail) -> SubmitPrintJobInput {
    SubmitPrintJobInput {
        idempotency_key: key.into(),
        printer_id: "hub-front".into(),
        document: PrintDocument::Receipt {
            invoice: Box::new(invoice.clone()),
            reprint: false,
        },
    }
}

#[test]
fn the_hub_prints_a_counter_receipt_exactly_once() {
    let kit = Kit::new();
    let printer = FakePrinter::start();
    configure(&kit, hub_printer(printer.port, true));
    let invoice = cash_sale(&kit, "a");
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let input = submit_input("print-000001", &invoice);
    let job = queue::submit(&kit.state, &mut kit.ctx(&mut conn, &cashier), &input).unwrap();
    assert_eq!(job.status, "printed");
    assert_eq!(job.attempts, 1);
    assert_eq!(job.title, format!("Receipt {}", invoice.doc_no));

    // What the printer received is the receipt for exactly these figures, without a drawer pulse.
    let received = printer.next_job().expect("the printer got the job");
    let public = settings::public(&conn).unwrap();
    let expected = printing::render(
        &input.document,
        &hub_printer(printer.port, true),
        &public,
        kit.now(),
        None,
        false,
    )
    .unwrap();
    assert_eq!(received, expected);
    assert!(String::from_utf8_lossy(&received).contains("Rs.236.00"));

    // A replayed request (lost answer) returns the same job and prints nothing more.
    let again = queue::submit(&kit.state, &mut kit.ctx(&mut conn, &cashier), &input).unwrap();
    assert_eq!(again.id, job.id);
    assert!(printer.nothing_more(), "never printed twice");
}

#[test]
fn a_failed_job_waits_and_is_retried_when_the_printer_is_back() {
    let kit = Kit::new();
    configure(&kit, hub_printer(closed_port(), true));
    let invoice = cash_sale(&kit, "b");
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let job = queue::submit(
        &kit.state,
        &mut kit.ctx(&mut conn, &cashier),
        &submit_input("print-000002", &invoice),
    )
    .unwrap();
    assert_eq!(
        job.status, "failed",
        "the sale is never blocked: the job waits"
    );
    assert!(
        matches!(job.error_kind.as_deref(), Some("refused" | "unreachable")),
        "{job:?}"
    );
    let next = job
        .next_try_at
        .clone()
        .expect("an automatic retry is planned");
    assert_eq!(
        next,
        (kit.now() + chrono::Duration::minutes(1)).to_rfc3339()
    );

    // Not due yet.
    assert!(queue::retry_due(&kit.state).unwrap().starts_with("0 "));
    // The printer is fixed (new address), and a minute passes.
    let printer = FakePrinter::start();
    configure(&kit, hub_printer(printer.port, true));
    kit.clock.advance(chrono::Duration::minutes(2));
    assert!(queue::retry_due(&kit.state).unwrap().contains("1 printed"));
    assert!(printer.next_job().is_some());
    let done = queue::get_job(&conn, &job.id).unwrap();
    assert_eq!((done.status.as_str(), done.attempts), ("printed", 2));

    // A printed job can't be retried or cancelled.
    assert_eq!(
        code(queue::retry_job(
            &kit.state,
            &mut kit.ctx(&mut conn, &cashier),
            &IdInput { id: job.id.clone() }
        )),
        ErrorCode::Conflict
    );
    assert_eq!(
        code(queue::cancel_job(
            &mut kit.ctx(&mut conn, &cashier),
            &IdInput { id: job.id }
        )),
        ErrorCode::Conflict
    );
}

#[test]
fn only_shared_printers_take_jobs_and_permissions_apply() {
    let kit = Kit::new();
    configure(&kit, hub_printer(closed_port(), false));
    let invoice = cash_sale(&kit, "c");
    let cashier = kit.user("cashier");
    let keeper = kit.user("stock_keeper");
    let manager = kit.user("manager");
    let mut conn = kit.conn();
    let err = queue::submit(
        &kit.state,
        &mut kit.ctx(&mut conn, &cashier),
        &submit_input("print-000003", &invoice),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::PrinterError);
    assert_eq!(err.details.unwrap()["kind"], "not_configured");

    configure(&kit, hub_printer(closed_port(), true));
    assert_eq!(
        code(queue::submit(
            &kit.state,
            &mut kit.ctx(&mut conn, &keeper),
            &submit_input("print-000004", &invoice)
        )),
        ErrorCode::Forbidden,
        "a stock keeper can't print sales documents"
    );
    let mine = queue::submit(
        &kit.state,
        &mut kit.ctx(&mut conn, &cashier),
        &submit_input("print-000005", &invoice),
    )
    .unwrap();
    queue::submit(
        &kit.state,
        &mut kit.ctx(&mut conn, &manager),
        &submit_input("print-000006", &invoice),
    )
    .unwrap();

    // Cashiers see their own jobs; managers see the whole queue and can cancel anyone's.
    let list = |actor| {
        queue::list_jobs(&kit.ctx(&mut kit.conn(), actor), &PrintJobQuery::default())
            .unwrap()
            .total
    };
    assert_eq!(list(&cashier), 1);
    assert_eq!(list(&manager), 2);
    let failed = queue::list_jobs(
        &kit.ctx(&mut conn, &manager),
        &PrintJobQuery {
            status: Some("failed".into()),
            ..PrintJobQuery::default()
        },
    )
    .unwrap();
    assert_eq!(failed.total, 2);
    let cancelled =
        queue::cancel_job(&mut kit.ctx(&mut conn, &manager), &IdInput { id: mine.id }).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let audit: i64 = conn
        .query_row(
            "SELECT count(*) FROM audit_log WHERE action = 'PRINT_JOB_CANCELLED'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audit, 1);

    // Old printed and cancelled jobs are cleared by the daily clean-up.
    kit.clock.advance(chrono::Duration::days(8));
    assert_eq!(queue::purge_old_jobs(&conn, kit.now()).unwrap(), 1);
}

#[test]
fn the_drawer_opens_by_hand_only_with_permission_or_approval() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let manager = kit.user("manager");
    let mut conn = kit.conn();
    let input = DrawerInput {
        reason: Some("Change for a customer".into()),
    };
    assert_eq!(
        code(queue::authorize_drawer(
            &mut kit.ctx(&mut conn, &cashier),
            &input
        )),
        ErrorCode::ApprovalRequired
    );
    let approver = kit.approver(&manager);
    let mut ctx = kit.ctx(&mut conn, &cashier);
    ctx.approver = Some(&approver);
    queue::authorize_drawer(&mut ctx, &input).unwrap();
    let (summary, approved_by): (String, String) = conn
        .query_row(
            "SELECT summary, approved_by FROM audit_log WHERE action = 'DRAWER_OPENED'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(summary, "Change for a customer");
    assert_eq!(approved_by, manager.user_id);
    queue::authorize_drawer(
        &mut kit.ctx(&mut conn, &manager),
        &DrawerInput { reason: None },
    )
    .unwrap();
}

#[test]
fn label_templates_can_be_kept_and_edited_safely() {
    let kit = Kit::new();
    let owner = kit.owner();
    let keeper = kit.user("stock_keeper");
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let seeded = queue::list_templates(&kit.ctx(&mut conn, &keeper)).unwrap();
    assert_eq!(seeded.len(), 2, "two sizes to start from");
    assert_eq!(
        code(queue::list_templates(&kit.ctx(&mut conn, &cashier))),
        ErrorCode::Forbidden
    );

    let mut template = LabelTemplate {
        id: None,
        name: "Jewellery tag 30 x 15".into(),
        ..seeded[0].clone()
    };
    template.width_mm = 30;
    template.height_mm = 15;
    template.symbology = Symbology::Qr;
    let saved = queue::save_template(&mut kit.ctx(&mut conn, &owner), &template).unwrap();
    assert!(saved.id.is_some() && saved.updated_at.is_some());

    // Two people editing: the second save with the old timestamp is refused.
    let mut first = saved.clone();
    first.columns = 2;
    kit.clock.advance(chrono::Duration::seconds(5));
    queue::save_template(&mut kit.ctx(&mut conn, &owner), &first).unwrap();
    let mut stale = saved.clone();
    stale.gap_mm = 3;
    assert_eq!(
        code(queue::save_template(
            &mut kit.ctx(&mut conn, &owner),
            &stale
        )),
        ErrorCode::Conflict
    );

    let mut bad = saved.clone();
    bad.width_mm = 500;
    assert_eq!(
        code(queue::save_template(&mut kit.ctx(&mut conn, &owner), &bad)),
        ErrorCode::Validation
    );
    assert_eq!(
        code(queue::save_template(
            &mut kit.ctx(&mut conn, &cashier),
            &template
        )),
        ErrorCode::Forbidden
    );

    queue::delete_template(
        &mut kit.ctx(&mut conn, &owner),
        &IdInput {
            id: saved.id.unwrap(),
        },
    )
    .unwrap();
    assert_eq!(
        queue::list_templates(&kit.ctx(&mut conn, &owner))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_goods_receipt_gives_one_label_per_unit_with_its_batch() {
    let kit = Kit::new();
    let owner = kit.owner();
    let dal = kit.product("Toor Dal 1 kg", "DAL-1", 15_000, 12_000, 0, None);
    let supplier = kit.party("Dal Traders", "supplier", None);
    let mut conn = kit.conn();
    let receipt = purchases::receive_goods(
        &mut kit.ctx(&mut conn, &owner),
        &GoodsReceiptInput {
            idempotency_key: "grn-label-0001".into(),
            party_id: supplier,
            warehouse_id: None,
            purchase_order_id: None,
            supplier_reference: None,
            lines: vec![PurchaseLineInput {
                variant_id: dal.clone(),
                unit_id: None,
                qty: 12_000,
                unit_cost: 12_000,
                discount: None,
                batch_no: Some("B-77".into()),
                expiry_date: Some("2027-06-30".into()),
                mfg_date: None,
                po_item_id: None,
            }],
            notes: None,
        },
    )
    .unwrap();
    let keeper = kit.user("stock_keeper");
    let items = queue::label_items(
        &kit.ctx(&mut conn, &keeper),
        &LabelSourceInput {
            variant_ids: vec![dal],
            receipt_id: Some(receipt.id),
        },
    )
    .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!((items[0].copies, items[0].batch_no.as_deref()), (1, None));
    assert_eq!(items[1].copies, 12);
    assert_eq!(items[1].batch_no.as_deref(), Some("B-77"));
    assert_eq!(items[1].expiry_date.as_deref(), Some("2027-06-30"));
    assert_eq!(items[1].price, 15_000);

    let mut template = queue::list_templates(&kit.ctx(&mut conn, &keeper)).unwrap()[0].clone();
    template.symbology = Symbology::Ean13;
    let previews = queue::preview(
        &kit.ctx(&mut conn, &keeper),
        &LabelPreviewInput {
            template,
            items: items.clone(),
        },
    )
    .unwrap();
    assert_eq!(previews.len(), 2, "one preview per item, not per copy");
    assert_eq!(previews[0].symbology, Some(Symbology::Code128));
    assert!(previews[0].warning.as_deref().unwrap().contains("Code 128"));
    assert!(previews[0].svg.starts_with("<svg"));
}

fn product_without_barcode(kit: &Kit, sku: &str) -> String {
    let variant = kit.product(&format!("Loose {sku}"), sku, 5_000, 3_000, 0, None);
    kit.conn()
        .execute(
            "UPDATE product_variants SET barcode = NULL WHERE id = ?1",
            [&variant],
        )
        .unwrap();
    variant
}

#[test]
fn in_store_barcodes_are_valid_unique_and_checked_on_save() {
    let kit = Kit::new();
    let owner = kit.owner();
    let a = product_without_barcode(&kit, "LOOSE-A");
    let b = product_without_barcode(&kit, "LOOSE-B");
    let mut conn = kit.conn();
    let made = queue::generate_barcodes(
        &mut kit.ctx(&mut conn, &owner),
        &GenerateBarcodesInput {
            variant_ids: vec![a.clone(), b.clone()],
        },
    )
    .unwrap();
    assert_eq!(made.len(), 2);
    assert_eq!(made[0].barcode, "2900000000018");
    assert_eq!(made[1].barcode, "2900000000025");
    assert!(made.iter().all(|m| barcode::is_valid_ean(&m.barcode, 13)));
    // A product that has a barcode keeps it.
    let again = queue::generate_barcodes(
        &mut kit.ctx(&mut conn, &owner),
        &GenerateBarcodesInput {
            variant_ids: vec![a.clone()],
        },
    )
    .unwrap();
    assert!(again.is_empty());
    let cashier = kit.user("cashier");
    assert_eq!(
        code(queue::generate_barcodes(
            &mut kit.ctx(&mut conn, &cashier),
            &GenerateBarcodesInput {
                variant_ids: vec![b]
            }
        )),
        ErrorCode::Forbidden
    );
    let audited: i64 = conn
        .query_row(
            "SELECT count(*) FROM audit_log WHERE action = 'BARCODE_GENERATED'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audited, 2);

    // Another product may not use that barcode, not even on a pack size.
    let unit: String = conn
        .query_row("SELECT id FROM units WHERE code = 'pcs'", [], |r| r.get(0))
        .unwrap();
    let box_unit: String = conn
        .query_row("SELECT id FROM units WHERE code = 'box'", [], |r| r.get(0))
        .unwrap();
    let product = |barcode: Option<&str>, pack: Option<&str>| ProductInput {
        id: None,
        expected_updated_at: None,
        name: "Sugar".into(),
        description: None,
        category_id: None,
        brand_id: None,
        base_unit_id: unit.clone(),
        tax_rate_id: None,
        hsn_sac: None,
        track_batches: false,
        track_expiry: false,
        is_service: false,
        is_active: true,
        variants: vec![VariantInput {
            id: None,
            variant_name: None,
            sku: "SUGAR-1".into(),
            barcode: barcode.map(String::from),
            sale_price: 4_500,
            mrp: None,
            cost_price: Some(4_000),
            reorder_level: 0,
            reorder_qty: 0,
            is_active: true,
            opening_stock: None,
        }],
        unit_conversions: pack
            .map(|p| UnitConversionInput {
                from_unit_id: box_unit.clone(),
                factor_num: 10_000,
                factor_den: 1_000,
                barcode: Some(p.into()),
            })
            .into_iter()
            .collect(),
    };
    let err = catalog::save_product(
        &mut kit.ctx(&mut conn, &owner),
        &product(None, Some("2900000000018")),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert!(err.message.contains("Another product"), "{}", err.message);
    let err = catalog::save_product(
        &mut kit.ctx(&mut conn, &owner),
        &product(Some("8900001"), Some("8900001")),
    )
    .unwrap_err();
    assert!(err.message.contains("used twice"), "{}", err.message);
    catalog::save_product(
        &mut kit.ctx(&mut conn, &owner),
        &product(Some("8900001"), Some("8900002")),
    )
    .unwrap();
}

#[test]
fn a_receipt_job_needs_a_receipt_printer() {
    let kit = Kit::new();
    let mut label = hub_printer(closed_port(), true);
    label.kind = PrinterKind::Label;
    configure(&kit, label);
    let invoice = cash_sale(&kit, "d");
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    assert_eq!(
        code(queue::submit(
            &kit.state,
            &mut kit.ctx(&mut conn, &cashier),
            &submit_input("print-000007", &invoice)
        )),
        ErrorCode::Validation
    );
}
