//! Reading bank statement values exactly: amounts as Indian banks print them ("1,23,456.78",
//! "(500.00)", "250.00 Dr", "₹ 99") into minor units with no floating point, and dates in the
//! common formats.

use chrono::NaiveDate;

/// Date layouts a statement may use (the column mapping names one).
pub const DATE_FORMATS: [&str; 7] = [
    "DD/MM/YYYY",
    "DD-MM-YYYY",
    "DD.MM.YYYY",
    "YYYY-MM-DD",
    "MM/DD/YYYY",
    "DD/MM/YY",
    "DD-MMM-YYYY",
];

/// A statement date as "YYYY-MM-DD", or None when it doesn't fit the layout.
pub fn parse_date(text: &str, format: &str) -> Option<String> {
    let t = text.trim();
    // Some banks add a time ("01/10/2026 10:15:00"): the date part is enough.
    let t = t.split_whitespace().next().unwrap_or_default();
    let pattern = match format {
        "DD/MM/YYYY" => "%d/%m/%Y",
        "DD-MM-YYYY" => "%d-%m-%Y",
        "DD.MM.YYYY" => "%d.%m.%Y",
        "YYYY-MM-DD" => "%Y-%m-%d",
        "MM/DD/YYYY" => "%m/%d/%Y",
        "DD/MM/YY" => "%d/%m/%y",
        "DD-MMM-YYYY" => "%d-%b-%Y",
        _ => return None,
    };
    NaiveDate::parse_from_str(t, pattern)
        .ok()
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Text that is not an amount (or has more decimals than the currency allows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAnAmount;

/// An amount in minor units. Empty text is None (a blank debit or credit column); text that is
/// not an amount is Err.
pub fn parse_amount(text: &str, minor_digits: u32) -> Result<Option<i64>, NotAnAmount> {
    let mut t = text.trim().to_string();
    if t.is_empty() || t == "-" {
        return Ok(None);
    }
    let mut negative = false;
    if t.starts_with('(') && t.ends_with(')') {
        negative = true;
        t = t[1..t.len() - 1].to_string();
    }
    let upper = t.to_ascii_uppercase();
    for (suffix, neg) in [("DR", true), ("CR", false)] {
        if let Some(stripped) = upper.trim_end().strip_suffix(suffix) {
            negative ^= neg;
            t = stripped.trim_end().trim_end_matches('.').to_string();
            break;
        }
    }
    let mut t = t.trim().to_string();
    for prefix in ["INR", "RS.", "RS", "₹"] {
        if t.to_ascii_uppercase().starts_with(prefix) {
            t = t[prefix.len()..].to_string();
            break;
        }
    }
    let t: String = t
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '\u{a0}'))
        .collect();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(&t)),
    };
    negative ^= sign;
    let (whole, fraction) = match digits.split_once('.') {
        Some((w, f)) => (w, f),
        None => (digits, ""),
    };
    if whole.is_empty() && fraction.is_empty() {
        return Err(NotAnAmount);
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(NotAnAmount);
    }
    let width = minor_digits as usize;
    let fraction = fraction.trim_end_matches('0');
    if fraction.len() > width {
        return Err(NotAnAmount);
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| NotAnAmount)?
    };
    let minor: i64 = if width == 0 {
        0
    } else {
        format!("{fraction:0<width$}")
            .parse()
            .map_err(|_| NotAnAmount)?
    };
    let scale = 10i64.checked_pow(minor_digits).ok_or(NotAnAmount)?;
    let value = whole
        .checked_mul(scale)
        .and_then(|v| v.checked_add(minor))
        .ok_or(NotAnAmount)?;
    Ok(Some(if negative { -value } else { value }))
}

#[cfg(test)]
mod tests {
    use super::{parse_amount, parse_date, NotAnAmount};

    #[test]
    fn reads_indian_bank_amounts_exactly() {
        assert_eq!(parse_amount("1,23,456.78", 2), Ok(Some(12_345_678)));
        assert_eq!(parse_amount("(500.00)", 2), Ok(Some(-50_000)));
        assert_eq!(parse_amount("250.00 Dr", 2), Ok(Some(-25_000)));
        assert_eq!(parse_amount("250.00 CR", 2), Ok(Some(25_000)));
        assert_eq!(parse_amount("₹ 99", 2), Ok(Some(9_900)));
        assert_eq!(parse_amount("-0.5", 2), Ok(Some(-50)));
        assert_eq!(parse_amount(".75", 2), Ok(Some(75)));
        assert_eq!(parse_amount("10.10", 2), Ok(Some(1_010)));
        assert_eq!(parse_amount("", 2), Ok(None));
        assert_eq!(parse_amount(" - ", 2), Ok(None));
        assert_eq!(parse_amount("1.005", 2), Err(NotAnAmount));
        assert_eq!(parse_amount("Rs. 1,000", 2), Ok(Some(100_000)));
        assert_eq!(parse_amount("INR 5", 2), Ok(Some(500)));
        assert_eq!(parse_amount("12abc", 2), Err(NotAnAmount));
        assert_eq!(parse_amount("1.2.3", 2), Err(NotAnAmount));
        assert_eq!(parse_amount("99999999999999999999", 2), Err(NotAnAmount));
    }

    #[test]
    fn reads_the_usual_date_layouts() {
        assert_eq!(
            parse_date("05/10/2026", "DD/MM/YYYY").as_deref(),
            Some("2026-10-05")
        );
        assert_eq!(
            parse_date("05-Oct-2026", "DD-MMM-YYYY").as_deref(),
            Some("2026-10-05")
        );
        assert_eq!(
            parse_date("10/05/2026", "MM/DD/YYYY").as_deref(),
            Some("2026-10-05")
        );
        assert_eq!(
            parse_date("05/10/26", "DD/MM/YY").as_deref(),
            Some("2026-10-05")
        );
        assert_eq!(
            parse_date("2026-10-05 10:15", "YYYY-MM-DD").as_deref(),
            Some("2026-10-05")
        );
        assert_eq!(parse_date("31/02/2026", "DD/MM/YYYY"), None);
        assert_eq!(parse_date("05/10/2026", "nope"), None);
    }
}
