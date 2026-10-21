//! Shared request/response shapes and input validation helpers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// Which column to sort by: one of the keys the list documents (unknown keys are refused).
    pub sort: Option<String>,
    /// Sort the other way round.
    #[serde(default)]
    pub descending: bool,
}

/// Sortable columns of one list: the key the client sends and the SQL expression it means. The
/// client's text never reaches the SQL, only the expression found here.
pub type SortKeys<'a> = &'a [(&'a str, &'a str)];

impl PageQuery {
    pub const MAX_PAGE_SIZE: u32 = 200;

    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn page_size(&self) -> u32 {
        self.page_size.unwrap_or(50).clamp(1, Self::MAX_PAGE_SIZE)
    }

    pub fn limit_offset(&self) -> (i64, i64) {
        let size = i64::from(self.page_size());
        (size, (i64::from(self.page()) - 1) * size)
    }

    /// The ORDER BY body for this request. No `sort` gives `default` (written by the list, so it
    /// is trusted); a `sort` must be in `keys`. `tiebreak` (a unique column) keeps pages stable.
    pub fn order_by(&self, keys: SortKeys<'_>, default: &str, tiebreak: &str) -> AppResult<String> {
        let Some(requested) = self
            .sort
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return Ok(default.to_string());
        };
        let Some((_, expr)) = keys.iter().find(|(key, _)| *key == requested) else {
            return Err(AppError::field("sort", "You can't sort by that column."));
        };
        let direction = if self.descending { "DESC" } else { "ASC" };
        Ok(format!("{expr} {direction}, {tiebreak}"))
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, total: i64, query: &PageQuery) -> Self {
        Self {
            items,
            total,
            page: query.page(),
            page_size: query.page_size(),
        }
    }
}

/// Empty input for commands that take none.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct NoInput {}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdResult {
    pub id: String,
}

/// Trimmed, required text with a length limit.
pub fn required(field: &str, value: &str, max: usize) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::field(field, "This field is required."));
    }
    if trimmed.chars().count() > max {
        return Err(AppError::field(
            field,
            format!("Use at most {max} characters."),
        ));
    }
    Ok(trimmed.to_string())
}

/// Optional text: blank becomes None.
pub fn optional(field: &str, value: Option<&str>, max: usize) -> AppResult<Option<String>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) if v.chars().count() > max => Err(AppError::field(
            field,
            format!("Use at most {max} characters."),
        )),
        Some(v) => Ok(Some(v.to_string())),
    }
}

pub fn optional_email(field: &str, value: Option<&str>) -> AppResult<Option<String>> {
    let Some(email) = optional(field, value, 254)? else {
        return Ok(None);
    };
    let valid = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !email.contains(' ')
    });
    if valid {
        Ok(Some(email.to_lowercase()))
    } else {
        Err(AppError::field(field, "Enter a valid email address."))
    }
}

pub fn optional_phone(field: &str, value: Option<&str>) -> AppResult<Option<String>> {
    let Some(phone) = optional(field, value, 32)? else {
        return Ok(None);
    };
    let normalized: String = phone
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '(' | ')'))
        .collect();
    let digits = normalized.trim_start_matches('+');
    if (6..=15).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit()) {
        Ok(Some(normalized))
    } else {
        Err(AppError::field(field, "Enter a valid phone number."))
    }
}

const GSTIN_CHARSET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// GSTIN format + checksum (same algorithm as packages/shared/src/gstin.ts).
pub fn is_valid_gstin(value: &str) -> bool {
    let gstin = value.trim().to_uppercase();
    let bytes = gstin.as_bytes();
    if bytes.len() != 15 {
        return false;
    }
    let pattern_ok = bytes[0..2].iter().all(u8::is_ascii_digit)
        && bytes[2..7].iter().all(u8::is_ascii_uppercase)
        && bytes[7..11].iter().all(u8::is_ascii_digit)
        && bytes[11].is_ascii_uppercase()
        && (bytes[12].is_ascii_uppercase() || (b'1'..=b'9').contains(&bytes[12]))
        && bytes[13] == b'Z'
        && bytes[14].is_ascii_alphanumeric();
    if !pattern_ok {
        return false;
    }
    let mut total = 0usize;
    for (i, b) in bytes[..14].iter().enumerate() {
        let Some(value) = GSTIN_CHARSET.iter().position(|c| c == b) else {
            return false;
        };
        let product = value * if i % 2 == 0 { 1 } else { 2 };
        total += product / 36 + product % 36;
    }
    GSTIN_CHARSET[(36 - total % 36) % 36] == bytes[14]
}

pub fn optional_gstin(field: &str, value: Option<&str>) -> AppResult<Option<String>> {
    let Some(gstin) = optional(field, value, 15)? else {
        return Ok(None);
    };
    let upper = gstin.to_uppercase();
    if is_valid_gstin(&upper) {
        Ok(Some(upper))
    } else {
        Err(AppError::field(field, "Enter a valid 15-character GSTIN."))
    }
}

pub fn non_negative(field: &str, value: i64) -> AppResult<i64> {
    if value < 0 {
        return Err(AppError::field(field, "Must not be negative."));
    }
    Ok(value)
}

pub fn positive(field: &str, value: i64) -> AppResult<i64> {
    if value <= 0 {
        return Err(AppError::field(field, "Must be greater than zero."));
    }
    Ok(value)
}

pub fn rate(field: &str, value: i64) -> AppResult<i64> {
    if !(0..=1_000_000).contains(&value) {
        return Err(AppError::field(field, "Rate must be between 0% and 100%."));
    }
    Ok(value)
}

pub fn date(field: &str, value: &str) -> AppResult<String> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string())
        .map_err(|_| AppError::field(field, "Use a YYYY-MM-DD date."))
}

/// Escapes LIKE wildcards in user search text.
pub fn like_pattern(search: &str) -> String {
    let escaped = search
        .trim()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_contact_fields() {
        assert_eq!(
            optional_email("e", Some(" Owner@Shop.IN ")).unwrap(),
            Some("owner@shop.in".into())
        );
        assert!(optional_email("e", Some("nope")).is_err());
        assert_eq!(optional_email("e", Some("  ")).unwrap(), None);
        assert_eq!(
            optional_phone("p", Some("+91 98765-43210")).unwrap(),
            Some("+919876543210".into())
        );
        assert!(optional_phone("p", Some("12")).is_err());
        assert!(is_valid_gstin("27AAPFU0939F1ZV"));
        assert!(is_valid_gstin("29aagcb7383j1z4"));
        assert!(!is_valid_gstin("27AAPFU0939F1ZW"));
        assert_eq!(
            optional_gstin("g", Some("27aapfu0939f1zv")).unwrap(),
            Some("27AAPFU0939F1ZV".into())
        );
        assert_eq!(required("n", "  Rice ", 10).unwrap(), "Rice");
        assert!(required("n", "   ", 10).is_err());
        assert!(rate("r", 1_000_001).is_err());
        assert_eq!(date("d", "2026-04-01").unwrap(), "2026-04-01");
        assert!(date("d", "01/04/2026").is_err());
        assert_eq!(like_pattern("50%_off"), "%50\\%\\_off%");
    }

    #[test]
    fn pages_are_clamped() {
        let q = PageQuery {
            page: Some(0),
            page_size: Some(10_000),
            ..Default::default()
        };
        assert_eq!((q.page(), q.page_size()), (1, 200));
        assert_eq!(
            PageQuery {
                page: Some(3),
                page_size: Some(20),
                ..Default::default()
            }
            .limit_offset(),
            (20, 40)
        );
    }
}
