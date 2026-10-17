//! Financial years and document-number templates. Mirrors packages/shared/src/numbering.ts.

/// "25-26" for a business date in Apr 2025–Mar 2026 with `start_month` 4; "2025" when the FY is
/// the calendar year. Invalid dates fall back to the calendar year of the string's first 4 chars.
pub fn financial_year(business_date: &str, start_month: u32) -> String {
    let year: i32 = business_date
        .get(0..4)
        .and_then(|y| y.parse().ok())
        .unwrap_or(1970);
    let month: u32 = business_date
        .get(5..7)
        .and_then(|m| m.parse().ok())
        .unwrap_or(1);
    if start_month <= 1 {
        return year.to_string();
    }
    let start = if month >= start_month { year } else { year - 1 };
    format!(
        "{:02}-{:02}",
        start.rem_euclid(100),
        (start + 1).rem_euclid(100)
    )
}

/// Fills a series template. Tokens: {prefix} {fy} {branch} {seq}.
pub fn format_doc_number(
    template: &str,
    prefix: &str,
    fy: &str,
    branch: &str,
    seq: i64,
    padding: usize,
) -> String {
    template
        .replace("{prefix}", prefix)
        .replace("{fy}", fy)
        .replace("{branch}", branch)
        .replace("{seq}", &format!("{seq:0padding$}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn financial_years() {
        assert_eq!(financial_year("2025-04-01", 4), "25-26");
        assert_eq!(financial_year("2026-03-31", 4), "25-26");
        assert_eq!(financial_year("2099-12-01", 4), "99-00");
        assert_eq!(financial_year("2025-01-15", 1), "2025");
    }

    #[test]
    fn templates() {
        assert_eq!(
            format_doc_number("{prefix}/{fy}/{seq}", "INV", "25-26", "", 123, 5),
            "INV/25-26/00123"
        );
        assert_eq!(
            format_doc_number("{branch}-{prefix}{seq}", "S", "x", "C2", 7, 0),
            "C2-S7"
        );
    }
}
