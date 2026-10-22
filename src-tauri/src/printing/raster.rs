//! Text as a picture, for characters a thermal printer's code page lacks (₹, Hindi, Malayalam…).
//! The line is shaped with rustybuzz (so Indian scripts join correctly) and drawn with fontdue in
//! a font that has those scripts: Nirmala UI on Windows, or a font the printer settings name.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One-bit picture, rows packed 8 dots a byte with the left-most dot in the high bit (the layout
/// ESC/POS `GS v 0` expects).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Bitmap {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width.div_ceil(8) * 8;
        Self {
            width,
            height,
            data: vec![0; width / 8 * height],
        }
    }

    pub fn bytes_per_row(&self) -> usize {
        self.width / 8
    }

    pub fn set(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            let row = self.bytes_per_row();
            self.data[y * row + x / 8] |= 0x80 >> (x % 8);
        }
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.width
            && y < self.height
            && self.data[y * self.bytes_per_row() + x / 8] & (0x80 >> (x % 8)) != 0
    }

    pub fn is_blank(&self) -> bool {
        self.data.iter().all(|b| *b == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Left,
    Center,
    Right,
}

pub struct TextRaster {
    data: Vec<u8>,
    index: u32,
    font: fontdue::Font,
}

struct PlacedGlyph {
    id: u16,
    x: f32,
    y: f32,
}

/// Fonts tried in order when the printer settings name none.
fn candidates() -> Vec<(PathBuf, u32)> {
    let windows = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
        .join("Fonts");
    vec![
        (windows.join("Nirmala.ttc"), 0),
        (windows.join("Nirmala.ttf"), 0),
        (windows.join("segoeui.ttf"), 0),
        (
            PathBuf::from("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"),
            0,
        ),
        (
            PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
            0,
        ),
    ]
}

impl TextRaster {
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Option<Self> {
        rustybuzz::Face::from_slice(&data, index)?;
        let font = fontdue::Font::from_bytes(
            data.as_slice(),
            fontdue::FontSettings {
                collection_index: index,
                ..fontdue::FontSettings::default()
            },
        )
        .ok()?;
        Some(Self { data, index, font })
    }

    pub fn from_file(path: &Path, index: u32) -> Option<Self> {
        Self::from_bytes(std::fs::read(path).ok()?, index)
    }

    /// The system font, loaded once (None when no suitable font is installed).
    pub fn system() -> Option<&'static TextRaster> {
        static FONT: OnceLock<Option<TextRaster>> = OnceLock::new();
        FONT.get_or_init(|| {
            candidates()
                .into_iter()
                .find_map(|(path, index)| Self::from_file(&path, index))
        })
        .as_ref()
    }

    /// Whether the font has a glyph for every character (else the picture would show boxes).
    pub fn covers(&self, text: &str) -> bool {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .all(|c| self.font.lookup_glyph_index(c) != 0)
    }

    fn shape(&self, text: &str, px: f32) -> (Vec<PlacedGlyph>, f32) {
        let Some(face) = rustybuzz::Face::from_slice(&self.data, self.index) else {
            return (Vec::new(), 0.0);
        };
        let scale = px / face.units_per_em() as f32;
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let mut pen = 0.0f32;
        let mut glyphs = Vec::with_capacity(shaped.len());
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            glyphs.push(PlacedGlyph {
                id: u16::try_from(info.glyph_id).unwrap_or(0),
                x: pen + pos.x_offset as f32 * scale,
                y: pos.y_offset as f32 * scale,
            });
            pen += pos.x_advance as f32 * scale;
        }
        (glyphs, pen)
    }

    /// Ascent and line height in dots at this size.
    fn metrics(&self, px: f32) -> (f32, usize) {
        let Some(face) = rustybuzz::Face::from_slice(&self.data, self.index) else {
            return (px, px as usize);
        };
        let scale = px / face.units_per_em() as f32;
        let ascent = f32::from(face.ascender()) * scale;
        let descent = -f32::from(face.descender()) * scale;
        (ascent, (ascent + descent).ceil() as usize + 2)
    }

    fn draw(
        &self,
        bitmap: &mut Bitmap,
        glyphs: &[PlacedGlyph],
        start: f32,
        baseline: f32,
        px: f32,
    ) {
        for g in glyphs {
            let (m, coverage) = self.font.rasterize_indexed(g.id, px);
            let left = (start + g.x).round() as i64 + i64::from(m.xmin);
            let top = (baseline - g.y).round() as i64 - i64::from(m.ymin) - m.height as i64;
            for row in 0..m.height {
                for col in 0..m.width {
                    if coverage[row * m.width + col] >= 112 {
                        let (x, y) = (left + col as i64, top + row as i64);
                        if x >= 0 && y >= 0 {
                            bitmap.set(x as usize, y as usize);
                        }
                    }
                }
            }
        }
    }

    /// One printed line `width` dots wide: `left` placed as asked and, when given, `right` at the
    /// right edge (the receipt's "name … amount" lines).
    pub fn line(
        &self,
        left: &str,
        right: Option<&str>,
        placement: Placement,
        width: usize,
        px: f32,
    ) -> Bitmap {
        let (ascent, height) = self.metrics(px);
        let mut bitmap = Bitmap::new(width, height);
        let baseline = ascent + 1.0;
        let (glyphs, advance) = self.shape(left, px);
        let width_f = bitmap.width as f32;
        let start = match placement {
            Placement::Left => 0.0,
            Placement::Center => ((width_f - advance) / 2.0).max(0.0),
            Placement::Right => (width_f - advance).max(0.0),
        };
        self.draw(&mut bitmap, &glyphs, start, baseline, px);
        if let Some(right) = right.filter(|r| !r.is_empty()) {
            let (glyphs, advance) = self.shape(right, px);
            self.draw(
                &mut bitmap,
                &glyphs,
                (width_f - advance).max(0.0),
                baseline,
                px,
            );
        }
        bitmap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmaps_pack_eight_dots_a_byte() {
        let mut b = Bitmap::new(10, 2);
        assert_eq!(b.width, 16);
        b.set(0, 0);
        b.set(9, 1);
        assert_eq!(b.data, vec![0x80, 0, 0, 0x40]);
        assert!(b.get(9, 1) && !b.get(8, 1));
    }

    #[test]
    fn draws_the_rupee_sign_and_indian_scripts_when_a_font_is_installed() {
        let Some(font) = TextRaster::system() else {
            eprintln!("no system font with Indian scripts here; skipped");
            return;
        };
        if !font.covers("₹ नमस्ते") {
            eprintln!("the system font lacks these glyphs; skipped");
            return;
        }
        let line = font.line("नमस्ते", Some("₹1,180.00"), Placement::Left, 576, 22.0);
        assert_eq!(line.width, 576);
        assert!(line.height >= 22);
        assert!(!line.is_blank());
        // The amount sits at the right edge, the text at the left; the middle is empty.
        let column_has_ink = |x: usize| (0..line.height).any(|y| line.get(x, y));
        assert!((0..120).any(column_has_ink));
        assert!((440..576).any(column_has_ink));
        assert!(!(250..330).any(column_has_ink));
    }
}
