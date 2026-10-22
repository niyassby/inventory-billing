//! Text for printers: the code pages thermal printers understand, and the money, quantity, rate
//! and date formats of the screens (packages/shared `formatMoney`, `formatQty`, `showRate`), so a
//! receipt shows exactly what the till showed.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Character tables an ESC/POS printer can be switched to (`ESC t n`). Everything outside the
/// table is printed as a picture or replaced (see `encode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CodePage {
    /// US (the factory default of most printers).
    #[default]
    Pc437,
    /// Multilingual Latin with €.
    Pc858,
    /// Windows Latin-1.
    Wpc1252,
}

const PC437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
const PC858_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜø£Ø×ƒáíóúñÑªº¿®¬½¼¡«»░▒▓│┤ÁÂÀ©╣║╗╝¢¥┐└┴┬├─┼ãÃ╚╔╩╦╠═╬¤ðÐÊËÈ€ÍÎÏ┘┌█▄¦Ì▀ÓßÔÒõÕµþÞÚÛÙýÝ¯´\u{ad}±‗¾¶§÷¸°¨·¹³²■\u{a0}";
const WPC1252_HIGH: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8d}Ž\u{8f}\u{90}‘’“”•–—˜™š›œ\u{9d}žŸ\u{a0}¡¢£¤¥¦§¨©ª«¬\u{ad}®¯°±²³´µ¶·¸¹º»¼½¾¿ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿ";

impl CodePage {
    /// The `n` of `ESC t n` (Epson numbering, which most ESC/POS clones follow).
    pub fn escpos_number(self) -> u8 {
        match self {
            CodePage::Pc437 => 0,
            CodePage::Pc858 => 19,
            CodePage::Wpc1252 => 16,
        }
    }

    fn high_half(self) -> &'static str {
        match self {
            CodePage::Pc437 => PC437_HIGH,
            CodePage::Pc858 => PC858_HIGH,
            CodePage::Wpc1252 => WPC1252_HIGH,
        }
    }

    /// The byte for a character, if the table has it.
    pub fn byte_of(self, c: char) -> Option<u8> {
        if (' '..='~').contains(&c) {
            return u8::try_from(c as u32).ok();
        }
        self.high_half()
            .chars()
            .position(|h| h == c)
            .and_then(|i| u8::try_from(0x80 + i).ok())
    }

    pub fn can_print(self, text: &str) -> bool {
        text.chars().all(|c| self.byte_of(c).is_some())
    }
}

/// Plain substitutes for punctuation that most code pages lack. Applied before deciding whether a
/// line needs a picture, so a curly quote never turns a whole line into one.
pub fn simplify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{2032}' => out.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{2033}' => out.push('"'),
            '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            '\u{00d7}' => out.push('x'),
            '\u{2022}' | '\u{00b7}' => out.push('-'),
            '\t' => out.push(' '),
            '\r' | '\n' => out.push(' '),
            '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Bytes for text in a code page; characters the table lacks become `?`.
pub fn encode(page: CodePage, text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| page.byte_of(c).unwrap_or(b'?'))
        .collect()
}

/// Width of text in printer columns (one per character in the fixed-width printer font).
pub fn columns(text: &str) -> usize {
    text.chars().count()
}

/// Splits text into lines of at most `width` columns, breaking at spaces where it can.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    // A small indent ("  2 pcs x 118.00") is kept on every line.
    let body = text.trim_start_matches(' ');
    let indent = text.len() - body.len();
    if indent > 0 && indent < width / 2 {
        let pad = " ".repeat(indent);
        return wrap(body, width - indent)
            .into_iter()
            .map(|l| format!("{pad}{l}"))
            .collect();
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let mut word: Vec<char> = word.chars().collect();
        loop {
            let used = columns(&line);
            let gap = usize::from(used > 0);
            if used + gap + word.len() <= width {
                if gap == 1 {
                    line.push(' ');
                }
                line.extend(word.iter());
                break;
            }
            if used > 0 {
                lines.push(std::mem::take(&mut line));
                continue;
            }
            // A word longer than the line: cut it.
            let rest = word.split_off(width);
            lines.push(word.iter().collect());
            word = rest;
            if word.is_empty() {
                break;
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Left and right text on one line of `width` columns; the left side wraps when both don't fit.
pub fn pair(left: &str, right: &str, width: usize) -> Vec<String> {
    let right_cols = columns(right);
    if right_cols >= width {
        let mut lines = wrap(left, width);
        lines.push(format!("{right:>width$}"));
        return lines;
    }
    let room = width - right_cols - 1;
    let mut lines = wrap(left, room.max(1));
    let last = lines.pop().unwrap_or_default();
    let pad = width - right_cols - columns(&last);
    lines.push(format!("{last}{}{right}", " ".repeat(pad)));
    lines
}

/// Indian digit grouping (12,34,567.00) for `-IN` locales, groups of three otherwise.
fn group(digits: &str, indian: bool) -> String {
    let len = digits.len();
    if len <= 3 {
        return digits.to_string();
    }
    let (head, tail) = digits.split_at(len - 3);
    let size = if indian { 2 } else { 3 };
    let mut groups: Vec<&str> = Vec::new();
    let mut end = head.len();
    while end > 0 {
        let start = end.saturating_sub(size);
        groups.push(&head[start..end]);
        end = start;
    }
    groups.reverse();
    format!("{},{tail}", groups.join(","))
}

/// An amount in minor units as the screens show it, without the currency symbol (receipts put the
/// symbol only on the total line).
pub fn money(minor: i64, digits: u32, locale: &str) -> String {
    let indian = locale.ends_with("-IN");
    let negative = minor < 0;
    let abs = minor.unsigned_abs();
    let scale = 10u64.pow(digits);
    let whole = group(&(abs / scale).to_string(), indian);
    let text = if digits == 0 {
        whole
    } else {
        format!(
            "{whole}.{:0width$}",
            abs % scale,
            width = usize::try_from(digits).unwrap_or(2)
        )
    };
    if negative {
        format!("-{text}")
    } else {
        text
    }
}

/// Thousandths of a unit: 1500 → "1.5", 2000 → "2" (as `formatQty`).
pub fn qty(milli: i64) -> String {
    let negative = milli < 0;
    let abs = milli.unsigned_abs();
    let whole = abs / 1000;
    let fraction = format!("{:03}", abs % 1000);
    let fraction = fraction.trim_end_matches('0');
    let sign = if negative { "-" } else { "" };
    if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

/// Parts per million as a percentage: 180000 → "18%", 25000 → "2.5%" (as `showRate`).
pub fn rate(ppm: i64) -> String {
    let whole = ppm / 10_000;
    let rest = (ppm % 10_000).abs();
    if rest == 0 {
        return format!("{whole}%");
    }
    let fraction = format!("{rest:04}");
    let fraction = fraction.get(..2).unwrap_or("00").trim_end_matches('0');
    if fraction.is_empty() {
        format!("{whole}%")
    } else {
        format!("{whole}.{fraction}%")
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// "2026-10-22" → "22 Oct 2026" (the en-IN form the screens use).
pub fn date(iso: &str) -> String {
    let parts: Vec<&str> = iso.get(..10).unwrap_or(iso).split('-').collect();
    match parts.as_slice() {
        [y, m, d] => {
            let month = m
                .parse::<usize>()
                .ok()
                .and_then(|m| MONTHS.get(m.wrapping_sub(1)));
            match (month, d.parse::<u32>()) {
                (Some(month), Ok(day)) => format!("{day} {month} {y}"),
                _ => iso.to_string(),
            }
        }
        _ => iso.to_string(),
    }
}

/// "bank_transfer" → "Bank transfer" (as `label` in src/lib/format.ts).
pub fn label(code: &str) -> String {
    let text = code.replace('_', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_page_table_has_128_characters() {
        for page in [CodePage::Pc437, CodePage::Pc858, CodePage::Wpc1252] {
            assert_eq!(page.high_half().chars().count(), 128, "{page:?}");
        }
        assert_eq!(CodePage::Pc858.byte_of('€'), Some(0xd5));
        assert_eq!(CodePage::Wpc1252.byte_of('€'), Some(0x80));
        assert_eq!(CodePage::Pc437.byte_of('é'), Some(0x82));
        assert_eq!(CodePage::Pc437.byte_of('₹'), None);
    }

    #[test]
    fn encodes_and_replaces() {
        assert_eq!(encode(CodePage::Pc437, "Café ₹"), b"Caf\x82 ?".to_vec());
        assert_eq!(simplify("“Tea” – 2×"), "\"Tea\" - 2x");
    }

    #[test]
    fn formats_like_the_screens() {
        assert_eq!(money(123_456_789, 2, "en-IN"), "12,34,567.89");
        assert_eq!(money(123_456_789, 2, "en-US"), "1,234,567.89");
        assert_eq!(money(-5, 2, "en-IN"), "-0.05");
        assert_eq!(money(100_000, 2, "en-IN"), "1,000.00");
        assert_eq!(money(1234, 0, "en-IN"), "1,234");
        assert_eq!(qty(1500), "1.5");
        assert_eq!(qty(2000), "2");
        assert_eq!(qty(-250), "-0.25");
        assert_eq!(rate(180_000), "18%");
        assert_eq!(rate(25_000), "2.5%");
        assert_eq!(rate(2_500), "0.25%");
        assert_eq!(date("2026-10-22"), "22 Oct 2026");
        assert_eq!(label("bank_transfer"), "Bank transfer");
    }

    #[test]
    fn wraps_and_pairs_within_the_width() {
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(
            wrap("  2 x 118.00 long", 10),
            vec!["  2 x", "  118.00", "  long"]
        );
        assert_eq!(pair("Total", "10.00", 16), vec!["Total      10.00"]);
        let long = pair("A very long item name", "1,000.00", 20);
        assert!(long.iter().all(|l| columns(l) <= 20), "{long:?}");
        assert!(long.last().unwrap().ends_with("1,000.00"));
    }
}
