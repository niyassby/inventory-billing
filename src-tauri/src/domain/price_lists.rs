//! Which price a price list gives for a line. Port of `packages/shared/src/price-lists.ts`; both
//! pass `packages/shared/test-vectors/price-lists.json`, so the till, the hub and an offline counter
//! agree.
//!
//! 1. Only prices valid on the bill's date count (`valid_from` / `valid_to`, inclusive, either open).
//! 2. A price for the sold unit (a pack) wins over a base-unit price; a base-unit price is scaled to
//!    the pack (1 pack = factor_num / factor_den base units, rounded half up).
//! 3. Quantity slabs: the price with the highest `min_qty` the line reaches (in the price's unit).
//! 4. Ties: the most recent `valid_from`, then the first entry.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::money::{self, RoundingMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListPriceEntry {
    /// The unit the price is for; None = the product's base unit.
    pub unit_id: Option<String>,
    /// Smallest quantity (thousandths of the price's unit) the price applies from.
    pub min_qty: i64,
    /// Price per unit (minor units).
    pub price: i64,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPriceQuery<'a> {
    pub entries: Vec<ListPriceEntry>,
    #[serde(borrow)]
    pub sold_unit_id: &'a str,
    #[serde(borrow)]
    pub base_unit_id: &'a str,
    pub factor_num: i64,
    pub factor_den: i64,
    /// Thousandths of the sold unit.
    pub qty: i64,
    #[serde(borrow)]
    pub date: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPriceResult {
    /// Price per sold unit; None when the list has none for this line.
    pub price: Option<i64>,
    pub entry_index: Option<usize>,
}

fn scale(value: i64, num: i64, den: i64) -> i64 {
    i64::try_from(money::div_round(
        i128::from(value) * i128::from(num),
        i128::from(den.max(1)),
        RoundingMode::HalfUp,
    ))
    .unwrap_or(i64::MAX)
}

pub fn resolve(q: &ListPriceQuery<'_>) -> ListPriceResult {
    let valid: Vec<(usize, &ListPriceEntry)> = q
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.valid_from.as_deref().is_none_or(|f| f <= q.date)
                && e.valid_to.as_deref().is_none_or(|t| t >= q.date)
        })
        .collect();
    let sold_is_base = q.sold_unit_id == q.base_unit_id;
    let is_base = |e: &ListPriceEntry| e.unit_id.as_deref().is_none_or(|u| u == q.base_unit_id);
    let exact: Vec<(usize, &ListPriceEntry)> = valid
        .iter()
        .copied()
        .filter(|(_, e)| {
            if sold_is_base {
                is_base(e)
            } else {
                e.unit_id.as_deref() == Some(q.sold_unit_id)
            }
        })
        .collect();
    let use_base = !sold_is_base && exact.is_empty();
    let candidates: Vec<(usize, &ListPriceEntry)> = if use_base {
        valid.iter().copied().filter(|(_, e)| is_base(e)).collect()
    } else {
        exact
    };
    let slab_qty = if use_base {
        scale(q.qty, q.factor_num, q.factor_den)
    } else {
        q.qty
    };
    let mut best: Option<(usize, &ListPriceEntry)> = None;
    for (index, entry) in candidates {
        if entry.min_qty > slab_qty {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, b)) => {
                entry.min_qty > b.min_qty
                    || (entry.min_qty == b.min_qty
                        && entry.valid_from.as_deref().unwrap_or("")
                            > b.valid_from.as_deref().unwrap_or(""))
            }
        };
        if better {
            best = Some((index, entry));
        }
    }
    match best {
        None => ListPriceResult {
            price: None,
            entry_index: None,
        },
        Some((index, entry)) => ListPriceResult {
            price: Some(if use_base {
                scale(entry.price, q.factor_num, q.factor_den)
            } else {
                entry.price
            }),
            entry_index: Some(index),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../../packages/shared/test-vectors/price-lists.json");

    #[test]
    fn matches_the_typescript_rule() {
        let vectors: Value = serde_json::from_str(VECTORS).unwrap();
        let cases = vectors["cases"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for case in cases {
            let text = case["query"].to_string();
            let query: ListPriceQuery<'_> = serde_json::from_str(&text).unwrap();
            assert_eq!(
                serde_json::to_value(resolve(&query)).unwrap(),
                case["expected"],
                "{}",
                case["name"]
            );
        }
    }
}
