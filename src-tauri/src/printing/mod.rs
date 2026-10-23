//! Direct printing (Phase 9): ESC/POS receipts and slips, ZPL/TSPL labels and the cash drawer,
//! over the network, a serial port or a Windows print queue. Everything that touches a printer
//! runs here, in Rust; the window only asks for a document to be printed.
//!
//! * `config`: this PC's printers and which document goes to which (printers.json).
//! * `templates` → `escpos`: what a receipt says, then the bytes for the printer.
//! * `labels`: one label layout drawn as ZPL, TSPL or SVG (preview, A4 sheets).
//! * `transport`: sending the bytes; failures come back as PRINTER_ERROR with a reason.
//! * The hub's print queue for counters is in services::printing.

pub mod barcode;
pub mod config;
pub mod escpos;
pub mod labels;
pub mod raster;
pub mod templates;
pub mod text;
pub mod transport;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::config::{DocKind, PrinterConfig, PrinterKind, PrinterProfile};
use self::labels::{LabelItem, LabelTemplate};
use self::raster::TextRaster;
use self::templates::HeldSlip;
use crate::error::{AppError, AppResult};
use crate::services::accounting::DaySummary;
use crate::services::payments::RegisterSession;
use crate::services::sales::{InvoiceDetail, ReturnDetail};
use crate::services::settings::PublicSettings;

/// A document to print, with the figures the backend produced (the same ones the screen shows).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrintDocument {
    Receipt {
        invoice: Box<InvoiceDetail>,
        /// A copy printed later: marked "REPRINT".
        #[serde(default)]
        reprint: bool,
    },
    ReturnSlip {
        #[serde(rename = "salesReturn")]
        sales_return: Box<ReturnDetail>,
        #[serde(default, rename = "invoiceNo")]
        invoice_no: Option<String>,
    },
    TillClose {
        session: Box<RegisterSession>,
    },
    /// The day-close summary (printed on the till-close route).
    DayClose {
        summary: Box<DaySummary>,
    },
    HeldSlip {
        slip: HeldSlip,
    },
    Labels {
        template: LabelTemplate,
        items: Vec<LabelItem>,
    },
    /// The printer test page.
    Test,
}

impl PrintDocument {
    /// Which route prints it (the test page always names its printer).
    pub fn kind(&self) -> Option<DocKind> {
        match self {
            PrintDocument::Receipt { .. } => Some(DocKind::Receipt),
            PrintDocument::ReturnSlip { .. } => Some(DocKind::ReturnSlip),
            PrintDocument::TillClose { .. } | PrintDocument::DayClose { .. } => {
                Some(DocKind::TillClose)
            }
            PrintDocument::HeldSlip { .. } => Some(DocKind::HeldSlip),
            PrintDocument::Labels { .. } => Some(DocKind::Labels),
            PrintDocument::Test => None,
        }
    }

    /// A short name for queues and logs ("Receipt INV/26-27/00012").
    pub fn title(&self) -> String {
        match self {
            PrintDocument::Receipt { invoice, .. } => format!("Receipt {}", invoice.doc_no),
            PrintDocument::ReturnSlip { sales_return, .. } => {
                format!("Credit note {}", sales_return.doc_no)
            }
            PrintDocument::TillClose { session } => format!("Till close {}", session.register_name),
            PrintDocument::DayClose { summary } => format!("Day close {}", summary.business_date),
            PrintDocument::HeldSlip { slip } => format!("Held bill {}", slip.label),
            PrintDocument::Labels { items, .. } => {
                let count: i64 = items.iter().map(|i| i.copies.max(0)).sum();
                format!("{count} labels")
            }
            PrintDocument::Test => "Test page".into(),
        }
    }

    /// Whether a sale was paid (partly) in cash: the drawer opens for it.
    pub fn takes_cash(&self) -> bool {
        match self {
            PrintDocument::Receipt { invoice, reprint } => {
                !reprint
                    && invoice
                        .payments
                        .iter()
                        .any(|p| p.mode == "cash" && p.status != "voided")
            }
            _ => false,
        }
    }
}

/// The font for pictures of text: the one the printer settings name, else the system one.
pub fn font(config: &PrinterConfig) -> Option<&'static TextRaster> {
    static CUSTOM: OnceLock<Mutex<HashMap<String, Option<&'static TextRaster>>>> = OnceLock::new();
    if let Some(path) = config.font_path.as_deref() {
        let cache = CUSTOM.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut cache) = cache.lock() {
            let loaded = *cache.entry(path.to_string()).or_insert_with(|| {
                TextRaster::from_file(std::path::Path::new(path), 0)
                    .map(|f| &*Box::leak(Box::new(f)))
            });
            if loaded.is_some() {
                return loaded;
            }
        }
    }
    TextRaster::system()
}

/// The bytes for a document on a printer.
pub fn render(
    doc: &PrintDocument,
    profile: &PrinterProfile,
    settings: &PublicSettings,
    now: DateTime<Utc>,
    font: Option<&TextRaster>,
    kick_drawer: bool,
) -> AppResult<Vec<u8>> {
    let wanted = match doc {
        PrintDocument::Labels { .. } => PrinterKind::Label,
        _ => PrinterKind::Receipt,
    };
    if profile.kind != wanted && !matches!(doc, PrintDocument::Test) {
        return Err(AppError::validation(match wanted {
            PrinterKind::Label => "Labels need a label printer.",
            PrinterKind::Receipt => "Receipts and slips need a receipt printer.",
        }));
    }
    let laid_out = match doc {
        PrintDocument::Receipt { invoice, reprint } => {
            templates::receipt(invoice, settings, *reprint)
        }
        PrintDocument::ReturnSlip {
            sales_return,
            invoice_no,
        } => templates::return_slip(sales_return, invoice_no.as_deref(), settings),
        PrintDocument::TillClose { session } => templates::till_close(session, settings, now),
        PrintDocument::DayClose { summary } => templates::day_close(summary, settings, now),
        PrintDocument::HeldSlip { slip } => templates::held_slip(slip, settings),
        PrintDocument::Labels { template, items } => {
            return labels::render(
                template,
                items,
                settings,
                profile.label_language,
                profile.label_dpi,
            );
        }
        PrintDocument::Test if profile.kind == PrinterKind::Label => {
            return labels::render(
                &test_label(),
                &[test_label_item()],
                settings,
                profile.label_language,
                profile.label_dpi,
            );
        }
        PrintDocument::Test => templates::test_page(profile, settings, now),
    };
    Ok(escpos::render(&laid_out, profile, font, kick_drawer))
}

fn test_label() -> LabelTemplate {
    LabelTemplate {
        id: None,
        name: "Test".into(),
        width_mm: 50,
        height_mm: 25,
        gap_mm: 2,
        columns: 1,
        symbology: barcode::Symbology::Code128,
        show_business: true,
        show_name: true,
        show_price: true,
        show_mrp: false,
        show_sku: false,
        show_barcode: true,
        show_batch: false,
        show_expiry: false,
        updated_at: None,
    }
}

fn test_label_item() -> LabelItem {
    LabelItem {
        variant_id: None,
        name: "Printer test".into(),
        sku: Some("TEST-128".into()),
        barcode: None,
        price: 11_800,
        mrp: None,
        batch_no: None,
        expiry_date: None,
        copies: 1,
    }
}

/// What happened to a print request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrintStatus {
    /// Handed to the printer.
    Printed,
    /// Waiting in the hub's queue (it retries).
    Queued,
    /// No printer is set up for this document: print through the system dialog instead.
    NoPrinter,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintOutcome {
    pub status: PrintStatus,
    pub printer_name: Option<String>,
    /// The hub's print-queue entry, when it went through the hub.
    pub job_id: Option<String>,
    pub message: Option<String>,
}
