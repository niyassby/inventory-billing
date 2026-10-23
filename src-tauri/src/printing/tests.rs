//! Byte-level snapshots of the print jobs for known documents. A snapshot is the job written as
//! text (printable characters as they are, everything else as `\xNN`, one printed line per file
//! line) in `src/printing/snapshots/`. When a layout changes on purpose, refresh them with
//! `UPDATE_SNAPSHOTS=1 cargo test printing` and review the diff.

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::barcode::Symbology;
use super::config::{
    CutMode, DrawerPin, LabelLanguage, PaperWidth, PrinterKind, PrinterProfile, RupeeStyle,
};
use super::labels::{self, LabelItem, LabelTemplate};
use super::templates::{self, HeldSlip, HeldSlipLine};
use super::text::CodePage;
use super::transport::PrinterConnection;
use super::{render, PrintDocument};
use crate::services::payments::RegisterSession;
use crate::services::sales::{InvoiceDetail, ReturnDetail};
use crate::services::settings::{PublicSettings, Settings};

fn dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for b in bytes {
        match b {
            b'\n' => out.push_str("\\n\n"),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(char::from(*b)),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out
}

fn check_snapshot(name: &str, bytes: &[u8]) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/printing/snapshots")
        .join(name);
    let fresh = dump(bytes);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed == fresh,
        "print job {name} changed: run `UPDATE_SNAPSHOTS=1 cargo test printing` and review the diff\n--- new ---\n{fresh}"
    );
}

pub(crate) fn settings() -> PublicSettings {
    let mut s = Settings::default();
    s.business.name = "Demo Stores".into();
    s.business.address = Some("MG Road, Bengaluru".into());
    s.business.phone = Some("9876543210".into());
    s.business.gstin = Some("29AAGCB7383J1Z4".into());
    s.invoice.footer_text = Some("Goods once sold are exchanged within 7 days.".into());
    PublicSettings::from(&s)
}

pub(crate) fn profile(paper: PaperWidth) -> PrinterProfile {
    PrinterProfile {
        id: "p1".into(),
        name: "Front counter".into(),
        kind: PrinterKind::Receipt,
        connection: PrinterConnection::Network {
            host: "192.168.1.50".into(),
            port: 9100,
        },
        paper,
        code_page: CodePage::Pc437,
        rupee: RupeeStyle::Text,
        picture_fallback: false,
        cut: CutMode::Partial,
        feed_lines: 4,
        copies: 1,
        has_drawer: true,
        drawer_pin: DrawerPin::Pin2,
        label_language: LabelLanguage::Zpl,
        label_dpi: 203,
        shared: false,
    }
}

/// A cash sale of two items under 18% GST, prices including tax: ₹295.00, paid ₹300.00.
pub(crate) fn invoice() -> InvoiceDetail {
    let lines = json!([
        { "id": "l1", "lineNo": 1, "variantId": "v1", "itemName": "Assam Tea 250 g", "sku": "TEA-250",
          "hsnSac": "0902", "unitName": "pcs", "qty": 2000, "unitPrice": 11800, "mrp": 12500,
          "lineDiscount": 0, "billDiscountShare": 0, "taxable": 20000, "taxRate": 180000,
          "cgst": 1800, "sgst": 1800, "igst": 0, "otherTax": 0, "cess": 0, "taxTotal": 3600,
          "lineTotal": 23600, "returnedQty": 0 },
        { "id": "l2", "lineNo": 2, "variantId": "v2", "itemName": "Butter Biscuits Family Pack 400 g", "sku": "BIS-400",
          "hsnSac": "1905", "unitName": "pcs", "qty": 1000, "unitPrice": 5900, "mrp": null,
          "lineDiscount": 0, "billDiscountShare": 0, "taxable": 5000, "taxRate": 180000,
          "cgst": 450, "sgst": 450, "igst": 0, "otherTax": 0, "cess": 0, "taxTotal": 900,
          "lineTotal": 5900, "returnedQty": 0 }
    ]);
    let payments = json!([
        { "id": "p1", "docNo": "RCT/26-27/00031", "businessDate": "2026-10-22", "mode": "cash",
          "amount": 29500, "tendered": 30000, "changeGiven": 500, "status": "finalized" }
    ]);
    let tax = json!([
        { "taxRate": 180000, "taxable": 25000, "cgst": 2250, "sgst": 2250, "igst": 0, "cess": 0, "otherTax": 0 }
    ]);
    let mut header = json!({
        "id": "inv1", "docNo": "INV/26-27/00012", "businessDate": "2026-10-22", "status": "finalized",
        "paymentStatus": "paid", "partyId": null, "customerName": "Walk-in customer",
        "customerPhone": null, "customerGstin": null, "billingAddress": null, "placeOfSupply": null,
        "isInterState": false, "pricesIncludeTax": true
    });
    let totals = json!({
        "gross": 29500, "lineDiscount": 0, "billDiscount": 0, "taxable": 25000, "taxTotal": 4500,
        "roundOff": 0, "total": 29500, "amountPaid": 29500, "balanceDue": 0, "notes": null,
        "createdByName": "Asha", "finalizedAt": "2026-10-22T06:30:00Z", "voidedAt": null,
        "voidReason": null, "snapshotSource": "live", "origin": "online"
    });
    let object = header.as_object_mut().unwrap();
    object.extend(totals.as_object().unwrap().clone());
    object.insert("lines".into(), lines);
    object.insert("payments".into(), payments);
    object.insert("taxSummary".into(), tax);
    serde_json::from_value(header).unwrap()
}

fn sales_return() -> ReturnDetail {
    serde_json::from_value(json!({
        "id": "r1", "docNo": "CN/26-27/00003", "businessDate": "2026-10-23", "status": "finalized",
        "originalInvoiceId": "inv1", "partyId": null, "customerName": "Walk-in customer",
        "reason": "Damaged pack", "taxable": 10000, "taxTotal": 1800, "total": 11800, "amountRefunded": 11800,
        "lines": [{ "variantId": "v1", "itemName": "Assam Tea 250 g", "qty": 1000, "unitPrice": 11800,
                    "taxable": 10000, "taxTotal": 1800, "lineTotal": 11800, "restock": false }]
    }))
    .unwrap()
}

fn session() -> RegisterSession {
    serde_json::from_value(json!({
        "id": "s1", "registerId": "r1", "registerName": "Till 1", "status": "closed",
        "openedAt": "2026-10-22T03:30:00Z", "openedByName": "Asha", "openingFloat": 200000,
        "cashIn": 1229500, "cashOut": 11800, "expectedCash": 1417700, "countedCash": 1417200,
        "difference": -500, "closedAt": "2026-10-22T14:45:00Z"
    }))
    .unwrap()
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 15, 0, 0).unwrap()
}

fn job(doc: &PrintDocument, paper: PaperWidth, kick: bool) -> Vec<u8> {
    render(doc, &profile(paper), &settings(), now(), None, kick).unwrap()
}

#[test]
fn receipt_80mm() {
    let doc = PrintDocument::Receipt {
        invoice: Box::new(invoice()),
        reprint: false,
    };
    check_snapshot("receipt_80mm.txt", &job(&doc, PaperWidth::Mm80, true));
}

#[test]
fn receipt_58mm() {
    let doc = PrintDocument::Receipt {
        invoice: Box::new(invoice()),
        reprint: true,
    };
    check_snapshot(
        "receipt_58mm_reprint.txt",
        &job(&doc, PaperWidth::Mm58, false),
    );
}

#[test]
fn return_slip_till_close_and_held_slip() {
    let ret = PrintDocument::ReturnSlip {
        sales_return: Box::new(sales_return()),
        invoice_no: Some("INV/26-27/00012".into()),
    };
    check_snapshot("return_slip_80mm.txt", &job(&ret, PaperWidth::Mm80, false));
    let close = PrintDocument::TillClose {
        session: Box::new(session()),
    };
    check_snapshot("till_close_80mm.txt", &job(&close, PaperWidth::Mm80, false));
    let day = PrintDocument::DayClose {
        summary: Box::new(
            serde_json::from_value(json!({
                "businessDate": "2026-10-22", "bills": 41, "salesTotal": 1824500, "taxTotal": 98200,
                "discountTotal": 12000, "returnsTotal": 25000, "purchasesTotal": 600000, "expensesTotal": 45000,
                "received": [{ "mode": "cash", "amount": 1229500 }, { "mode": "upi", "amount": 595000 }],
                "paid": [{ "mode": "cash", "amount": 11800 }],
                "tills": [{ "register": "Till 1", "countedBy": "Asha", "expectedCash": 1417700, "countedCash": 1417200,
                            "difference": -500, "differenceReason": "wrong_change", "approvedBy": null }],
                "openTills": 0, "closedAt": "2026-10-22T15:00:00Z", "closedBy": "Ravi", "notes": "Festival rush"
            }))
            .unwrap(),
        ),
    };
    check_snapshot("day_close_80mm.txt", &job(&day, PaperWidth::Mm80, false));
    let held = PrintDocument::HeldSlip {
        slip: HeldSlip {
            label: "Table 4".into(),
            held_at: "2026-10-22T07:05:00Z".into(),
            held_by: Some("Asha".into()),
            lines: vec![
                HeldSlipLine {
                    name: "Assam Tea 250 g".into(),
                    qty: 2000,
                    unit: Some("pcs".into()),
                },
                HeldSlipLine {
                    name: "Loose sugar".into(),
                    qty: 1500,
                    unit: Some("kg".into()),
                },
            ],
            total: Some(29_500),
        },
    };
    check_snapshot("held_slip_58mm.txt", &job(&held, PaperWidth::Mm58, false));
}

#[test]
fn the_receipt_shows_the_invoice_figures_to_the_paisa() {
    let inv = invoice();
    let text = templates::receipt(&inv, &settings(), false).plain_text();
    for expected in [
        "Tax invoice INV/26-27/00012",
        "22 Oct 2026",
        "236.00",
        "59.00",
        "Items total 295.00",
        "Tax 45.00",
        "TOTAL ₹295.00",
        "Cash 300.00",
        "Change 5.00",
        "18% on 250.00 CGST 22.50 SGST 22.50",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
    // The printed job (₹ as "Rs.") has the same total.
    let bytes = job(
        &PrintDocument::Receipt {
            invoice: Box::new(inv),
            reprint: false,
        },
        PaperWidth::Mm80,
        false,
    );
    let printed = String::from_utf8_lossy(&bytes);
    assert!(printed.contains("Rs.295.00"));
    assert!(!printed.contains('₹'));
}

#[test]
fn the_drawer_opens_first_and_only_with_a_drawer() {
    let doc = PrintDocument::Receipt {
        invoice: Box::new(invoice()),
        reprint: false,
    };
    let with = job(&doc, PaperWidth::Mm80, true);
    assert_eq!(&with[..10], b"\x1b@\x1bt\x00\x1bp\x00\x19\xfa");
    let mut no_drawer = profile(PaperWidth::Mm80);
    no_drawer.has_drawer = false;
    let without = render(&doc, &no_drawer, &settings(), now(), None, true).unwrap();
    assert!(!without.windows(3).any(|w| w == b"\x1bp\x00"));
    assert!(doc.takes_cash());
    let reprint = PrintDocument::Receipt {
        invoice: Box::new(invoice()),
        reprint: true,
    };
    assert!(!reprint.takes_cash(), "a reprint never opens the drawer");
}

#[test]
fn copies_repeat_the_whole_receipt_with_a_cut_each() {
    let doc = PrintDocument::Receipt {
        invoice: Box::new(invoice()),
        reprint: false,
    };
    let mut two = profile(PaperWidth::Mm80);
    two.copies = 2;
    let bytes = render(&doc, &two, &settings(), now(), None, false).unwrap();
    let cuts = bytes.windows(3).filter(|w| *w == b"\x1dV\x01").count();
    assert_eq!(cuts, 2);
}

#[test]
fn a_label_job_is_refused_on_a_receipt_printer() {
    let doc = PrintDocument::Labels {
        template: template(Symbology::Ean13),
        items: vec![item()],
    };
    let err = render(
        &doc,
        &profile(PaperWidth::Mm80),
        &settings(),
        now(),
        None,
        false,
    )
    .unwrap_err();
    assert!(err.message.contains("label printer"));
}

#[test]
fn the_test_page_prints_on_both_kinds() {
    let receipt = job(&PrintDocument::Test, PaperWidth::Mm80, false);
    assert!(String::from_utf8_lossy(&receipt).contains("Printer test"));
    let mut label = profile(PaperWidth::Mm80);
    label.kind = PrinterKind::Label;
    let bytes = render(
        &PrintDocument::Test,
        &label,
        &settings(),
        now(),
        None,
        false,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&bytes).starts_with("^XA"));
}

// ---------------------------------------------------------------- labels

fn template(symbology: Symbology) -> LabelTemplate {
    LabelTemplate {
        id: None,
        name: "Shelf 50x25".into(),
        width_mm: 50,
        height_mm: 25,
        gap_mm: 2,
        columns: 2,
        symbology,
        show_business: false,
        show_name: true,
        show_price: true,
        show_mrp: true,
        show_sku: false,
        show_barcode: true,
        show_batch: true,
        show_expiry: true,
        updated_at: None,
    }
}

fn item() -> LabelItem {
    LabelItem {
        variant_id: Some("v1".into()),
        name: "Assam Tea 250 g".into(),
        sku: Some("TEA-250".into()),
        barcode: Some("8901234567890".into()),
        price: 11_800,
        mrp: Some(12_500),
        batch_no: Some("B24".into()),
        expiry_date: Some("2027-03-31".into()),
        copies: 3,
    }
}

#[test]
fn zpl_and_tspl_label_jobs() {
    let zpl = labels::render(
        &template(Symbology::Ean13),
        &[item()],
        &settings(),
        LabelLanguage::Zpl,
        203,
    )
    .unwrap();
    check_snapshot("labels_ean13.zpl.txt", &zpl);
    let tspl = labels::render(
        &template(Symbology::Code128),
        &[item()],
        &settings(),
        LabelLanguage::Tspl,
        203,
    )
    .unwrap();
    check_snapshot("labels_code128.tspl.txt", &tspl);
    let text = String::from_utf8(zpl).unwrap();
    // 3 copies, 2 across: two rows.
    assert_eq!(text.matches("^XA").count(), 2);
    assert!(
        text.contains("^FD890123456789^FS"),
        "EAN-13 without its check digit"
    );
    assert!(text.contains("Rs.118.00"));
    assert!(text.contains("MRP Rs.125.00"), "prices are never cut short");
    assert!(!text.contains("..."));
}

#[test]
fn every_barcode_type_reaches_the_label_printer() {
    let cases = [
        (Symbology::Ean13, "8901234567890", "^BEN", "\"EAN13\""),
        (Symbology::Ean8, "96385074", "^B8N", "\"EAN8\""),
        (Symbology::Code128, "TEA-250", "^BCN", "\"128\""),
        (Symbology::Qr, "TEA-250", "^BQN", "QRCODE"),
    ];
    for (symbology, data, zpl_command, tspl_command) in cases {
        let mut t = template(symbology);
        t.columns = 1;
        let mut i = item();
        i.barcode = Some(data.into());
        i.copies = 1;
        let zpl = String::from_utf8(
            labels::render(&t, &[i.clone()], &settings(), LabelLanguage::Zpl, 203).unwrap(),
        )
        .unwrap();
        assert!(zpl.contains(zpl_command), "{symbology:?}: {zpl}");
        let tspl = String::from_utf8(
            labels::render(&t, &[i.clone()], &settings(), LabelLanguage::Tspl, 300).unwrap(),
        )
        .unwrap();
        assert!(tspl.contains(tspl_command), "{symbology:?}: {tspl}");
        let (svg, used) = labels::svg(&t, &i, &settings());
        assert_eq!(used, Some(symbology));
        assert!(
            svg.starts_with("<svg") && svg.contains("<rect"),
            "{symbology:?}"
        );
    }
}

#[test]
fn an_ean_template_falls_back_to_code128_for_a_sku() {
    let mut i = item();
    i.barcode = None;
    let (svg, used) = labels::svg(&template(Symbology::Ean13), &i, &settings());
    assert_eq!(used, Some(Symbology::Code128));
    assert!(svg.contains("TEA-250"));
}

#[test]
fn label_limits() {
    let mut i = item();
    i.copies = 0;
    assert!(labels::render(
        &template(Symbology::Code128),
        &[i.clone()],
        &settings(),
        LabelLanguage::Zpl,
        203
    )
    .is_err());
    i.copies = labels::MAX_LABELS + 1;
    assert!(labels::render(
        &template(Symbology::Code128),
        &[i],
        &settings(),
        LabelLanguage::Zpl,
        203
    )
    .is_err());
    let mut t = template(Symbology::Code128);
    t.width_mm = 5;
    assert!(t.validate().is_err());
}
