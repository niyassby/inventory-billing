//! Money and rounding, mirroring `packages/shared/src/{money,rounding}.ts`.
//!
//! Money is `i64` minor units (paise). Quantities are `i64` thousandths. Rates are `i64` ppm.

/// Quantities are fixed-point with 3 decimals.
pub const QTY_SCALE: i64 = 1_000;
/// 100% in parts per million.
pub const RATE_SCALE: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// .5 away from zero (commercial rounding; the default).
    HalfUp,
    /// .5 to the even neighbour.
    HalfEven,
    /// Towards zero.
    Down,
    /// Away from zero.
    Up,
}

/// Integer division with explicit rounding. Uses i128 so price × qty × rate can't overflow.
///
/// # Panics
/// When `denominator` is zero (a programming error, never user input).
pub fn div_round(numerator: i128, denominator: i128, mode: RoundingMode) -> i128 {
    assert!(denominator != 0, "division by zero");
    let (numerator, denominator) = if denominator < 0 {
        (-numerator, -denominator)
    } else {
        (numerator, denominator)
    };
    let negative = numerator < 0;
    let abs = numerator.abs();
    let quotient = abs / denominator;
    let remainder = abs % denominator;
    let mut rounded = quotient;
    if remainder != 0 {
        let twice = remainder * 2;
        rounded = match mode {
            RoundingMode::Down => quotient,
            RoundingMode::Up => quotient + 1,
            RoundingMode::HalfUp if twice >= denominator => quotient + 1,
            RoundingMode::HalfUp => quotient,
            RoundingMode::HalfEven
                if twice > denominator || (twice == denominator && quotient % 2 == 1) =>
            {
                quotient + 1
            }
            RoundingMode::HalfEven => quotient,
        };
    }
    if negative {
        -rounded
    } else {
        rounded
    }
}

/// unit price × quantity (thousandths), rounded once.
pub fn multiply_by_qty(unit_price: i64, qty: i64, mode: RoundingMode) -> i64 {
    to_i64(div_round(
        i128::from(unit_price) * i128::from(qty),
        i128::from(QTY_SCALE),
        mode,
    ))
}

/// amount × rate (ppm), rounded once.
pub fn apply_rate(amount: i64, rate_ppm: i64, mode: RoundingMode) -> i64 {
    to_i64(div_round(
        i128::from(amount) * i128::from(rate_ppm),
        i128::from(RATE_SCALE),
        mode,
    ))
}

fn to_i64(value: i128) -> i64 {
    i64::try_from(value).expect("money value out of i64 range")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AllocationError {
    #[error("cannot allocate a non-zero total over no weights or all-zero weights")]
    NoWeight,
    #[error("weights must not be negative")]
    NegativeWeight,
}

/// Splits `total` over `weights` with the largest-remainder method. Parts always sum exactly to
/// `total`; ties go to the earliest index. Identical to `allocateProportionally` in TypeScript.
pub fn allocate(total: i64, weights: &[i64]) -> Result<Vec<i64>, AllocationError> {
    if weights.iter().any(|w| *w < 0) {
        return Err(AllocationError::NegativeWeight);
    }
    let weight_sum: i128 = weights.iter().map(|w| i128::from(*w)).sum();
    if weight_sum == 0 {
        return if total == 0 {
            Ok(vec![0; weights.len()])
        } else {
            Err(AllocationError::NoWeight)
        };
    }
    let sign: i128 = if total < 0 { -1 } else { 1 };
    let abs_total = i128::from(total).abs();
    let parts: Vec<(i128, i128)> = weights
        .iter()
        .map(|w| {
            let exact = abs_total * i128::from(*w);
            (exact / weight_sum, exact % weight_sum)
        })
        .collect();
    let mut leftover = abs_total - parts.iter().map(|(floor, _)| floor).sum::<i128>();
    let mut order: Vec<usize> = (0..parts.len()).collect();
    // Stable sort keeps the earliest index first among equal remainders.
    order.sort_by(|a, b| parts[*b].1.cmp(&parts[*a].1));
    let mut result: Vec<i128> = parts.iter().map(|(floor, _)| *floor).collect();
    for index in order {
        if leftover == 0 {
            break;
        }
        result[index] += 1;
        leftover -= 1;
    }
    Ok(result.into_iter().map(|v| to_i64(v * sign)).collect())
}

/// Converts a legacy REAL major amount to minor units. Rounds like JavaScript's `Math.round`
/// (.5 towards +infinity) so the Rust migration and the TS helper agree exactly. `lossy` is true
/// when rounding changed the value by more than 0.001 of a minor unit.
pub fn from_legacy_float(major: f64, minor_digits: u32) -> Option<(i64, bool)> {
    if !major.is_finite() {
        return None;
    }
    let scaled = major * 10f64.powi(i32::try_from(minor_digits).ok()?);
    let rounded = (scaled + 0.5).floor();
    if rounded.abs() > 9_007_199_254_740_991.0 {
        return None;
    }
    // In range of the safe-integer check above, so the cast is exact.
    #[allow(clippy::cast_possible_truncation)]
    let amount = rounded as i64;
    Some((amount, (scaled - rounded).abs() > 0.001))
}

/// Minor units as plain text for messages: 12345 with 2 digits is "123.45", -5 is "-0.05".
pub fn format_minor(amount: i64, minor_digits: u32) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    if minor_digits == 0 {
        return format!("{sign}{abs}");
    }
    let scale = 10u64.pow(minor_digits.min(6));
    let width = minor_digits.min(6) as usize;
    format!("{sign}{}.{:0width$}", abs / scale, abs % scale)
}

/// Legacy REAL quantity (e.g. 2.5 kg) to thousandths.
pub fn qty_from_legacy_float(qty: f64) -> Option<(i64, bool)> {
    from_legacy_float(qty, 3)
}

#[cfg(test)]
mod format_tests {
    use super::format_minor;

    #[test]
    fn formats_minor_units() {
        assert_eq!(format_minor(12_345, 2), "123.45");
        assert_eq!(format_minor(-5, 2), "-0.05");
        assert_eq!(format_minor(7, 0), "7");
        assert_eq!(format_minor(1_000, 3), "1.000");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../../packages/shared/test-vectors/money.json");

    fn vectors() -> Value {
        serde_json::from_str(VECTORS).expect("valid vectors")
    }

    fn int(v: &Value, key: &str) -> i64 {
        v[key]
            .as_i64()
            .unwrap_or_else(|| panic!("{key} missing in {v}"))
    }

    #[test]
    fn div_round_matches_shared_vectors() {
        for v in vectors()["divRound"].as_array().unwrap() {
            let (n, d) = (i128::from(int(v, "n")), i128::from(int(v, "d")));
            assert_eq!(
                div_round(n, d, RoundingMode::HalfUp),
                i128::from(int(v, "half_up")),
                "{v}"
            );
            assert_eq!(
                div_round(n, d, RoundingMode::HalfEven),
                i128::from(int(v, "half_even")),
                "{v}"
            );
            assert_eq!(
                div_round(n, d, RoundingMode::Down),
                i128::from(int(v, "down")),
                "{v}"
            );
            assert_eq!(
                div_round(n, d, RoundingMode::Up),
                i128::from(int(v, "up")),
                "{v}"
            );
        }
    }

    #[test]
    fn allocate_matches_shared_vectors() {
        for v in vectors()["allocate"].as_array().unwrap() {
            let weights: Vec<i64> = v["weights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_i64().unwrap())
                .collect();
            let expected: Vec<i64> = v["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_i64().unwrap())
                .collect();
            assert_eq!(
                allocate(int(v, "total"), &weights).unwrap(),
                expected,
                "{v}"
            );
        }
    }

    #[test]
    fn legacy_float_matches_shared_vectors() {
        for v in vectors()["legacyFloat"].as_array().unwrap() {
            let digits = u32::try_from(int(v, "minorDigits")).unwrap();
            let (amount, lossy) = from_legacy_float(v["major"].as_f64().unwrap(), digits).unwrap();
            assert_eq!(amount, int(v, "amount"), "{v}");
            assert_eq!(lossy, v["lossy"].as_bool().unwrap(), "{v}");
        }
    }

    #[test]
    fn allocation_errors() {
        assert_eq!(allocate(5, &[]), Err(AllocationError::NoWeight));
        assert_eq!(allocate(5, &[0, 0]), Err(AllocationError::NoWeight));
        assert_eq!(allocate(5, &[-1, 2]), Err(AllocationError::NegativeWeight));
        assert_eq!(allocate(0, &[]), Ok(vec![]));
    }

    #[test]
    fn rates_and_quantities() {
        assert_eq!(apply_rate(10_000, 180_000, RoundingMode::HalfUp), 1_800);
        assert_eq!(multiply_by_qty(4_550, 1_255, RoundingMode::HalfUp), 5_710);
        assert_eq!(qty_from_legacy_float(2.5), Some((2_500, false)));
        assert_eq!(from_legacy_float(f64::NAN, 2), None);
    }
}
