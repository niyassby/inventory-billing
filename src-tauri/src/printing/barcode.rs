//! Barcode symbologies for labels and previews: EAN-13, EAN-8, Code 128 and QR. Thermal and
//! label printers draw barcodes themselves from the data; these encoders check the data, choose
//! the symbology and draw the same barcode for the preview and for A4 sheets.

use qrcode::{Color, EcLevel, QrCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum Symbology {
    Ean13,
    Ean8,
    #[default]
    Code128,
    Qr,
}

/// The check digit of an EAN/UPC number (the digits before the check digit).
pub fn ean_check_digit(body: &str) -> Option<u8> {
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = body
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    u8::try_from((10 - sum % 10) % 10).ok()
}

/// A complete EAN-13 (13 digits) or EAN-8 (8 digits) with a correct check digit.
pub fn is_valid_ean(code: &str, len: usize) -> bool {
    code.len() == len
        && code.bytes().all(|b| b.is_ascii_digit())
        && ean_check_digit(&code[..len - 1]) == Some(code.as_bytes()[len - 1] - b'0')
}

const L_CODES: [&str; 10] = [
    "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
    "0110111", "0001011",
];
const PARITY: [&str; 10] = [
    "LLLLLL", "LLGLGG", "LLGGLG", "LLGGGL", "LGLLGG", "LGGLLG", "LGGGLL", "LGLGLG", "LGLGGL",
    "LGGLGL",
];

fn digit_modules(digit: u8, set: char) -> Vec<bool> {
    let l: Vec<bool> = L_CODES[usize::from(digit)]
        .bytes()
        .map(|b| b == b'1')
        .collect();
    match set {
        'L' => l,
        // R is L inverted; G is R reversed.
        'R' => l.iter().map(|m| !m).collect(),
        _ => l.iter().rev().map(|m| !m).collect(),
    }
}

fn guard(out: &mut Vec<bool>, pattern: &str) {
    out.extend(pattern.bytes().map(|b| b == b'1'));
}

/// The 95 modules (true = bar) of an EAN-13.
pub fn ean13_modules(code: &str) -> AppResult<Vec<bool>> {
    if !is_valid_ean(code, 13) {
        return Err(AppError::validation(
            "An EAN-13 barcode has 13 digits with a correct check digit.",
        ));
    }
    let d: Vec<u8> = code.bytes().map(|b| b - b'0').collect();
    let parity = PARITY[usize::from(d[0])];
    let mut out = Vec::with_capacity(95);
    guard(&mut out, "101");
    for (digit, set) in d[1..7].iter().zip(parity.chars()) {
        out.extend(digit_modules(*digit, set));
    }
    guard(&mut out, "01010");
    for digit in &d[7..13] {
        out.extend(digit_modules(*digit, 'R'));
    }
    guard(&mut out, "101");
    Ok(out)
}

/// The 67 modules of an EAN-8.
pub fn ean8_modules(code: &str) -> AppResult<Vec<bool>> {
    if !is_valid_ean(code, 8) {
        return Err(AppError::validation(
            "An EAN-8 barcode has 8 digits with a correct check digit.",
        ));
    }
    let d: Vec<u8> = code.bytes().map(|b| b - b'0').collect();
    let mut out = Vec::with_capacity(67);
    guard(&mut out, "101");
    for digit in &d[0..4] {
        out.extend(digit_modules(*digit, 'L'));
    }
    guard(&mut out, "01010");
    for digit in &d[4..8] {
        out.extend(digit_modules(*digit, 'R'));
    }
    guard(&mut out, "101");
    Ok(out)
}

/// Bar and space widths of the 107 Code 128 symbols (103–105 are the start codes).
const CODE128: [&str; 106] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212",
    "221213", "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221",
    "223211", "221132", "221231", "213212", "223112", "312131", "311222", "321122", "321221",
    "312212", "322112", "322211", "212123", "212321", "232121", "111323", "131123", "131321",
    "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131",
    "113123", "113321", "133121", "313121", "211331", "231131", "213113", "213311", "213131",
    "311123", "311321", "331121", "312113", "312311", "332111", "314111", "221411", "431111",
    "111224", "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114",
    "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242",
    "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311",
    "113141", "114131", "311141", "411131", "211412", "211214", "211232",
];
const CODE128_STOP: &str = "2331112";
const START_B: usize = 104;
const START_C: usize = 105;

/// The symbol values of a Code 128 barcode: start, data, checksum (no stop). All-digit data of
/// even length uses code set C (two digits a symbol), anything else code set B (ASCII 32–126).
pub fn code128_values(data: &str) -> AppResult<Vec<usize>> {
    if data.is_empty() || data.len() > 48 {
        return Err(AppError::validation(
            "A Code 128 barcode holds 1 to 48 characters.",
        ));
    }
    let mut values = Vec::with_capacity(data.len() + 2);
    if data.len().is_multiple_of(2) && data.len() >= 4 && data.bytes().all(|b| b.is_ascii_digit()) {
        values.push(START_C);
        for pair in data.as_bytes().chunks(2) {
            values.push(usize::from(pair[0] - b'0') * 10 + usize::from(pair[1] - b'0'));
        }
    } else {
        values.push(START_B);
        for b in data.bytes() {
            if !(32..=126).contains(&b) {
                return Err(AppError::validation(
                    "A Code 128 barcode can only hold letters, digits and plain punctuation.",
                ));
            }
            values.push(usize::from(b - 32));
        }
    }
    let checksum = values
        .iter()
        .enumerate()
        .map(|(i, v)| if i == 0 { *v } else { v * i })
        .sum::<usize>()
        % 103;
    values.push(checksum);
    Ok(values)
}

fn widths_to_modules(out: &mut Vec<bool>, widths: &str) {
    for (i, w) in widths.bytes().enumerate() {
        out.extend(std::iter::repeat_n(i % 2 == 0, usize::from(w - b'0')));
    }
}

pub fn code128_modules(data: &str) -> AppResult<Vec<bool>> {
    let values = code128_values(data)?;
    let mut out = Vec::with_capacity(11 * values.len() + 13);
    for v in values {
        widths_to_modules(&mut out, CODE128[v]);
    }
    widths_to_modules(&mut out, CODE128_STOP);
    Ok(out)
}

/// The dark modules of a QR code (medium error correction), row by row, and its width.
pub fn qr_matrix(data: &str) -> AppResult<(usize, Vec<bool>)> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M)
        .map_err(|_| AppError::validation("That is too much text for a QR code."))?;
    let width = code.width();
    let dark = code
        .to_colors()
        .into_iter()
        .map(|c| c == Color::Dark)
        .collect();
    Ok((width, dark))
}

/// What a label's barcode will actually be: the wanted symbology when the data fits it, else
/// Code 128 (EAN needs a valid EAN number). `None` when there is nothing to encode.
pub fn choose(wanted: Symbology, data: &str) -> Option<Symbology> {
    let data = data.trim();
    if data.is_empty() {
        return None;
    }
    let fits = match wanted {
        Symbology::Ean13 => is_valid_ean(data, 13),
        Symbology::Ean8 => is_valid_ean(data, 8),
        Symbology::Code128 => code128_values(data).is_ok(),
        Symbology::Qr => qr_matrix(data).is_ok(),
    };
    if fits {
        Some(wanted)
    } else if code128_values(data).is_ok() {
        Some(Symbology::Code128)
    } else {
        None
    }
}

/// Linear barcode modules for a symbology (not QR).
pub fn linear_modules(symbology: Symbology, data: &str) -> AppResult<Vec<bool>> {
    match symbology {
        Symbology::Ean13 => ean13_modules(data),
        Symbology::Ean8 => ean8_modules(data),
        Symbology::Code128 => code128_modules(data),
        Symbology::Qr => Err(AppError::validation("QR is not a linear barcode.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(modules: &[bool]) -> String {
        modules.iter().map(|m| if *m { '1' } else { '0' }).collect()
    }

    #[test]
    fn ean13_check_digits_and_modules() {
        assert!(is_valid_ean("4006381333931", 13));
        assert!(is_valid_ean("5901234123457", 13));
        assert!(!is_valid_ean("5901234123458", 13));
        assert_eq!(ean_check_digit("400638133393"), Some(1));
        let m = ean13_modules("5901234123457").unwrap();
        assert_eq!(m.len(), 95);
        let s = bits(&m);
        assert!(s.starts_with("101"));
        assert_eq!(&s[45..50], "01010");
        assert!(s.ends_with("101"));
        // First digit 5 → parity LGGLLG; second digit 9 in set L is 0001011.
        assert_eq!(&s[3..10], "0001011");
        // Third digit 0 in set G (reversed R of 0001101 → R 1110010 → G 0100111).
        assert_eq!(&s[10..17], "0100111");
        // Last digit 7 in set R: inverted L 0111011 → 1000100.
        assert_eq!(&s[85..92], "1000100");
    }

    #[test]
    fn ean8_check_digits_and_modules() {
        assert!(is_valid_ean("96385074", 8));
        assert!(!is_valid_ean("96385075", 8));
        let m = ean8_modules("96385074").unwrap();
        assert_eq!(m.len(), 67);
        assert!(ean8_modules("1234").is_err());
    }

    #[test]
    fn code128_table_and_encoding() {
        for (i, w) in CODE128.iter().enumerate() {
            let sum: u32 = w.bytes().map(|b| u32::from(b - b'0')).sum();
            assert_eq!(sum, 11, "symbol {i}");
        }
        // "AB": start B (104), A = 33, B = 34; checksum (104 + 33·1 + 34·2) % 103 = 102.
        assert_eq!(code128_values("AB").unwrap(), vec![104, 33, 34, 102]);
        // All digits, even length → code set C: "1234" → 12, 34.
        assert_eq!(
            code128_values("1234").unwrap(),
            vec![105, 12, 34, (105 + 12 + 34 * 2) % 103]
        );
        let m = code128_modules("AB").unwrap();
        assert_eq!(m.len(), 11 * 4 + 13);
        assert!(m[0] && m[m.len() - 1], "starts and ends with a bar");
        assert!(code128_modules("₹").is_err());
    }

    #[test]
    fn qr_matrix_has_a_valid_size() {
        let (width, dark) = qr_matrix("INV/26-27/00001").unwrap();
        assert!(width >= 21 && (width - 17) % 4 == 0, "{width}");
        assert_eq!(dark.len(), width * width);
        // Finder pattern: the top-left corner module is dark.
        assert!(dark[0]);
    }

    #[test]
    fn falls_back_to_code128_when_the_data_is_not_an_ean() {
        assert_eq!(
            choose(Symbology::Ean13, "5901234123457"),
            Some(Symbology::Ean13)
        );
        assert_eq!(
            choose(Symbology::Ean13, "SKU-001"),
            Some(Symbology::Code128)
        );
        assert_eq!(choose(Symbology::Ean8, "96385074"), Some(Symbology::Ean8));
        assert_eq!(choose(Symbology::Qr, "SKU-001"), Some(Symbology::Qr));
        assert_eq!(choose(Symbology::Code128, "  "), None);
    }
}
