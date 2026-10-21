//! Catalogue, parties and stock documents.

use crate::error::ErrorCode;
use crate::services::catalog::{
    self, ProductInput, ProductListQuery, SearchQuery, UnitConversionInput, UnitInput, VariantInput,
};
use crate::services::common::PageQuery;
use crate::services::parties::{self, PartyInput, PartyListQuery, StatementQuery};
use crate::services::stock::{
    self, AdjustInput, AdjustLine, CountEntriesInput, CountEntry, CountFinalizeInput,
    CountStartInput, StockCardQuery, TransferInput, TransferLine,
};
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

fn variant(sku: &str, price: i64) -> VariantInput {
    VariantInput {
        id: None,
        variant_name: None,
        sku: sku.into(),
        barcode: None,
        sale_price: price,
        mrp: None,
        cost_price: Some(price / 2),
        reorder_level: 0,
        reorder_qty: 0,
        is_active: true,
        opening_stock: None,
    }
}

fn product_input(kit: &Kit, name: &str, variants: Vec<VariantInput>) -> ProductInput {
    let unit: String = kit
        .conn()
        .query_row("SELECT id FROM units WHERE code = 'pcs'", [], |r| r.get(0))
        .unwrap_or_else(|_| {
            let owner = kit.owner();
            let mut conn = kit.conn();
            catalog::save_unit(
                &mut kit.ctx(&mut conn, &owner),
                &UnitInput {
                    id: None,
                    code: "pcs".into(),
                    name: "Pieces".into(),
                    decimals: 0,
                },
            )
            .unwrap()
            .id
        });
    ProductInput {
        id: None,
        expected_updated_at: None,
        name: name.into(),
        description: None,
        category_id: None,
        brand_id: None,
        base_unit_id: unit,
        tax_rate_id: None,
        hsn_sac: None,
        track_batches: false,
        track_expiry: false,
        is_service: false,
        is_active: true,
        variants,
        unit_conversions: Vec::new(),
    }
}

#[test]
fn products_with_variants_pack_barcodes_and_search() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut input = product_input(
        &kit,
        "Cotton T-Shirt",
        vec![variant("TS-S", 29900), variant("TS-M", 29900)],
    );
    input.variants[0].variant_name = Some("Small".into());
    input.variants[1].variant_name = Some("Medium".into());
    input.variants[0].barcode = Some("8901000000017".into());
    let box_unit = catalog::save_unit(
        &mut kit.ctx(&mut conn, &owner),
        &UnitInput {
            id: None,
            code: "ctn".into(),
            name: "Carton".into(),
            decimals: 0,
        },
    )
    .unwrap()
    .id;
    input.unit_conversions = vec![UnitConversionInput {
        from_unit_id: box_unit,
        factor_num: 12,
        factor_den: 1,
        barcode: Some("8901000000999".into()),
    }];
    let product = catalog::save_product(&mut kit.ctx(&mut conn, &owner), &input).unwrap();
    assert_eq!(product.variants.len(), 2);

    let ctx = kit.ctx(&mut conn, &owner);
    let by_barcode = catalog::lookup_code(
        &ctx,
        &SearchQuery {
            term: "8901000000017".into(),
            limit: None,
            warehouse_id: None,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(by_barcode.sku, "TS-S");
    let pack = catalog::lookup_code(
        &ctx,
        &SearchQuery {
            term: "8901000000999".into(),
            limit: None,
            warehouse_id: None,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        (pack.unit_code.as_str(), pack.factor_num, pack.sale_price),
        ("ctn", 12, 12 * 29900)
    );
    let by_sku = catalog::lookup_code(
        &ctx,
        &SearchQuery {
            term: "ts-m".into(),
            limit: None,
            warehouse_id: None,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(by_sku.name, "Cotton T-Shirt — Medium");
    let found = catalog::search(
        &ctx,
        &SearchQuery {
            term: "cott shi".into(),
            limit: None,
            warehouse_id: None,
        },
    )
    .unwrap();
    assert_eq!(found.len(), 2);
    assert!(catalog::search(
        &ctx,
        &SearchQuery {
            term: "\"); DROP".into(),
            limit: None,
            warehouse_id: None
        }
    )
    .unwrap()
    .is_empty());

    let page = catalog::list_products(
        &ctx,
        &ProductListQuery {
            search: Some("shirt".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 2);
}

#[test]
fn product_rules() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut ctx = kit.ctx(&mut conn, &owner);

    let dup = product_input(&kit, "Dup", vec![variant("A-1", 100), variant("a-1", 100)]);
    assert_eq!(
        code(catalog::save_product(&mut ctx, &dup)),
        ErrorCode::Validation
    );
    let mut above_mrp = product_input(&kit, "X", vec![variant("X-1", 200)]);
    above_mrp.variants[0].mrp = Some(150);
    assert_eq!(
        code(catalog::save_product(&mut ctx, &above_mrp)),
        ErrorCode::Validation
    );

    let created = catalog::save_product(
        &mut ctx,
        &product_input(&kit, "Soap", vec![variant("SOAP-1", 4500)]),
    )
    .unwrap();
    let taken = catalog::save_product(
        &mut ctx,
        &product_input(&kit, "Other soap", vec![variant("soap-1", 4500)]),
    );
    assert_eq!(code(taken), ErrorCode::Conflict);

    // Optimistic concurrency.
    let mut stale = product_input(&kit, "Soap bar", vec![variant("SOAP-1", 4500)]);
    stale.id = Some(created.id.clone());
    stale.variants[0].id = Some(created.variants[0].id.clone());
    stale.expected_updated_at = Some("2000-01-01T00:00:00+00:00".into());
    assert_eq!(
        code(catalog::save_product(&mut ctx, &stale)),
        ErrorCode::Conflict
    );
    stale.expected_updated_at = Some(created.updated_at.clone());
    let renamed = catalog::save_product(&mut ctx, &stale).unwrap();
    assert_eq!(renamed.name, "Soap bar");
    let search = catalog::search(
        &ctx,
        &SearchQuery {
            term: "bar".into(),
            limit: None,
            warehouse_id: None,
        },
    )
    .unwrap();
    assert_eq!(search.len(), 1, "search index follows renames");

    catalog::delete_product(
        &mut ctx,
        &IdInput {
            id: created.id.clone(),
        },
    )
    .unwrap();
    let after = catalog::list_products(&ctx, &ProductListQuery::default()).unwrap();
    assert_eq!(after.total, 0);
}

#[test]
fn price_changes_need_permission_and_are_audited() {
    let kit = Kit::new();
    let variant_id = kit.product("Rice", "RICE-1", 50000, 40000, 0, None);
    let keeper = kit.user("stock_keeper");
    let mut conn = kit.conn();
    let product_id: String = conn
        .query_row(
            "SELECT product_id FROM product_variants WHERE id = ?1",
            [&variant_id],
            |r| r.get(0),
        )
        .unwrap();
    let mut input = product_input(&kit, "Rice", vec![variant("RICE-1", 55000)]);
    input.id = Some(product_id);
    input.variants[0].id = Some(variant_id);
    input.variants[0].cost_price = None;
    let denied = catalog::save_product(&mut kit.ctx(&mut conn, &keeper), &input);
    assert_eq!(code(denied), ErrorCode::ApprovalRequired);

    let manager = kit.user("manager");
    let approver = kit.approver(&manager);
    let mut ctx = kit.ctx(&mut conn, &keeper);
    ctx.approver = Some(&approver);
    let saved = catalog::save_product(&mut ctx, &input).unwrap();
    assert_eq!(saved.variants[0].sale_price, 55000);
    let approved_by: String = conn
        .query_row(
            "SELECT approved_by FROM audit_log WHERE action = 'PRICE_CHANGE'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(approved_by, manager.user_id);
}

#[test]
fn party_balances_only_move_through_the_ledger() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut ctx = kit.ctx(&mut conn, &owner);
    let input = PartyInput {
        id: None,
        expected_updated_at: None,
        party_type: "customer".into(),
        code: Some("C001".into()),
        name: "Hotel Annapurna".into(),
        phone: Some("98450 12345".into()),
        email: None,
        gstin: Some("29AAGCB7383J1Z4".into()),
        state_code: None,
        billing_address: None,
        shipping_address: None,
        credit_limit: Some(5_000_000),
        credit_days: Some(30),
        notes: None,
        is_active: true,
        opening_balance: Some(250_000),
    };
    let party = parties::save(&mut ctx, &input).unwrap();
    assert_eq!(
        (party.balance, party.state_code.as_deref()),
        (250_000, Some("29"))
    );
    let mut edit = input.clone();
    edit.id = Some(party.id.clone());
    edit.opening_balance = Some(1);
    assert_eq!(code(parties::save(&mut ctx, &edit)), ErrorCode::Validation);
    assert_eq!(
        code(parties::delete(
            &mut ctx,
            &IdInput {
                id: party.id.clone()
            }
        )),
        ErrorCode::Conflict
    );
    let mut bad_gstin = input.clone();
    bad_gstin.gstin = Some("29AAGCB7383J1Z5".into());
    assert_eq!(
        code(parties::save(&mut ctx, &bad_gstin)),
        ErrorCode::Validation
    );

    let statement = parties::statement(
        &ctx,
        &StatementQuery {
            party_id: party.id.clone(),
            from: None,
            to: None,
        },
    )
    .unwrap();
    assert_eq!(
        (statement.lines.len(), statement.closing_balance),
        (1, 250_000)
    );
    let found = parties::list(
        &ctx,
        &PartyListQuery {
            search: Some("98450".into()),
            party_type: Some("customer".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(found.total, 1);
    kit.assert_books_balance();
}

#[test]
fn adjustments_transfers_and_the_negative_stock_policy() {
    let kit = Kit::new();
    let rice = kit.product("Rice", "RICE-1", 50000, 40000, 10, None);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let warehouse: String = conn
        .query_row("SELECT id FROM warehouses WHERE is_default = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let branch: String = conn
        .query_row("SELECT id FROM branches", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO warehouses (id, branch_id, code, name, created_at, updated_at) VALUES ('w2', ?1, 'BACK', 'Back store', ?2, ?2)",
        rusqlite::params![branch, kit.now().to_rfc3339()],
    )
    .unwrap();
    let mut ctx = kit.ctx(&mut conn, &owner);

    let damaged = stock::adjust(
        &mut ctx,
        &AdjustInput {
            idempotency_key: "adj-0000001".into(),
            warehouse_id: None,
            reason: "Damaged in rain".into(),
            notes: None,
            lines: vec![AdjustLine {
                variant_id: rice.clone(),
                batch_id: None,
                qty_change: -2000,
                unit_cost: None,
            }],
        },
    )
    .unwrap();
    assert!(damaged.doc_no.starts_with("ADJ/26-27/"));
    // Retrying the same request returns the same document instead of adjusting twice.
    let retry = stock::adjust(
        &mut ctx,
        &AdjustInput {
            idempotency_key: "adj-0000001".into(),
            warehouse_id: None,
            reason: "Damaged in rain".into(),
            notes: None,
            lines: vec![AdjustLine {
                variant_id: rice.clone(),
                batch_id: None,
                qty_change: -2000,
                unit_cost: None,
            }],
        },
    )
    .unwrap();
    assert_eq!(retry.id, damaged.id);
    assert_eq!(kit.stock(&rice), 8000);

    let mut ctx = kit.ctx(&mut conn, &owner);
    let too_much = stock::adjust(
        &mut ctx,
        &AdjustInput {
            idempotency_key: "adj-0000002".into(),
            warehouse_id: None,
            reason: "Theft".into(),
            notes: None,
            lines: vec![AdjustLine {
                variant_id: rice.clone(),
                batch_id: None,
                qty_change: -9000,
                unit_cost: None,
            }],
        },
    );
    assert_eq!(code(too_much), ErrorCode::InsufficientStock);

    stock::transfer(
        &mut ctx,
        &TransferInput {
            idempotency_key: "trf-0000001".into(),
            from_warehouse_id: warehouse.clone(),
            to_warehouse_id: "w2".into(),
            notes: None,
            lines: vec![TransferLine {
                variant_id: rice.clone(),
                batch_id: None,
                qty: 3000,
            }],
        },
    )
    .unwrap();
    let card = stock::stock_card(
        &ctx,
        &StockCardQuery {
            variant_id: rice.clone(),
            warehouse_id: Some("w2".into()),
            from: None,
            to: None,
            page: PageQuery::default(),
        },
    )
    .unwrap();
    assert_eq!(card.closing_qty, 3000);
    let all = stock::stock_card(
        &ctx,
        &StockCardQuery {
            variant_id: rice.clone(),
            warehouse_id: None,
            from: None,
            to: None,
            page: PageQuery::default(),
        },
    )
    .unwrap();
    assert_eq!((all.lines.len(), all.closing_qty), (4, 8000));
    kit.assert_books_balance();
}

#[test]
fn stock_take_posts_only_counted_variances() {
    let kit = Kit::new();
    let rice = kit.product("Rice", "RICE-1", 50000, 40000, 10, None);
    let dal = kit.product("Dal", "DAL-1", 12000, 10000, 5, None);
    let keeper = kit.user("stock_keeper");
    let mut conn = kit.conn();
    let mut ctx = kit.ctx(&mut conn, &keeper);
    let count = stock::start_count(
        &mut ctx,
        &CountStartInput {
            warehouse_id: None,
            scope_note: Some("Monthly".into()),
            variant_ids: None,
            category_id: None,
        },
    )
    .unwrap();
    assert_eq!(count.lines.len(), 2);
    let again = stock::start_count(
        &mut ctx,
        &CountStartInput {
            warehouse_id: None,
            scope_note: None,
            variant_ids: None,
            category_id: None,
        },
    );
    assert_eq!(code(again), ErrorCode::Conflict);
    let rice_line = count
        .lines
        .iter()
        .find(|l| l.variant_id == rice)
        .unwrap()
        .id
        .clone();
    let counted = stock::enter_counts(
        &mut ctx,
        &CountEntriesInput {
            count_id: count.id.clone(),
            entries: vec![CountEntry {
                line_id: rice_line,
                counted_qty: Some(9000),
            }],
        },
    )
    .unwrap();
    assert_eq!(
        (counted.counted_lines, counted.total_variance_value),
        (1, -40000)
    );
    let done = stock::finalize_count(
        &mut ctx,
        &CountFinalizeInput {
            count_id: count.id.clone(),
            idempotency_key: "cnt-final-001".into(),
        },
    )
    .unwrap();
    assert_eq!(done.status, "finalized");
    assert_eq!((kit.stock(&rice), kit.stock(&dal)), (9000, 5000));
    kit.assert_books_balance();
}

#[test]
fn list_pages_are_bounded() {
    let kit = Kit::new();
    for i in 0..5 {
        kit.product(&format!("Item {i}"), &format!("SKU-{i}"), 100, 50, 0, None);
    }
    let owner = kit.owner();
    let mut conn = kit.conn();
    let ctx = kit.ctx(&mut conn, &owner);
    let page = catalog::list_products(
        &ctx,
        &ProductListQuery {
            page: PageQuery {
                page: Some(2),
                page_size: Some(2),
                sort: Some("sku".into()),
                descending: true,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        (page.total, page.items.len(), page.items[0].sku.as_str()),
        (5, 2, "SKU-2")
    );
}
