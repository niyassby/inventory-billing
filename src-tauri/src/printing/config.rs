//! This PC's printers and which document goes where (`printers.json` next to the database, like
//! `device.json`). It is per PC because ports and Windows printer names belong to the PC; a
//! counter can also send documents to a printer the hub shares (the hub's print queue).

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::text::CodePage;
use super::transport::{self, PrinterConnection};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrinterKind {
    /// ESC/POS thermal receipt printer.
    Receipt,
    /// ZPL or TSPL label printer.
    Label,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaperWidth {
    Mm58,
    #[default]
    Mm80,
}

impl PaperWidth {
    /// Characters a line in the standard font (12 × 24 dots).
    pub fn columns(self) -> usize {
        match self {
            PaperWidth::Mm58 => 32,
            PaperWidth::Mm80 => 48,
        }
    }

    /// Printable dots across (8 dots/mm).
    pub fn dots(self) -> usize {
        match self {
            PaperWidth::Mm58 => 384,
            PaperWidth::Mm80 => 576,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum RupeeStyle {
    /// Print ₹ (as a picture: no printer code page has it).
    #[default]
    Symbol,
    /// Print "Rs." (faster, works without the picture fallback).
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutMode {
    None,
    #[default]
    Partial,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum LabelLanguage {
    #[default]
    Zpl,
    Tspl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum DrawerPin {
    /// Connector pin 2 (most drawers).
    #[default]
    Pin2,
    Pin5,
}

fn default_true() -> bool {
    true
}

fn default_one() -> u8 {
    1
}

fn default_feed() -> u8 {
    4
}

fn default_dpi() -> u16 {
    203
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrinterProfile {
    /// Empty for a new printer (an id is given on save).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub kind: PrinterKind,
    pub connection: PrinterConnection,
    #[serde(default)]
    pub paper: PaperWidth,
    #[serde(default)]
    pub code_page: CodePage,
    #[serde(default)]
    pub rupee: RupeeStyle,
    /// Print text the code page lacks (Hindi, Malayalam…) as a picture instead of "?".
    #[serde(default = "default_true")]
    pub picture_fallback: bool,
    #[serde(default)]
    pub cut: CutMode,
    /// Blank lines before the cut, so the last line clears the cutter.
    #[serde(default = "default_feed")]
    pub feed_lines: u8,
    #[serde(default = "default_one")]
    pub copies: u8,
    /// A cash drawer is plugged into this printer.
    #[serde(default)]
    pub has_drawer: bool,
    #[serde(default)]
    pub drawer_pin: DrawerPin,
    #[serde(default)]
    pub label_language: LabelLanguage,
    #[serde(default = "default_dpi")]
    pub label_dpi: u16,
    /// On the hub: counters may print to it through the hub's print queue.
    #[serde(default)]
    pub shared: bool,
}

/// Where one kind of document is printed: a printer of this PC, or one the hub shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintRoute {
    pub printer_id: String,
    #[serde(default)]
    pub on_hub: bool,
}

/// Document kinds that can be routed to a printer. Without a route the screen prints through the
/// system print dialog as before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Receipt,
    ReturnSlip,
    TillClose,
    HeldSlip,
    Labels,
}

impl DocKind {
    pub fn printer_kind(self) -> PrinterKind {
        match self {
            DocKind::Labels => PrinterKind::Label,
            _ => PrinterKind::Receipt,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Routes {
    pub receipt: Option<PrintRoute>,
    pub return_slip: Option<PrintRoute>,
    pub till_close: Option<PrintRoute>,
    pub held_slip: Option<PrintRoute>,
    pub labels: Option<PrintRoute>,
}

impl Routes {
    pub fn get(&self, kind: DocKind) -> Option<&PrintRoute> {
        match kind {
            DocKind::Receipt => self.receipt.as_ref(),
            DocKind::ReturnSlip => self.return_slip.as_ref(),
            DocKind::TillClose => self.till_close.as_ref(),
            DocKind::HeldSlip => self.held_slip.as_ref(),
            DocKind::Labels => self.labels.as_ref(),
        }
    }

    fn all(&self) -> [(DocKind, Option<&PrintRoute>); 5] {
        [
            DocKind::Receipt,
            DocKind::ReturnSlip,
            DocKind::TillClose,
            DocKind::HeldSlip,
            DocKind::Labels,
        ]
        .map(|k| (k, self.get(k)))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrinterConfig {
    #[serde(default)]
    pub printers: Vec<PrinterProfile>,
    #[serde(default)]
    pub routes: Routes,
    /// Print the receipt as soon as a sale is saved.
    #[serde(default)]
    pub auto_print_receipt: bool,
    /// Open the drawer when a sale is paid (partly) in cash.
    #[serde(default)]
    pub drawer_on_cash: bool,
    /// The receipt printer the cash drawer is plugged into.
    #[serde(default)]
    pub drawer_printer_id: Option<String>,
    /// A font file with the rupee sign and Indian scripts, when the system one won't do.
    #[serde(default)]
    pub font_path: Option<String>,
}

pub const FILE_NAME: &str = "printers.json";

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

fn field(name: impl Into<String>, message: impl Into<String>) -> AppError {
    AppError::field(&name.into(), message.into())
}

impl PrinterConfig {
    /// The saved configuration, or an empty one (no printers: browser printing as before).
    pub fn load(data_dir: &Path) -> Self {
        let file = path(data_dir);
        match std::fs::read_to_string(&file) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::error!("{} is unreadable ({e}); using no printers", file.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes the file atomically (a temporary file, then a rename).
    pub fn save(&self, data_dir: &Path) -> AppResult<()> {
        std::fs::create_dir_all(data_dir).map_err(AppError::internal)?;
        let file = path(data_dir);
        let temp = file.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(AppError::internal)?;
        std::fs::write(&temp, text).map_err(AppError::internal)?;
        std::fs::rename(&temp, &file).map_err(AppError::internal)?;
        Ok(())
    }

    pub fn printer(&self, id: &str) -> Option<&PrinterProfile> {
        self.printers.iter().find(|p| p.id == id)
    }

    /// Cleans and checks a configuration before it is saved: names, addresses, limits, and that
    /// every route and the drawer point at a suitable printer of this PC.
    pub fn validated(mut self) -> AppResult<Self> {
        if self.printers.len() > 20 {
            return Err(AppError::validation("At most 20 printers per PC."));
        }
        let mut names = std::collections::HashSet::new();
        for (i, p) in self.printers.iter_mut().enumerate() {
            p.name = p.name.trim().chars().take(60).collect();
            if p.name.is_empty() {
                return Err(field(
                    format!("printers.{i}.name"),
                    "Give the printer a name.",
                ));
            }
            if !names.insert(p.name.to_lowercase()) {
                return Err(field(
                    format!("printers.{i}.name"),
                    "Two printers have this name.",
                ));
            }
            if p.id.trim().is_empty() {
                p.id = new_id();
            }
            if let PrinterConnection::Network { host, .. } = &mut p.connection {
                *host = host.trim().to_string();
            }
            if let Err((name, message)) = transport::validate(&p.connection) {
                return Err(field(format!("printers.{i}.{name}"), message));
            }
            if !(1..=5).contains(&p.copies) {
                return Err(field(
                    format!("printers.{i}.copies"),
                    "Print 1 to 5 copies.",
                ));
            }
            if p.feed_lines > 10 {
                return Err(field(
                    format!("printers.{i}.feedLines"),
                    "Feed at most 10 lines.",
                ));
            }
            if ![203, 300].contains(&p.label_dpi) {
                return Err(field(
                    format!("printers.{i}.labelDpi"),
                    "Choose 203 or 300 dpi.",
                ));
            }
            if p.kind == PrinterKind::Label {
                p.has_drawer = false;
            }
        }
        let mut ids = std::collections::HashSet::new();
        if !self.printers.iter().all(|p| ids.insert(p.id.clone())) {
            return Err(AppError::validation("Two printers have the same id."));
        }
        for (kind, route) in self.routes.all() {
            let Some(route) = route else { continue };
            if route.on_hub {
                if route.printer_id.trim().is_empty() {
                    return Err(AppError::validation("Choose the hub's printer."));
                }
                continue;
            }
            match self.printer(&route.printer_id) {
                Some(p) if p.kind == kind.printer_kind() => {}
                Some(_) => {
                    return Err(AppError::validation(match kind {
                        DocKind::Labels => "Labels need a label printer.",
                        _ => "Receipts and slips need a receipt printer.",
                    }))
                }
                None => {
                    return Err(AppError::validation(
                        "A route points at a printer that no longer exists.",
                    ))
                }
            }
        }
        if let Some(id) = &self.drawer_printer_id {
            match self.printer(id) {
                Some(p) if p.kind == PrinterKind::Receipt && p.has_drawer => {}
                _ => {
                    return Err(field(
                        "drawerPrinterId",
                        "The drawer must be on a receipt printer marked \"cash drawer attached\".",
                    ))
                }
            }
        }
        if self.drawer_on_cash && self.drawer_printer_id.is_none() {
            return Err(field(
                "drawerOnCash",
                "Choose the printer the cash drawer is plugged into.",
            ));
        }
        self.font_path = self
            .font_path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        Ok(self)
    }

    /// What changed, for the audit log (no addresses of other PCs, nothing secret).
    pub fn summary(&self) -> serde_json::Value {
        json!({
            "printers": self.printers.iter().map(|p| json!({
                "name": p.name, "kind": p.kind, "connection": p.connection.describe(), "shared": p.shared,
            })).collect::<Vec<_>>(),
            "routes": self.routes,
            "autoPrintReceipt": self.auto_print_receipt,
            "drawerOnCash": self.drawer_on_cash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn receipt_printer(name: &str, port: u16) -> PrinterProfile {
        PrinterProfile {
            id: String::new(),
            name: name.into(),
            kind: PrinterKind::Receipt,
            connection: PrinterConnection::Network {
                host: "127.0.0.1".into(),
                port,
            },
            paper: PaperWidth::Mm80,
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

    #[test]
    fn saves_and_loads_atomically() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(PrinterConfig::load(dir.path()), PrinterConfig::default());
        let config = PrinterConfig {
            printers: vec![receipt_printer("Front", 9100)],
            ..PrinterConfig::default()
        }
        .validated()
        .unwrap();
        assert!(!config.printers[0].id.is_empty(), "an id is given on save");
        config.save(dir.path()).unwrap();
        assert_eq!(PrinterConfig::load(dir.path()), config);
        assert!(!dir.path().join("printers.json.tmp").exists());
    }

    #[test]
    fn refuses_bad_names_routes_and_drawers() {
        let two = PrinterConfig {
            printers: vec![
                receipt_printer("Front", 9100),
                receipt_printer(" front ", 9101),
            ],
            ..PrinterConfig::default()
        };
        assert!(two.validated().is_err(), "duplicate names");

        let mut config = PrinterConfig {
            printers: vec![receipt_printer("Front", 9100)],
            ..PrinterConfig::default()
        }
        .validated()
        .unwrap();
        let id = config.printers[0].id.clone();
        config.routes.labels = Some(PrintRoute {
            printer_id: id.clone(),
            on_hub: false,
        });
        assert!(
            config.clone().validated().is_err(),
            "labels on a receipt printer"
        );

        config.routes.labels = None;
        config.drawer_on_cash = true;
        assert!(
            config.clone().validated().is_err(),
            "drawer without a printer"
        );
        config.drawer_printer_id = Some(id);
        assert!(config.clone().validated().is_ok());

        config.printers[0].connection = PrinterConnection::Network {
            host: "8.8.8.8".into(),
            port: 9100,
        };
        let err = config.validated().unwrap_err();
        assert!(err.message.contains("shop network"), "{}", err.message);
    }
}
