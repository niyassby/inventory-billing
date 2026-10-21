//! Server-side sorting and paging of the lists: the order holds across pages, only whitelisted
//! columns can be sorted, and a stock card's running balance is right on every page.

use super::reporting::sale;
use crate::error::ErrorCode;
use crate::services::catalog::{self, ProductListQuery};
use crate::services::common::PageQuery;
use crate::services::parties::{self, PartyListQuery};
use crate::services::sales::{self, InvoiceListQuery};
use crate::services::stock::{self, StockCardQuery};
use crate::services::testkit::{code, Kit};

fn page(size: u32, number: u32, sort: Option<&str>, descending: bool) -> PageQuery {
    PageQuery {
        page: Some(number),
        page_size: Some(size),
        sort: sort.map(String::from),
        descending,
    }
}

/// Five products at ₹10 … ₹50 and five cash invoices of different sizes.
fn shop() -> (Kit, Vec<String>) {
    let kit = Kit::new();
    let mut variants = Vec::new();
    for (i, price) in [1_000, 2_000, 3_000, 4_000, 5_000].into_iter().enumerate() {
        variants.push(kit.product(
            &format!("Item {}", (b'A' + u8::try_from(i).unwrap()) as char),
            &format!("LST-{i}"),
            price,
            price / 2,
            100,
            None,
        ));
    }
    let owner = kit.owner();
    let mut conn = kit.conn();
    // totals: 5_000, 2_000, 9_000, 4_000, 25_000
    for (n, (variant, units, total)) in [
        (0usize, 5, 5_000),
        (1, 1, 2_000),
        (2, 3, 9_000),
        (3, 1, 4_000),
        (4, 5, 25_000),
    ]
    .into_iter()
    .enumerate()
    {
        sales::create_invoice(
            &mut kit.ctx(&mut conn, &owner),
            &sale(
                &format!("sale-lst-{n:04}"),
                &variants[variant],
                units,
                total,
            ),
        )
        .unwrap();
    }
    drop(conn);
    (kit, variants)
}

fn invoice_totals(kit: &Kit, size: u32, sort: Option<&str>, descending: bool) -> Vec<i64> {
    let owner = kit.owner();
    let mut conn = kit.conn();
    let first = sales::list_invoices(
        &kit.ctx(&mut conn, &owner),
        &InvoiceListQuery {
            page: page(size, 1, sort, descending),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(first.total, 5);
    let pages = (first.total + i64::from(size) - 1) / i64::from(size);
    let mut totals: Vec<i64> = first.items.iter().map(|i| i.total).collect();
    for number in 2..=u32::try_from(pages).unwrap() {
        let next = sales::list_invoices(
            &kit.ctx(&mut conn, &owner),
            &InvoiceListQuery {
                page: page(size, number, sort, descending),
                ..Default::default()
            },
        )
        .unwrap();
        totals.extend(next.items.iter().map(|i| i.total));
    }
    totals
}

#[test]
fn invoices_sort_across_every_page() {
    let (kit, _) = shop();
    assert_eq!(
        invoice_totals(&kit, 2, Some("total"), false),
        vec![2_000, 4_000, 5_000, 9_000, 25_000]
    );
    assert_eq!(
        invoice_totals(&kit, 2, Some("total"), true),
        vec![25_000, 9_000, 5_000, 4_000, 2_000]
    );
    // No sort: the list's own order (newest day first; the test clock stands still, so the
    // invoices of this one day come in the order they were made) and stable across pages.
    let default = invoice_totals(&kit, 2, None, false);
    assert_eq!(default, vec![5_000, 2_000, 9_000, 4_000, 25_000]);
    assert_eq!(default, invoice_totals(&kit, 50, None, false));
    // A bigger page gives the same order as small ones.
    assert_eq!(
        invoice_totals(&kit, 50, Some("total"), true),
        invoice_totals(&kit, 2, Some("total"), true)
    );
}

#[test]
fn only_listed_columns_can_be_sorted() {
    let (kit, _) = shop();
    let owner = kit.owner();
    let mut conn = kit.conn();
    for bad in [
        "nope",
        "total; DROP TABLE sales_invoices",
        "total DESC, (SELECT 1)",
        "doc_no", // a real column, but not on the whitelist (the key is `docNo`)
    ] {
        let result = sales::list_invoices(
            &kit.ctx(&mut conn, &owner),
            &InvoiceListQuery {
                page: page(10, 1, Some(bad), false),
                ..Default::default()
            },
        );
        assert_eq!(code(result), ErrorCode::Validation, "{bad}");
    }
    // The table is still there.
    let ok = sales::list_invoices(&kit.ctx(&mut conn, &owner), &InvoiceListQuery::default());
    assert_eq!(ok.unwrap().total, 5);
}

#[test]
fn products_sort_by_price_and_stock_across_pages() {
    let (kit, _) = shop();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut names = Vec::new();
    for number in 1..=3 {
        let p = catalog::list_products(
            &kit.ctx(&mut conn, &owner),
            &ProductListQuery {
                page: page(2, number, Some("price"), true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.total, 5);
        names.extend(p.items.into_iter().map(|i| i.name));
    }
    assert_eq!(names, ["Item E", "Item D", "Item C", "Item B", "Item A"]);

    // Stock: the five sales left 95, 99, 97, 99, 95 units.
    let by_stock = catalog::list_products(
        &kit.ctx(&mut conn, &owner),
        &ProductListQuery {
            page: page(5, 1, Some("stock"), false),
            ..Default::default()
        },
    )
    .unwrap();
    let stocks: Vec<i64> = by_stock.items.iter().map(|i| i.stock).collect();
    let mut sorted = stocks.clone();
    sorted.sort_unstable();
    assert_eq!(stocks, sorted);
    assert_eq!(stocks.first(), Some(&95_000));
}

#[test]
fn parties_sort_by_name_and_balance() {
    let kit = Kit::new();
    for name in ["Moe", "Zed", "Amy", "Kim"] {
        kit.party(name, "customer", None);
    }
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut names = Vec::new();
    for number in 1..=2 {
        let p = parties::list(
            &kit.ctx(&mut conn, &owner),
            &PartyListQuery {
                party_type: Some("customer".into()),
                page: page(2, number, Some("name"), true),
                ..Default::default()
            },
        )
        .unwrap();
        names.extend(p.items.into_iter().map(|i| i.name));
    }
    assert_eq!(names, ["Zed", "Moe", "Kim", "Amy"]);
    let bad = parties::list(
        &kit.ctx(&mut conn, &owner),
        &PartyListQuery {
            page: page(2, 1, Some("password_hash"), false),
            ..Default::default()
        },
    );
    assert_eq!(code(bad), ErrorCode::Validation);
}

#[test]
fn a_stock_card_page_shows_the_true_running_balance() {
    let (kit, variants) = shop();
    // Item A: opening 100, then one sale of 5 units (see `shop`). Add two more movements.
    let owner = kit.owner();
    let mut conn = kit.conn();
    for n in 0..2 {
        sales::create_invoice(
            &mut kit.ctx(&mut conn, &owner),
            &sale(&format!("sale-card-{n:03}"), &variants[0], 1, 1_000),
        )
        .unwrap();
    }
    let mut card = |number: u32, descending: bool| {
        stock::stock_card(
            &kit.ctx(&mut conn, &owner),
            &StockCardQuery {
                variant_id: variants[0].clone(),
                warehouse_id: None,
                from: None,
                to: None,
                page: page(2, number, None, descending),
            },
        )
        .unwrap()
    };
    let first = card(1, false);
    let second = card(2, false);
    assert_eq!(first.total, 4); // opening + three sales
    assert_eq!(first.closing_qty, 100_000 - 5_000 - 1_000 - 1_000);
    assert_eq!(second.closing_qty, first.closing_qty);
    let balances: Vec<i64> = first
        .lines
        .iter()
        .chain(second.lines.iter())
        .map(|l| l.balance)
        .collect();
    assert_eq!(balances, [100_000, 95_000, 94_000, 93_000]);
    // Newest first reverses the same ledger without changing any balance.
    let newest = card(1, true);
    assert_eq!(
        newest.lines.iter().map(|l| l.balance).collect::<Vec<_>>(),
        [93_000, 94_000]
    );
    assert_eq!(newest.page_size, 2);
}
