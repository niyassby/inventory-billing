//! Document pricing: discounts → tax (inclusive or exclusive, GST split) → round-off.
//! Port of `packages/shared/src/tax.ts`; both pass `packages/shared/test-vectors/pricing.json`.
//! The backend always prices documents itself and never trusts client totals.

use serde::{Deserialize, Serialize};

use super::money::{self, RoundingMode, RATE_SCALE};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PricingError(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxComponentRate {
    pub code: String,
    pub rate: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Discount {
    Amount { amount: i64 },
    Percent { rate: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineInput {
    pub unit_price: i64,
    pub qty: i64,
    #[serde(default)]
    pub discount: Option<Discount>,
    pub taxes: Vec<TaxComponentRate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInput {
    pub lines: Vec<LineInput>,
    pub prices_include_tax: bool,
    #[serde(default)]
    pub bill_discount: Option<Discount>,
    #[serde(default)]
    pub round_total_to: Option<i64>,
    #[serde(default)]
    pub rounding: Option<RoundingMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxAmount {
    pub code: String,
    pub rate: i64,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineResult {
    pub gross: i64,
    pub line_discount: i64,
    pub bill_discount: i64,
    pub taxable: i64,
    pub taxes: Vec<TaxAmount>,
    pub tax: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentResult {
    pub lines: Vec<LineResult>,
    pub gross: i64,
    pub line_discount: i64,
    pub bill_discount: i64,
    pub taxable: i64,
    pub tax: i64,
    pub tax_summary: Vec<TaxAmount>,
    pub total_before_rounding: i64,
    pub round_off: i64,
    pub total: i64,
}

fn validate_rate(rate: i64, label: &str) -> Result<(), PricingError> {
    if !(0..=RATE_SCALE).contains(&rate) {
        return Err(PricingError(format!(
            "{label} must be between 0 and {RATE_SCALE} ppm"
        )));
    }
    Ok(())
}

fn discount_amount(
    base: i64,
    discount: Option<&Discount>,
    mode: RoundingMode,
) -> Result<i64, PricingError> {
    match discount {
        None => Ok(0),
        Some(Discount::Amount { amount }) => {
            if *amount < 0 {
                return Err(PricingError("Discount cannot be negative".into()));
            }
            if *amount > base {
                return Err(PricingError(
                    "Discount cannot exceed the amount it applies to".into(),
                ));
            }
            Ok(*amount)
        }
        Some(Discount::Percent { rate }) => {
            validate_rate(*rate, "Discount rate")?;
            Ok(money::apply_rate(base, *rate, mode))
        }
    }
}

/// Intra-state GST splits into CGST + SGST halves; inter-state is IGST.
pub fn gst_components(rate: i64, inter_state: bool) -> Result<Vec<TaxComponentRate>, PricingError> {
    validate_rate(rate, "GST rate")?;
    if rate == 0 {
        return Ok(Vec::new());
    }
    if inter_state {
        return Ok(vec![TaxComponentRate {
            code: "IGST".into(),
            rate,
        }]);
    }
    if rate % 2 != 0 {
        return Err(PricingError(
            "GST rate cannot be split evenly into CGST and SGST".into(),
        ));
    }
    Ok(vec![
        TaxComponentRate {
            code: "CGST".into(),
            rate: rate / 2,
        },
        TaxComponentRate {
            code: "SGST".into(),
            rate: rate / 2,
        },
    ])
}

fn compute_taxes(
    net: i64,
    taxes: &[TaxComponentRate],
    inclusive: bool,
    mode: RoundingMode,
) -> (i64, Vec<TaxAmount>) {
    if taxes.is_empty() {
        return (net, Vec::new());
    }
    if !inclusive {
        let amounts = taxes
            .iter()
            .map(|t| TaxAmount {
                code: t.code.clone(),
                rate: t.rate,
                amount: money::apply_rate(net, t.rate, mode),
            })
            .collect();
        return (net, amounts);
    }
    let total_rate: i64 = taxes.iter().map(|t| t.rate).sum();
    let taxable = i64::try_from(money::div_round(
        i128::from(net) * i128::from(RATE_SCALE),
        i128::from(RATE_SCALE + total_rate),
        mode,
    ))
    .expect("taxable fits in i64");
    let tax = net - taxable;
    let parts = if total_rate == 0 {
        vec![0; taxes.len()]
    } else {
        money::allocate(tax, &taxes.iter().map(|t| t.rate).collect::<Vec<_>>())
            .expect("rates are non-negative")
    };
    let amounts = taxes
        .iter()
        .zip(parts)
        .map(|(t, amount)| TaxAmount {
            code: t.code.clone(),
            rate: t.rate,
            amount,
        })
        .collect();
    (taxable, amounts)
}

pub fn price_document(input: &DocumentInput) -> Result<DocumentResult, PricingError> {
    let mode = input.rounding.unwrap_or(RoundingMode::HalfUp);

    let mut stage = Vec::with_capacity(input.lines.len());
    for (index, line) in input.lines.iter().enumerate() {
        let n = index + 1;
        if line.qty <= 0 {
            return Err(PricingError(format!("Line {n}: quantity must be positive")));
        }
        if line.unit_price < 0 {
            return Err(PricingError(format!("Line {n}: price cannot be negative")));
        }
        for t in &line.taxes {
            validate_rate(t.rate, &format!("Line {n}: tax rate"))?;
        }
        let gross = money::multiply_by_qty(line.unit_price, line.qty, mode);
        let line_discount = discount_amount(gross, line.discount.as_ref(), mode)?;
        stage.push((line, gross, line_discount, gross - line_discount));
    }

    let discount_base: i64 = stage.iter().map(|s| s.3).sum();
    let bill_total = discount_amount(discount_base, input.bill_discount.as_ref(), mode)?;
    let shares = if bill_total == 0 {
        vec![0; stage.len()]
    } else {
        money::allocate(bill_total, &stage.iter().map(|s| s.3).collect::<Vec<_>>())
            .map_err(|e| PricingError(e.to_string()))?
    };

    let lines: Vec<LineResult> = stage
        .iter()
        .zip(shares)
        .map(|((line, gross, line_discount, after), bill_discount)| {
            let net = after - bill_discount;
            let (taxable, taxes) = compute_taxes(net, &line.taxes, input.prices_include_tax, mode);
            let tax: i64 = taxes.iter().map(|t| t.amount).sum();
            LineResult {
                gross: *gross,
                line_discount: *line_discount,
                bill_discount,
                taxable,
                taxes,
                tax,
                total: taxable + tax,
            }
        })
        .collect();

    let mut summary: Vec<TaxAmount> = Vec::new();
    for t in lines.iter().flat_map(|l| &l.taxes) {
        match summary
            .iter_mut()
            .find(|s| s.code == t.code && s.rate == t.rate)
        {
            Some(existing) => existing.amount += t.amount,
            None => summary.push(t.clone()),
        }
    }
    summary.sort_by(|a, b| a.code.cmp(&b.code).then(a.rate.cmp(&b.rate)));

    let total_before_rounding: i64 = lines.iter().map(|l| l.total).sum();
    let total = match input.round_total_to {
        Some(unit) if unit <= 0 => {
            return Err(PricingError("Rounding unit must be positive".into()))
        }
        Some(unit) => i64::try_from(
            money::div_round(i128::from(total_before_rounding), i128::from(unit), mode)
                * i128::from(unit),
        )
        .expect("total fits in i64"),
        None => total_before_rounding,
    };

    Ok(DocumentResult {
        gross: lines.iter().map(|l| l.gross).sum(),
        line_discount: lines.iter().map(|l| l.line_discount).sum(),
        bill_discount: bill_total,
        taxable: lines.iter().map(|l| l.taxable).sum(),
        tax: lines.iter().map(|l| l.tax).sum(),
        tax_summary: summary,
        total_before_rounding,
        round_off: total - total_before_rounding,
        total,
        lines,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tender {
    pub applied: i64,
    pub change: i64,
    pub due: i64,
}

/// Splits what the customer handed over into applied / change / due (fixes audit B-4).
pub fn settle_tender(total: i64, tendered: i64) -> Tender {
    if tendered >= total {
        Tender {
            applied: total,
            change: tendered - total,
            due: 0,
        }
    } else {
        Tender {
            applied: tendered.max(0),
            change: 0,
            due: total - tendered.max(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../../packages/shared/test-vectors/pricing.json");

    #[test]
    fn matches_the_typescript_engine() {
        let vectors: Value = serde_json::from_str(VECTORS).unwrap();
        for case in vectors["cases"].as_array().unwrap() {
            let input: DocumentInput = serde_json::from_value(case["input"].clone()).unwrap();
            let result = price_document(&input).unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
            assert_eq!(
                serde_json::to_value(&result).unwrap(),
                case["expected"],
                "{}",
                case["name"]
            );
        }
        for case in vectors["errors"].as_array().unwrap() {
            let input: DocumentInput = serde_json::from_value(case["input"].clone()).unwrap();
            assert!(
                price_document(&input).is_err(),
                "{} should fail",
                case["name"]
            );
        }
    }

    #[test]
    fn gst_split_and_tender() {
        assert_eq!(gst_components(180_000, false).unwrap().len(), 2);
        assert_eq!(gst_components(180_000, true).unwrap()[0].code, "IGST");
        assert!(gst_components(0, false).unwrap().is_empty());
        assert!(gst_components(1, false).is_err());
        assert_eq!(
            settle_tender(23_600, 25_000),
            Tender {
                applied: 23_600,
                change: 1_400,
                due: 0
            }
        );
        assert_eq!(
            settle_tender(1_000, 400),
            Tender {
                applied: 400,
                change: 0,
                due: 600
            }
        );
    }
}
