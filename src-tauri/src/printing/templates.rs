//! What goes on a receipt, return slip, till-close summary, held-bill slip and test page, laid out
//! independently of the printer. The receipt mirrors the screen's receipt (src/features/sales/
//! print.ts `receiptHtml`) line for line, from the same backend figures, so it matches to the
//! paisa.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::config::PrinterProfile;
use super::raster::Placement;
use super::text::{self, money};
use crate::services::payments::RegisterSession;
use crate::services::sales::{InvoiceDetail, ReturnDetail};
use crate::services::settings::PublicSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Normal,
    /// Double width and height.
    Large,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text {
        text: String,
        place: Placement,
        bold: bool,
        size: Size,
    },
    /// Left text and a right-aligned value on one line.
    Pair {
        left: String,
        right: String,
        bold: bool,
        size: Size,
    },
    Rule,
    Blank,
    Barcode(String),
    Qr(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Doc {
    pub blocks: Vec<Block>,
}

impl Doc {
    fn text(&mut self, text: impl Into<String>, place: Placement) -> &mut Self {
        self.blocks.push(Block::Text {
            text: text.into(),
            place,
            bold: false,
            size: Size::Normal,
        });
        self
    }

    fn heading(&mut self, text: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Text {
            text: text.into(),
            place: Placement::Center,
            bold: true,
            size: Size::Large,
        });
        self
    }

    fn strong(&mut self, text: impl Into<String>, place: Placement) -> &mut Self {
        self.blocks.push(Block::Text {
            text: text.into(),
            place,
            bold: true,
            size: Size::Normal,
        });
        self
    }

    fn pair(&mut self, left: impl Into<String>, right: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Pair {
            left: left.into(),
            right: right.into(),
            bold: false,
            size: Size::Normal,
        });
        self
    }

    fn total(&mut self, left: impl Into<String>, right: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Pair {
            left: left.into(),
            right: right.into(),
            bold: true,
            size: Size::Large,
        });
        self
    }

    fn rule(&mut self) -> &mut Self {
        self.blocks.push(Block::Rule);
        self
    }

    fn blank(&mut self) -> &mut Self {
        self.blocks.push(Block::Blank);
        self
    }

    /// Every text on the document (tests and the "needs a picture" check).
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            match b {
                Block::Text { text, .. } => out.push_str(text),
                Block::Pair { left, right, .. } => {
                    out.push_str(left);
                    out.push(' ');
                    out.push_str(right);
                }
                Block::Rule => out.push_str("---"),
                Block::Blank => {}
                Block::Barcode(d) | Block::Qr(d) => out.push_str(d),
            }
            out.push('\n');
        }
        out
    }
}

/// Amount formatting in the business's currency and locale.
struct Fmt {
    digits: u32,
    locale: String,
    symbol: String,
    timezone: chrono_tz::Tz,
}

impl Fmt {
    fn new(settings: &PublicSettings) -> Self {
        let b = &settings.business;
        let symbol = match b.currency.as_str() {
            "INR" => "₹".to_string(),
            "USD" => "$".to_string(),
            "EUR" => "€".to_string(),
            "GBP" => "£".to_string(),
            other => format!("{other} "),
        };
        Self {
            digits: b.currency_minor_digits,
            locale: b.locale.clone(),
            symbol,
            timezone: b.timezone.parse().unwrap_or(chrono_tz::Asia::Kolkata),
        }
    }

    fn m(&self, minor: i64) -> String {
        money(minor, self.digits, &self.locale)
    }

    /// With the currency symbol (totals).
    fn total(&self, minor: i64) -> String {
        if minor < 0 {
            format!("-{}{}", self.symbol, self.m(-minor))
        } else {
            format!("{}{}", self.symbol, self.m(minor))
        }
    }

    fn date_time(&self, instant: &str) -> String {
        match DateTime::parse_from_rfc3339(instant) {
            Ok(t) => {
                let local = t.with_timezone(&self.timezone);
                format!(
                    "{} {}",
                    text::date(&local.format("%Y-%m-%d").to_string()),
                    local.format("%H:%M")
                )
            }
            Err(_) => instant.to_string(),
        }
    }
}

fn header(doc: &mut Doc, settings: &PublicSettings) {
    let b = &settings.business;
    doc.heading(b.name.clone());
    if let Some(address) = b.address.as_deref().filter(|a| !a.trim().is_empty()) {
        doc.text(address, Placement::Center);
    }
    if let Some(phone) = b.phone.as_deref().filter(|p| !p.trim().is_empty()) {
        doc.text(format!("Ph {phone}"), Placement::Center);
    }
    if let Some(gstin) = b.gstin.as_deref().filter(|g| !g.trim().is_empty()) {
        doc.text(format!("GSTIN {gstin}"), Placement::Center);
    }
    doc.rule();
}

fn footer(doc: &mut Doc, settings: &PublicSettings) {
    match settings
        .invoice
        .footer_text
        .as_deref()
        .filter(|f| !f.trim().is_empty())
    {
        Some(footer) => doc.text(footer, Placement::Center),
        None => doc.text("Thank you!", Placement::Center),
    };
}

/// The till receipt (58 or 80 mm), as `receiptHtml`.
pub fn receipt(invoice: &InvoiceDetail, settings: &PublicSettings, reprint: bool) -> Doc {
    let f = Fmt::new(settings);
    let mut doc = Doc::default();
    header(&mut doc, settings);
    if reprint {
        doc.strong("*** REPRINT ***", Placement::Center);
    }
    if invoice.status == "voided" {
        doc.strong("*** VOIDED ***", Placement::Center);
    }
    let kind = if invoice.tax_total > 0 {
        "Tax invoice"
    } else {
        "Bill"
    };
    doc.pair(
        format!("{kind} {}", invoice.doc_no),
        text::date(&invoice.business_date),
    );
    let mut customer = invoice.customer_name.clone();
    if let Some(gstin) = invoice.customer_gstin.as_deref().filter(|g| !g.is_empty()) {
        customer.push_str(&format!(" - {gstin}"));
    }
    doc.text(customer, Placement::Left);
    doc.rule();
    for line in &invoice.lines {
        doc.text(line.item_name.clone(), Placement::Left);
        let unit = line.unit_name.as_deref().unwrap_or("");
        let mut left = format!(
            "  {} {unit} x {}",
            text::qty(line.qty),
            f.m(line.unit_price)
        );
        if line.line_discount != 0 {
            left.push_str(&format!(" -{}", f.m(line.line_discount)));
        }
        doc.pair(left, f.m(line.line_total));
        for b in &line.batches {
            let expiry = b
                .expiry_date
                .as_deref()
                .filter(|e| e.len() >= 7)
                .map(|e| format!(" exp {}/{}", &e[5..7], &e[..4]))
                .unwrap_or_default();
            doc.text(format!("  Batch {}{expiry}", b.batch_no), Placement::Left);
        }
    }
    doc.rule();
    doc.pair("Items total", f.m(invoice.gross));
    let discount = invoice.line_discount + invoice.bill_discount;
    if discount != 0 {
        doc.pair("Discount", format!("-{}", f.m(discount)));
    }
    if invoice.tax_total != 0 {
        doc.pair("Tax", f.m(invoice.tax_total));
    }
    if invoice.round_off != 0 {
        doc.pair("Round off", f.m(invoice.round_off));
    }
    doc.total("TOTAL", f.total(invoice.total));
    for p in &invoice.payments {
        doc.pair(text::label(&p.mode), f.m(p.tendered.unwrap_or(p.amount)));
        if p.change_given != 0 {
            doc.pair("Change", f.m(p.change_given));
        }
    }
    if invoice.balance_due > 0 {
        doc.blocks.push(Block::Pair {
            left: "Balance due".into(),
            right: f.m(invoice.balance_due),
            bold: true,
            size: Size::Normal,
        });
    }
    if invoice.tax_total != 0 {
        doc.rule();
        for t in invoice.tax_summary.iter().filter(|t| t.tax_rate > 0) {
            let parts: Vec<String> = [
                ("CGST", t.cgst),
                ("SGST", t.sgst),
                ("IGST", t.igst),
                ("Cess", t.cess),
                ("Tax", t.other_tax),
            ]
            .iter()
            .filter(|(_, v)| *v != 0)
            .map(|(name, v)| format!("{name} {}", f.m(*v)))
            .collect();
            doc.pair(
                format!("{} on {}", text::rate(t.tax_rate), f.m(t.taxable)),
                parts.join(" "),
            );
        }
    }
    doc.rule();
    if invoice.origin == "offline" {
        doc.text("Billed offline - syncs to the hub", Placement::Center);
    }
    footer(&mut doc, settings);
    doc
}

/// Credit note / return slip for the customer.
pub fn return_slip(ret: &ReturnDetail, invoice_no: Option<&str>, settings: &PublicSettings) -> Doc {
    let f = Fmt::new(settings);
    let mut doc = Doc::default();
    header(&mut doc, settings);
    doc.strong("CREDIT NOTE", Placement::Center);
    doc.pair(ret.doc_no.clone(), text::date(&ret.business_date));
    if let Some(no) = invoice_no.filter(|n| !n.is_empty()) {
        doc.text(format!("Against bill {no}"), Placement::Left);
    }
    doc.text(ret.customer_name.clone(), Placement::Left);
    if let Some(reason) = ret.reason.as_deref().filter(|r| !r.trim().is_empty()) {
        doc.text(format!("Reason: {reason}"), Placement::Left);
    }
    doc.rule();
    for line in &ret.lines {
        doc.text(line.item_name.clone(), Placement::Left);
        doc.pair(
            format!("  {} x {}", text::qty(line.qty), f.m(line.unit_price)),
            f.m(line.line_total),
        );
    }
    doc.rule();
    doc.pair("Taxable", f.m(ret.taxable));
    if ret.tax_total != 0 {
        doc.pair("Tax", f.m(ret.tax_total));
    }
    doc.total("TOTAL", f.total(ret.total));
    if ret.amount_refunded != 0 {
        doc.pair("Refunded", f.m(ret.amount_refunded));
    }
    let credit = ret.total - ret.amount_refunded;
    if credit > 0 {
        doc.pair("Kept as credit", f.m(credit));
    }
    doc.rule();
    doc.blank();
    doc.text("Customer signature ____________", Placement::Left);
    doc
}

/// The till's cash-up when it is closed.
pub fn till_close(session: &RegisterSession, settings: &PublicSettings, now: DateTime<Utc>) -> Doc {
    let f = Fmt::new(settings);
    let mut doc = Doc::default();
    doc.heading(settings.business.name.clone());
    doc.strong("TILL CLOSE", Placement::Center);
    doc.rule();
    doc.pair("Till", session.register_name.clone());
    doc.pair("Opened", f.date_time(&session.opened_at));
    if let Some(by) = &session.opened_by_name {
        doc.pair("Opened by", by.clone());
    }
    if let Some(closed) = &session.closed_at {
        doc.pair("Closed", f.date_time(closed));
    }
    doc.rule();
    doc.pair("Opening float", f.m(session.opening_float));
    doc.pair("Cash in", f.m(session.cash_in));
    doc.pair("Cash out", format!("-{}", f.m(session.cash_out)));
    doc.blocks.push(Block::Pair {
        left: "Expected cash".into(),
        right: f.m(session.expected_cash),
        bold: true,
        size: Size::Normal,
    });
    if let Some(counted) = session.counted_cash {
        doc.pair("Counted cash", f.m(counted));
    }
    if let Some(diff) = session.difference {
        let label = match diff.cmp(&0) {
            std::cmp::Ordering::Less => "Short",
            std::cmp::Ordering::Greater => "Over",
            std::cmp::Ordering::Equal => "Difference",
        };
        doc.total(label, f.total(diff.abs()));
    }
    doc.rule();
    doc.text(
        format!("Printed {}", f.date_time(&now.to_rfc3339())),
        Placement::Left,
    );
    doc.blank();
    doc.text("Cashier  ____________", Placement::Left);
    doc.blank();
    doc.text("Manager  ____________", Placement::Left);
    doc
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HeldSlipLine {
    pub name: String,
    /// Thousandths.
    pub qty: i64,
    pub unit: Option<String>,
}

/// A parked bill, for the customer to keep while they fetch something.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HeldSlip {
    pub label: String,
    pub held_at: String,
    pub held_by: Option<String>,
    pub lines: Vec<HeldSlipLine>,
    /// The cart total the till showed, when it had one.
    pub total: Option<i64>,
}

pub fn held_slip(slip: &HeldSlip, settings: &PublicSettings) -> Doc {
    let f = Fmt::new(settings);
    let mut doc = Doc::default();
    doc.text(settings.business.name.clone(), Placement::Center);
    doc.strong("HELD BILL", Placement::Center);
    doc.heading(slip.label.clone());
    doc.pair("Held", f.date_time(&slip.held_at));
    if let Some(by) = &slip.held_by {
        doc.pair("By", by.clone());
    }
    doc.rule();
    for line in &slip.lines {
        let qty = format!(
            "{} {}",
            text::qty(line.qty),
            line.unit.as_deref().unwrap_or("")
        );
        doc.pair(line.name.clone(), qty.trim_end().to_string());
    }
    doc.rule();
    doc.pair("Items", slip.lines.len().to_string());
    if let Some(total) = slip.total {
        doc.total("TOTAL", f.total(total));
    }
    doc.text(
        "Show this slip at the counter to finish the bill.",
        Placement::Center,
    );
    doc
}

/// Proves the connection, the paper width, the code page, the rupee sign and the cutter.
pub fn test_page(profile: &PrinterProfile, settings: &PublicSettings, now: DateTime<Utc>) -> Doc {
    let f = Fmt::new(settings);
    let mut doc = Doc::default();
    doc.heading("Printer test");
    doc.text(settings.business.name.clone(), Placement::Center);
    doc.rule();
    doc.pair("Printer", profile.name.clone());
    doc.text(profile.connection.describe(), Placement::Left);
    let columns = profile.paper.columns();
    doc.pair("Paper", format!("{} characters", columns));
    let ruler: String = (1..=columns)
        .map(|i| char::from(b'0' + (i % 10) as u8))
        .collect();
    doc.text(ruler, Placement::Left);
    doc.pair("Amount", f.total(118_000));
    doc.text("Hindi: नमस्ते   Malayalam: നമസ്കാരം", Placement::Left);
    doc.text(
        format!("Printed {}", f.date_time(&now.to_rfc3339())),
        Placement::Left,
    );
    doc.rule();
    doc.blocks.push(Block::Barcode("TEST-128".into()));
    doc.blocks.push(Block::Qr("Printer test OK".into()));
    doc.text(
        "If you can read this, the printer works.",
        Placement::Center,
    );
    doc
}
