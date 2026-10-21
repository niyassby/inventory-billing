//! Benchmarks against the Master Prompt §12 targets, on generated data. They are slow to set up,
//! so they only run on request, and in a release build the numbers are the real ones:
//!
//! `cargo test --release --lib perf -- --ignored --nocapture`

use std::time::{Duration, Instant};

use crate::services::catalog::{self, ProductListQuery, SearchQuery};
use crate::services::common::PageQuery;
use crate::services::reports::{self, ReportQuery};
use crate::services::testkit::Kit;

const PRODUCTS: i64 = 50_000;
const MOVEMENTS: i64 = 100_000;

fn timed<T>(label: &str, runs: u32, mut f: impl FnMut() -> T) -> Duration {
    let mut worst = Duration::ZERO;
    for _ in 0..runs {
        let start = Instant::now();
        std::hint::black_box(f());
        worst = worst.max(start.elapsed());
    }
    println!("{label}: worst of {runs} = {worst:?}");
    worst
}

/// Bulk-loads products (with an opening stock each) and a year of sales movements with SQL, which
/// is how a migrated shop's data arrives; the services are then run over it.
fn big_shop() -> Kit {
    let kit = Kit::new();
    kit.product("Seed item", "SEED-1", 10_000, 5_000, 1, None);
    let conn = kit.conn();
    let started = Instant::now();
    conn.execute_batch(&format!(
        "BEGIN;
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {PRODUCTS})
         INSERT INTO products (id, name, base_unit_id, created_at, updated_at)
         SELECT printf('perf-p%06d', i),
                CASE i % 5 WHEN 0 THEN 'Basmati rice' WHEN 1 THEN 'Sunflower oil' WHEN 2 THEN 'Steel tumbler'
                           WHEN 3 THEN 'Notebook' ELSE 'Cotton towel' END || ' ' || i,
                (SELECT id FROM units LIMIT 1), '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
         FROM n;
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {PRODUCTS})
         INSERT INTO product_variants (id, product_id, sku, barcode, sale_price, cost_price, created_at, updated_at)
         SELECT printf('perf-v%06d', i), printf('perf-p%06d', i), printf('PERF-%06d', i),
                '89' || printf('%011d', i), 10000 + i % 500, 6000, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
         FROM n;
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {PRODUCTS})
         INSERT INTO stock_movements (id, business_date, occurred_at, warehouse_id, variant_id, movement_type,
                                      qty, unit_cost, value, source_type, source_id, created_at)
         SELECT printf('perf-open%06d', i), '2026-01-01', '2026-01-01T00:00:00Z', (SELECT id FROM warehouses LIMIT 1),
                printf('perf-v%06d', i), 'opening', 500000, 6000, 3000000, 'opening', 'perf', '2026-01-01T00:00:00Z'
         FROM n;
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {MOVEMENTS})
         INSERT INTO stock_movements (id, business_date, occurred_at, warehouse_id, variant_id, movement_type,
                                      qty, unit_cost, value, source_type, source_id, created_at)
         SELECT printf('perf-sale%07d', i), date('2026-01-02', '+' || (i % 300) || ' days'),
                '2026-01-02T00:00:00Z', (SELECT id FROM warehouses LIMIT 1),
                printf('perf-v%06d', 1 + (i * 7) % {PRODUCTS}), 'sale', -1000, 6000, -6000, 'sale',
                printf('perf-doc%07d', i), '2026-01-02T00:00:00Z'
         FROM n;
         COMMIT;"
    ))
    .unwrap();
    println!(
        "loaded {PRODUCTS} products and {} movements in {:?}",
        PRODUCTS + MOVEMENTS,
        started.elapsed()
    );
    drop(conn);
    kit
}

#[test]
#[ignore = "benchmark: run with --release --ignored --nocapture"]
fn targets_hold_with_50k_products_and_150k_movements() {
    let kit = big_shop();
    let owner = kit.owner();
    let mut conn = kit.conn();

    let search = timed("product search by name (word prefix)", 20, || {
        catalog::search(
            &kit.ctx(&mut conn, &owner),
            &SearchQuery {
                term: "basmati ri".into(),
                limit: Some(20),
                warehouse_id: None,
            },
        )
        .unwrap()
    });
    let scan = timed("barcode scan", 20, || {
        catalog::lookup_code(
            &kit.ctx(&mut conn, &owner),
            &SearchQuery {
                term: "89000000025000".into(),
                limit: None,
                warehouse_id: None,
            },
        )
        .unwrap()
    });
    let list = timed("products list, page 1 of 250", 10, || {
        catalog::list_products(
            &kit.ctx(&mut conn, &owner),
            &ProductListQuery {
                search: None,
                category_id: None,
                brand_id: None,
                low_stock_only: false,
                include_inactive: false,
                page: PageQuery {
                    page: Some(1),
                    page_size: Some(200),
                    ..Default::default()
                },
            },
        )
        .unwrap()
    });
    let mut report_time = |name: &str| {
        timed(&format!("report {name}, one year"), 3, || {
            reports::run(
                &kit.ctx(&mut conn, &owner),
                &ReportQuery {
                    report: name.into(),
                    from: Some("2026-01-01".into()),
                    to: Some("2026-12-31".into()),
                    warehouse_id: None,
                    party_type: None,
                    account_id: None,
                },
            )
            .unwrap()
        })
    };
    let valuation = report_time("stock_valuation");
    let movement = report_time("stock_movement");

    assert!(search < Duration::from_millis(100), "search {search:?}");
    assert!(scan < Duration::from_millis(100), "scan {scan:?}");
    assert!(list < Duration::from_millis(300), "list {list:?}");
    assert!(
        valuation < Duration::from_secs(3),
        "valuation {valuation:?}"
    );
    assert!(movement < Duration::from_secs(3), "movement {movement:?}");
}
