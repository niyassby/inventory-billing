//! ESC/POS bytes for thermal receipt printers (Epson TM and the many compatible 58/80 mm
//! printers). `render` turns a laid-out document (templates.rs) into one print job: text in the
//! printer's code page, lines the code page can't show as pictures, barcodes and QR codes drawn by
//! the printer, the cut, and the drawer pulse.

use super::config::{CutMode, DrawerPin, PrinterProfile, RupeeStyle};
use super::raster::{Bitmap, Placement, TextRaster};
use super::templates::{Block, Doc, Size};
use super::text::{self, CodePage};

const ESC: u8 = 0x1b;
const GS: u8 = 0x1d;
const LF: u8 = 0x0a;

pub struct EscPos {
    buf: Vec<u8>,
    page: CodePage,
}

impl EscPos {
    /// Resets the printer and selects the code page.
    pub fn new(page: CodePage) -> Self {
        let mut p = Self {
            buf: Vec::with_capacity(2048),
            page,
        };
        p.raw(&[ESC, b'@', ESC, b't', page.escpos_number()]);
        p
    }

    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    pub fn align(&mut self, placement: Placement) -> &mut Self {
        let n = match placement {
            Placement::Left => 0,
            Placement::Center => 1,
            Placement::Right => 2,
        };
        self.raw(&[ESC, b'a', n])
    }

    pub fn bold(&mut self, on: bool) -> &mut Self {
        self.raw(&[ESC, b'E', u8::from(on)])
    }

    /// Normal or double width and height.
    pub fn size(&mut self, size: Size) -> &mut Self {
        let n = match size {
            Size::Normal => 0x00,
            Size::Large => 0x11,
        };
        self.raw(&[GS, b'!', n])
    }

    pub fn line(&mut self, text: &str) -> &mut Self {
        let bytes = text::encode(self.page, text);
        self.raw(&bytes).raw(&[LF])
    }

    pub fn feed(&mut self, lines: u8) -> &mut Self {
        if lines > 0 {
            self.raw(&[ESC, b'd', lines]);
        }
        self
    }

    pub fn cut(&mut self, mode: CutMode) -> &mut Self {
        match mode {
            CutMode::None => self,
            CutMode::Partial => self.raw(&[GS, b'V', 1]),
            CutMode::Full => self.raw(&[GS, b'V', 0]),
        }
    }

    /// A pulse on the drawer connector (`ESC p m t1 t2`: 50 ms on, 500 ms off).
    pub fn drawer(&mut self, pin: DrawerPin) -> &mut Self {
        let m = match pin {
            DrawerPin::Pin2 => 0,
            DrawerPin::Pin5 => 1,
        };
        self.raw(&[ESC, b'p', m, 25, 250])
    }

    /// A picture, as raster bit image (`GS v 0`).
    pub fn image(&mut self, bitmap: &Bitmap) -> &mut Self {
        let x = u16::try_from(bitmap.bytes_per_row()).unwrap_or(u16::MAX);
        let y = u16::try_from(bitmap.height).unwrap_or(u16::MAX);
        let [xl, xh] = x.to_le_bytes();
        let [yl, yh] = y.to_le_bytes();
        self.raw(&[GS, b'v', b'0', 0, xl, xh, yl, yh]);
        self.raw(&bitmap.data)
    }

    /// Code 128 drawn by the printer, with the text under it.
    pub fn code128(&mut self, data: &str) -> &mut Self {
        let payload: Vec<u8> = b"{B".iter().copied().chain(data.bytes()).collect();
        let n = u8::try_from(payload.len()).unwrap_or(u8::MAX);
        self.raw(&[GS, b'h', 60, GS, b'w', 2, GS, b'H', 2, GS, b'k', 73, n]);
        self.raw(&payload).raw(&[LF])
    }

    /// QR code drawn by the printer (model 2, error correction M, 6-dot modules).
    pub fn qr(&mut self, data: &str) -> &mut Self {
        let len = u16::try_from(data.len() + 3).unwrap_or(u16::MAX);
        let [pl, ph] = len.to_le_bytes();
        self.raw(&[GS, b'(', b'k', 4, 0, 49, 65, 50, 0]);
        self.raw(&[GS, b'(', b'k', 3, 0, 49, 67, 6]);
        self.raw(&[GS, b'(', b'k', 3, 0, 49, 69, 49]);
        self.raw(&[GS, b'(', b'k', pl, ph, 49, 80, 48]);
        self.raw(data.as_bytes());
        self.raw(&[GS, b'(', b'k', 3, 0, 49, 81, 48]).raw(&[LF])
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Prepares text for this printer: plain punctuation, and ₹ as "Rs." when the printer is set to.
fn prepare(text: &str, profile: &PrinterProfile) -> String {
    let text = text::simplify(text);
    match profile.rupee {
        RupeeStyle::Text => text.replace('₹', "Rs."),
        RupeeStyle::Symbol => text,
    }
}

fn picture_font<'a>(
    profile: &PrinterProfile,
    font: Option<&'a TextRaster>,
    text: &str,
) -> Option<&'a TextRaster> {
    let rupee_picture = profile.rupee == RupeeStyle::Symbol && text.contains('₹');
    if !(profile.picture_fallback || rupee_picture) {
        return None;
    }
    font.filter(|f| f.covers(text))
}

fn px(size: Size) -> f32 {
    match size {
        Size::Normal => 22.0,
        Size::Large => 40.0,
    }
}

/// One copy of a document, without the drawer pulse.
fn render_copy(doc: &Doc, profile: &PrinterProfile, font: Option<&TextRaster>) -> EscPos {
    let mut p = EscPos::new(profile.code_page);
    let page = profile.code_page;
    let full = profile.paper.columns();
    let dots = profile.paper.dots();
    for block in &doc.blocks {
        match block {
            Block::Text {
                text: raw,
                place,
                bold,
                size,
            } => {
                let text = prepare(raw, profile);
                let width = if *size == Size::Large { full / 2 } else { full };
                let lines = text::wrap(&text, width);
                if !page.can_print(&text) {
                    if let Some(font) = picture_font(profile, font, &text) {
                        p.align(Placement::Left);
                        for l in &lines {
                            p.image(&font.line(l, None, *place, dots, px(*size)));
                        }
                        continue;
                    }
                }
                p.align(*place).bold(*bold).size(*size);
                for l in &lines {
                    p.line(l);
                }
                p.size(Size::Normal).bold(false);
            }
            Block::Pair {
                left,
                right,
                bold,
                size,
            } => {
                let left = prepare(left, profile);
                let right = prepare(right, profile);
                let width = if *size == Size::Large { full / 2 } else { full };
                let both = format!("{left}{right}");
                if !page.can_print(&both) {
                    if let Some(font) = picture_font(profile, font, &both) {
                        p.align(Placement::Left);
                        let room = width.saturating_sub(text::columns(&right) + 1).max(1);
                        let mut lines = text::wrap(&left, room);
                        let last = lines.pop().unwrap_or_default();
                        for l in &lines {
                            p.image(&font.line(l, None, Placement::Left, dots, px(*size)));
                        }
                        p.image(&font.line(&last, Some(&right), Placement::Left, dots, px(*size)));
                        continue;
                    }
                }
                p.align(Placement::Left).bold(*bold).size(*size);
                for l in text::pair(&left, &right, width) {
                    p.line(&l);
                }
                p.size(Size::Normal).bold(false);
            }
            Block::Rule => {
                p.align(Placement::Left).line(&"-".repeat(full));
            }
            Block::Blank => {
                p.raw(&[LF]);
            }
            Block::Barcode(data) => {
                p.align(Placement::Center)
                    .code128(data)
                    .align(Placement::Left);
            }
            Block::Qr(data) => {
                p.align(Placement::Center).qr(data).align(Placement::Left);
            }
        }
    }
    p.feed(profile.feed_lines).cut(profile.cut);
    p
}

/// The whole job: the drawer pulse first (when asked and the printer has a drawer), then every
/// copy, each cut.
pub fn render(
    doc: &Doc,
    profile: &PrinterProfile,
    font: Option<&TextRaster>,
    kick_drawer: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    if kick_drawer && profile.has_drawer {
        let mut p = EscPos::new(profile.code_page);
        p.drawer(profile.drawer_pin);
        out.extend(p.into_bytes());
    }
    for _ in 0..profile.copies.max(1) {
        out.extend(render_copy(doc, profile, font).into_bytes());
    }
    out
}

/// Just the drawer pulse (the "open drawer" button).
pub fn drawer_only(profile: &PrinterProfile) -> Vec<u8> {
    let mut p = EscPos::new(profile.code_page);
    p.drawer(profile.drawer_pin);
    p.into_bytes()
}
