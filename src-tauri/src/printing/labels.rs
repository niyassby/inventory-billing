//! Barcode labels. One layout (in millimetres) per label is computed from the template and the
//! item, then drawn three ways: ZPL (Zebra and compatibles), TSPL (TSC, Xprinter and most cheap
//! label printers) and SVG (the preview and A4 sheets), so what is previewed is what prints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::barcode::{self, Symbology};
use super::config::LabelLanguage;
use super::text;
use crate::error::{AppError, AppResult};
use crate::services::settings::PublicSettings;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplate {
    /// Absent for a template that isn't saved yet.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub width_mm: i64,
    pub height_mm: i64,
    /// Gap between labels on the roll.
    #[serde(default)]
    pub gap_mm: i64,
    /// Labels side by side on the roll (1–4).
    #[serde(default = "one")]
    pub columns: i64,
    #[serde(default)]
    pub symbology: Symbology,
    #[serde(default)]
    pub show_business: bool,
    #[serde(default = "yes")]
    pub show_name: bool,
    #[serde(default = "yes")]
    pub show_price: bool,
    #[serde(default)]
    pub show_mrp: bool,
    #[serde(default)]
    pub show_sku: bool,
    #[serde(default = "yes")]
    pub show_barcode: bool,
    #[serde(default)]
    pub show_batch: bool,
    #[serde(default)]
    pub show_expiry: bool,
    /// For edits: the `updatedAt` the editor loaded.
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn one() -> i64 {
    1
}

fn yes() -> bool {
    true
}

impl LabelTemplate {
    pub fn validate(&self) -> AppResult<()> {
        let field = |name: &str, message: &str| Err(AppError::field(name, message));
        if self.name.trim().is_empty() || self.name.chars().count() > 60 {
            return field("name", "Give the template a name (at most 60 characters).");
        }
        if !(15..=150).contains(&self.width_mm) {
            return field("widthMm", "Labels are 15 to 150 mm wide.");
        }
        if !(10..=150).contains(&self.height_mm) {
            return field("heightMm", "Labels are 10 to 150 mm high.");
        }
        if !(0..=10).contains(&self.gap_mm) {
            return field("gapMm", "The gap is 0 to 10 mm.");
        }
        if !(1..=4).contains(&self.columns) {
            return field("columns", "1 to 4 labels across.");
        }
        if self.width_mm * self.columns > 220 {
            return field("columns", "That many labels across don't fit on a roll.");
        }
        Ok(())
    }
}

/// One product to label, with how many copies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelItem {
    #[serde(default)]
    pub variant_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    pub price: i64,
    #[serde(default)]
    pub mrp: Option<i64>,
    #[serde(default)]
    pub batch_no: Option<String>,
    #[serde(default)]
    pub expiry_date: Option<String>,
    pub copies: i64,
}

pub const MAX_LABELS: i64 = 2_000;

/// Copies expanded, checked against the per-job limit.
pub fn expand(items: &[LabelItem]) -> AppResult<Vec<&LabelItem>> {
    let total: i64 = items.iter().map(|i| i.copies.max(0)).sum();
    if total == 0 {
        return Err(AppError::validation("Choose at least one label to print."));
    }
    if total > MAX_LABELS {
        return Err(AppError::validation(format!(
            "At most {MAX_LABELS} labels in one go."
        )));
    }
    Ok(items
        .iter()
        .flat_map(|i| std::iter::repeat_n(i, usize::try_from(i.copies.max(0)).unwrap_or(0)))
        .collect())
}

/// Something drawn on a label, positioned in millimetres from the label's top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Text {
        x: f64,
        y: f64,
        /// Character height.
        h: f64,
        text: String,
        bold: bool,
    },
    Linear {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        symbology: Symbology,
        data: String,
    },
    Qr {
        x: f64,
        y: f64,
        size: f64,
        data: String,
    },
}

const MARGIN: f64 = 1.5;

/// Cuts text to what fits `width` mm at character height `h` (glyphs about 0.55 h wide).
fn fit(text: &str, width: f64, h: f64) -> String {
    let max = ((width / (h * 0.55)).floor() as usize).max(1);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        text.to_string()
    } else {
        chars[..max.saturating_sub(1)].iter().collect::<String>() + "…"
    }
}

fn money_text(minor: i64, settings: &PublicSettings, rupee: &str) -> String {
    let b = &settings.business;
    let amount = text::money(minor, b.currency_minor_digits, &b.locale);
    if b.currency == "INR" {
        format!("{rupee}{amount}")
    } else {
        format!("{} {amount}", b.currency)
    }
}

/// The data for the barcode: the item's barcode, else its SKU.
pub fn barcode_data(item: &LabelItem) -> Option<String> {
    item.barcode
        .as_deref()
        .or(item.sku.as_deref())
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(String::from)
}

/// Lays out one label. `rupee` is "₹" for pictures and SVG, "Rs." for printer fonts.
pub fn layout(
    t: &LabelTemplate,
    item: &LabelItem,
    settings: &PublicSettings,
    rupee: &str,
) -> (Vec<Element>, Option<Symbology>) {
    let width = t.width_mm as f64 - 2.0 * MARGIN;
    let height = t.height_mm as f64;
    let small = (height / 9.0).clamp(1.8, 3.0);
    let normal = (height / 7.0).clamp(2.2, 3.6);
    let mut y = MARGIN;
    let mut out = Vec::new();
    let push_text = |out: &mut Vec<Element>, y: &mut f64, text: String, h: f64, bold: bool| {
        out.push(Element::Text {
            x: MARGIN,
            y: *y,
            h,
            text: fit(&text, width, h),
            bold,
        });
        *y += h * 1.25;
    };
    if t.show_business {
        push_text(
            &mut out,
            &mut y,
            settings.business.name.clone(),
            small,
            false,
        );
    }
    if t.show_name {
        let max = ((width / (normal * 0.55)).floor() as usize).max(1);
        let lines = text::wrap(&item.name, max);
        for line in lines.iter().take(2) {
            push_text(&mut out, &mut y, line.clone(), normal, true);
        }
    }
    // Prices are never cut short: they share a line when they fit, else MRP gets its own line,
    // and a line that still doesn't fit is printed smaller.
    let price = t
        .show_price
        .then(|| money_text(item.price, settings, rupee));
    let mrp = item
        .mrp
        .filter(|_| t.show_mrp)
        .map(|m| format!("MRP {}", money_text(m, settings, rupee)));
    let fits = |text: &str, h: f64| text.chars().count() as f64 * h * 0.55 <= width;
    let sized = |text: &str, h: f64| -> f64 {
        let n = text.chars().count().max(1) as f64;
        h.min(width / (n * 0.55)).max(1.6)
    };
    match (price, mrp) {
        (Some(p), Some(m)) if fits(&format!("{p}  {m}"), normal) => {
            push_text(&mut out, &mut y, format!("{p}  {m}"), normal, true);
        }
        (Some(p), Some(m)) => {
            let h = sized(&p, normal);
            push_text(&mut out, &mut y, p, h, true);
            let h = sized(&m, small);
            push_text(&mut out, &mut y, m, h, false);
        }
        (Some(text), None) => {
            let h = sized(&text, normal);
            push_text(&mut out, &mut y, text, h, true);
        }
        (None, Some(text)) => {
            let h = sized(&text, small);
            push_text(&mut out, &mut y, text, h, false);
        }
        (None, None) => {}
    }
    let mut details = Vec::new();
    if t.show_sku {
        if let Some(sku) = item.sku.as_deref().filter(|s| !s.is_empty()) {
            details.push(sku.to_string());
        }
    }
    if t.show_batch {
        if let Some(batch) = item.batch_no.as_deref().filter(|s| !s.is_empty()) {
            details.push(format!("B:{batch}"));
        }
    }
    if t.show_expiry {
        if let Some(expiry) = item.expiry_date.as_deref().filter(|s| s.len() >= 7) {
            details.push(format!("EXP {}/{}", &expiry[5..7], &expiry[..4]));
        }
    }
    let details_h = if details.is_empty() {
        0.0
    } else {
        small * 1.25
    };
    let mut used = None;
    if t.show_barcode {
        if let Some(data) = barcode_data(item) {
            used = barcode::choose(t.symbology, &data);
            let room = height - MARGIN - details_h - y;
            match used {
                Some(Symbology::Qr) if room >= 6.0 => {
                    let size = room.min(width);
                    out.push(Element::Qr {
                        x: MARGIN,
                        y,
                        size,
                        data,
                    });
                    y += size + 0.5;
                }
                Some(symbology) if room >= 5.0 => {
                    // The printer prints the digits under the bars: keep room for them.
                    let h = (room - 3.0).max(3.0);
                    out.push(Element::Linear {
                        x: MARGIN,
                        y,
                        w: width,
                        h,
                        symbology,
                        data,
                    });
                    y += room;
                }
                _ => used = None,
            }
        }
    }
    if !details.is_empty() {
        push_text(&mut out, &mut y, details.join("  "), small, false);
    }
    (out, used)
}

fn dots(mm: f64, dpi: u16) -> i64 {
    (mm * f64::from(dpi) / 25.4).round() as i64
}

/// Printer-font text: no ₹ and nothing that ends a field.
fn printer_text(text: &str) -> String {
    text::simplify(text)
        .replace('₹', "Rs.")
        .replace('…', "...")
        .chars()
        .map(|c| {
            if matches!(c, '^' | '~' | '"' | '\\') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn module_width(width_dots: i64, modules: usize) -> i64 {
    (width_dots / i64::try_from(modules.max(1)).unwrap_or(1)).clamp(1, 4)
}

fn modules_of(symbology: Symbology, data: &str) -> usize {
    barcode::linear_modules(symbology, data).map_or(1, |m| m.len())
}

/// Data the printer encodes (EAN without its check digit: the printer adds it).
fn printer_barcode_data(symbology: Symbology, data: &str) -> String {
    match symbology {
        Symbology::Ean13 => data[..12].to_string(),
        Symbology::Ean8 => data[..7].to_string(),
        _ => printer_text(data),
    }
}

fn zpl_row(t: &LabelTemplate, row: &[&LabelItem], settings: &PublicSettings, dpi: u16) -> String {
    let pitch = t.width_mm + t.gap_mm;
    let mut out = format!(
        "^XA^CI28^PW{}^LL{}^LH0,0\n",
        dots((pitch * t.columns - t.gap_mm) as f64, dpi),
        dots(t.height_mm as f64, dpi)
    );
    for (col, item) in row.iter().enumerate() {
        let dx = (pitch * col as i64) as f64;
        let (elements, _) = layout(t, item, settings, "Rs.");
        for e in elements {
            match e {
                Element::Text { x, y, h, text, .. } => {
                    let hd = dots(h, dpi);
                    out.push_str(&format!(
                        "^FO{},{}^A0N,{hd},{}^FD{}^FS\n",
                        dots(x + dx, dpi),
                        dots(y, dpi),
                        (hd * 9) / 10,
                        printer_text(&text)
                    ));
                }
                Element::Linear {
                    x,
                    y,
                    w,
                    h,
                    symbology,
                    data,
                } => {
                    let module = module_width(dots(w, dpi), modules_of(symbology, &data));
                    let command = match symbology {
                        Symbology::Ean13 => "^BEN",
                        Symbology::Ean8 => "^B8N",
                        _ => "^BCN",
                    };
                    let tail = if symbology == Symbology::Code128 {
                        ",Y,N,N,A"
                    } else {
                        ",Y,N"
                    };
                    out.push_str(&format!(
                        "^FO{},{}^BY{module},2{command},{}{tail}^FD{}^FS\n",
                        dots(x + dx, dpi),
                        dots(y, dpi),
                        dots(h, dpi),
                        printer_barcode_data(symbology, &data)
                    ));
                }
                Element::Qr { x, y, size, data } => {
                    let width = barcode::qr_matrix(&data).map_or(25, |(w, _)| w) as i64;
                    let magnification = (dots(size, dpi) / width).clamp(1, 10);
                    out.push_str(&format!(
                        "^FO{},{}^BQN,2,{magnification}^FDMA,{}^FS\n",
                        dots(x + dx, dpi),
                        dots(y, dpi),
                        printer_text(&data)
                    ));
                }
            }
        }
    }
    out.push_str("^PQ1\n^XZ\n");
    out
}

fn tspl_row(t: &LabelTemplate, row: &[&LabelItem], settings: &PublicSettings, dpi: u16) -> String {
    let pitch = t.width_mm + t.gap_mm;
    let mut out = String::from("CLS\r\n");
    for (col, item) in row.iter().enumerate() {
        let dx = (pitch * col as i64) as f64;
        let (elements, _) = layout(t, item, settings, "Rs.");
        for e in elements {
            match e {
                Element::Text { x, y, h, text, .. } => {
                    // Built-in fonts: "2" is 12 × 20 dots, "3" 16 × 24 dots (at 203 dpi).
                    let font = if dots(h, dpi) >= 22 { "3" } else { "2" };
                    out.push_str(&format!(
                        "TEXT {},{},\"{font}\",0,1,1,\"{}\"\r\n",
                        dots(x + dx, dpi),
                        dots(y, dpi),
                        printer_text(&text)
                    ));
                }
                Element::Linear {
                    x,
                    y,
                    w,
                    h,
                    symbology,
                    data,
                } => {
                    let module = module_width(dots(w, dpi), modules_of(symbology, &data));
                    let kind = match symbology {
                        Symbology::Ean13 => "EAN13",
                        Symbology::Ean8 => "EAN8",
                        _ => "128",
                    };
                    out.push_str(&format!(
                        "BARCODE {},{},\"{kind}\",{},1,0,{module},{module},\"{}\"\r\n",
                        dots(x + dx, dpi),
                        dots(y, dpi),
                        dots(h, dpi),
                        printer_barcode_data(symbology, &data)
                    ));
                }
                Element::Qr { x, y, size, data } => {
                    let width = barcode::qr_matrix(&data).map_or(25, |(w, _)| w) as i64;
                    let cell = (dots(size, dpi) / width).clamp(1, 10);
                    out.push_str(&format!(
                        "QRCODE {},{},M,{cell},A,0,\"{}\"\r\n",
                        dots(x + dx, dpi),
                        dots(y, dpi),
                        printer_text(&data)
                    ));
                }
            }
        }
    }
    out.push_str("PRINT 1,1\r\n");
    out
}

/// The print job for a label printer.
pub fn render(
    t: &LabelTemplate,
    items: &[LabelItem],
    settings: &PublicSettings,
    language: LabelLanguage,
    dpi: u16,
) -> AppResult<Vec<u8>> {
    t.validate()?;
    let labels = expand(items)?;
    let columns = usize::try_from(t.columns).unwrap_or(1).max(1);
    let mut out = String::new();
    if language == LabelLanguage::Tspl {
        let pitch = t.width_mm + t.gap_mm;
        out.push_str(&format!(
            "SIZE {} mm,{} mm\r\nGAP {} mm,0 mm\r\nDIRECTION 1\r\nCODEPAGE UTF-8\r\n",
            pitch * t.columns - t.gap_mm,
            t.height_mm,
            t.gap_mm
        ));
    }
    for row in labels.chunks(columns) {
        out.push_str(&match language {
            LabelLanguage::Zpl => zpl_row(t, row, settings, dpi),
            LabelLanguage::Tspl => tspl_row(t, row, settings, dpi),
        });
    }
    Ok(out.into_bytes())
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Runs of bars as (start, length) in modules.
fn bar_runs(modules: &[bool]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < modules.len() {
        if modules[i] {
            let start = i;
            while i < modules.len() && modules[i] {
                i += 1;
            }
            runs.push((start, i - start));
        } else {
            i += 1;
        }
    }
    runs
}

/// One label as SVG, sized in millimetres (preview and A4 sheets).
pub fn svg(
    t: &LabelTemplate,
    item: &LabelItem,
    settings: &PublicSettings,
) -> (String, Option<Symbology>) {
    let (elements, used) = layout(t, item, settings, "₹");
    let (w, h) = (t.width_mm, t.height_mm);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\
<rect width=\"{w}\" height=\"{h}\" fill=\"#fff\"/>"
    );
    for e in elements {
        match e {
            Element::Text { x, y, h, text, bold } => out.push_str(&format!(
                "<text x=\"{x:.2}\" y=\"{:.2}\" font-family=\"Arial, sans-serif\" font-size=\"{h:.2}\"{} fill=\"#000\">{}</text>",
                y + h * 0.85,
                if bold { " font-weight=\"700\"" } else { "" },
                xml(&text)
            )),
            Element::Linear { x, y, w, h, symbology, data } => {
                let Ok(modules) = barcode::linear_modules(symbology, &data) else { continue };
                let unit = w / modules.len() as f64;
                for (start, len) in bar_runs(&modules) {
                    out.push_str(&format!(
                        "<rect x=\"{:.3}\" y=\"{y:.2}\" width=\"{:.3}\" height=\"{h:.2}\" fill=\"#000\"/>",
                        x + start as f64 * unit,
                        len as f64 * unit
                    ));
                }
                let th = 2.2_f64.min(h / 2.0);
                out.push_str(&format!(
                    "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"Arial, sans-serif\" font-size=\"{th:.2}\" text-anchor=\"middle\" fill=\"#000\">{}</text>",
                    x + w / 2.0,
                    y + h + th,
                    xml(&data)
                ));
            }
            Element::Qr { x, y, size, data } => {
                let Ok((width, dark)) = barcode::qr_matrix(&data) else { continue };
                let unit = size / width as f64;
                for row in 0..width {
                    for (start, len) in bar_runs(&dark[row * width..(row + 1) * width]) {
                        out.push_str(&format!(
                            "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#000\"/>",
                            x + start as f64 * unit,
                            y + row as f64 * unit,
                            len as f64 * unit,
                            unit
                        ));
                    }
                }
            }
        }
    }
    out.push_str("</svg>");
    (out, used)
}
