//! Schema tests: the migration runner and the invariants the database enforces on its own
//! (immutability, ledgers, caches, allocation rules). Services rely on these in Phase 3.

use rusqlite::{params, Connection};

use super::migrate::{self, Migration};
use super::{open, open_in_memory, DbError};

const NOW: &str = "2026-10-17T10:00:00Z";
const TODAY: &str = "2026-10-17";
const CASH_ACCOUNT: &str = "01920000-0000-7000-8001-000000001000";
const OWNER_ROLE: &str = "01920000-0000-7000-8000-000000000001";

fn migrated() -> Connection {
    let mut conn = open_in_memory().unwrap();
    migrate::run(&mut conn).unwrap();
    conn
}

fn err_text(result: rusqlite::Result<usize>) -> String {
    match result {
        Ok(n) => panic!("expected an error, statement changed {n} row(s)"),
        Err(e) => e.to_string(),
    }
}

/// Minimal master data: branch, warehouse, unit, one product with one variant, a user and a
/// customer.
fn seed_basics(conn: &Connection) {
    conn.execute_batch(&format!(
        "INSERT INTO users (id, name, email, password_hash, role_id, created_at, updated_at)
           VALUES ('u1', 'Owner', 'owner@shop.in', 'x', '{OWNER_ROLE}', '{NOW}', '{NOW}');
         INSERT INTO branches (id, code, name, is_default, created_at, updated_at)
           VALUES ('b1', 'MAIN', 'Main', 1, '{NOW}', '{NOW}');
         INSERT INTO warehouses (id, branch_id, code, name, is_default, created_at, updated_at)
           VALUES ('w1', 'b1', 'WH1', 'Shop floor', 1, '{NOW}', '{NOW}');
         INSERT INTO units (id, code, name, decimals, created_at, updated_at)
           VALUES ('pcs', 'pcs', 'Pieces', 0, '{NOW}', '{NOW}');
         INSERT INTO products (id, name, base_unit_id, created_at, updated_at)
           VALUES ('p1', 'Basmati Rice 5kg', 'pcs', '{NOW}', '{NOW}');
         INSERT INTO product_variants (id, product_id, sku, barcode, sale_price, cost_price, created_at, updated_at)
           VALUES ('v1', 'p1', 'RICE-5', '8901234567890', 49900, 42000, '{NOW}', '{NOW}');
         INSERT INTO parties (id, party_type, name, created_at, updated_at)
           VALUES ('c1', 'customer', 'Ravi Traders', '{NOW}', '{NOW}');"
    ))
    .unwrap();
}

fn insert_movement(
    conn: &Connection,
    id: &str,
    kind: &str,
    qty: i64,
    cost: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO stock_movements (id, business_date, occurred_at, warehouse_id, variant_id,
            movement_type, qty, unit_cost, value, source_type, source_id, created_at)
         VALUES (?1, ?2, ?3, 'w1', 'v1', ?4, ?5, ?6, ?7, 'test', 'src', ?3)",
        params![id, TODAY, NOW, kind, qty, cost, qty * cost / 1000],
    )
}

/// A finalized invoice for 2 × ₹100 + 18% GST = ₹236.00.
fn finalized_invoice(conn: &Connection, id: &str, doc_no: &str) {
    conn.execute(
        "INSERT INTO sales_invoices (id, branch_id, warehouse_id, doc_no, business_date, status,
            party_id, customer_name, gross, taxable, tax_total, total, created_at, updated_at)
         VALUES (?1, 'b1', 'w1', ?2, ?3, 'draft', 'c1', 'Ravi Traders', 20000, 20000, 3600, 23600, ?4, ?4)",
        params![id, doc_no, TODAY, NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sales_invoice_items (id, invoice_id, line_no, variant_id, unit_id, item_name,
            qty, base_qty, unit_price, taxable, tax_rate, cgst, sgst, tax_total, line_total, created_at, updated_at)
         VALUES (?1, ?2, 1, 'v1', 'pcs', 'Basmati Rice 5kg', 2000, 2000, 10000, 20000, 180000, 1800, 1800, 3600, 23600, ?3, ?3)",
        params![format!("{id}-l1"), id, NOW],
    )
    .unwrap();
    conn.execute(
        "UPDATE sales_invoices SET status = 'finalized', finalized_at = ?2 WHERE id = ?1",
        params![id, NOW],
    )
    .unwrap();
}

fn payment(conn: &Connection, id: &str, direction: &str, amount: i64) {
    conn.execute(
        "INSERT INTO payments (id, branch_id, doc_no, business_date, direction, party_id, account_id,
            mode, amount, created_at, updated_at)
         VALUES (?1, 'b1', ?1, ?2, ?3, 'c1', ?4, 'cash', ?5, ?6, ?6)",
        params![id, TODAY, direction, CASH_ACCOUNT, amount, NOW],
    )
    .unwrap();
}

fn allocate(
    conn: &Connection,
    id: &str,
    payment_id: &str,
    target_type: &str,
    target_id: &str,
    amount: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO payment_allocations (id, payment_id, target_type, target_id, amount, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, payment_id, target_type, target_id, amount, NOW],
    )
}

fn amount_paid(conn: &Connection, invoice_id: &str) -> (i64, String) {
    conn.query_row(
        "SELECT amount_paid, payment_status FROM sales_invoices WHERE id = ?1",
        [invoice_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

// ---------------------------------------------------------------- migration runner

#[test]
fn migrates_a_fresh_database_and_is_idempotent() {
    let mut conn = open_in_memory().unwrap();
    let first = migrate::run(&mut conn).unwrap();
    assert_eq!(first.applied, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!(first.version, migrate::latest_version());

    let second = migrate::run(&mut conn).unwrap();
    assert!(second.applied.is_empty());
    assert_eq!(second.version, migrate::latest_version());
}

#[test]
fn detects_an_edited_migration() {
    let mut conn = open_in_memory().unwrap();
    let original = [Migration {
        version: 1,
        name: "one",
        sql: "CREATE TABLE t (x INTEGER) STRICT;",
    }];
    migrate::run_migrations(&mut conn, &original).unwrap();

    let edited = [Migration {
        version: 1,
        name: "one",
        sql: "CREATE TABLE t (y INTEGER) STRICT;",
    }];
    let err = migrate::run_migrations(&mut conn, &edited).unwrap_err();
    assert!(
        matches!(err, DbError::ChecksumMismatch { version: 1, .. }),
        "{err}"
    );
}

#[test]
fn line_endings_do_not_change_the_checksum() {
    assert_eq!(
        migrate::checksum("a;\nb;\n"),
        migrate::checksum("a;\r\nb;\r\n")
    );
}

#[test]
fn refuses_a_database_from_a_newer_app() {
    let mut conn = migrated();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at, duration_ms)
         VALUES (99, 'future', 'x', ?1, 0)",
        [NOW],
    )
    .unwrap();
    let err = migrate::run(&mut conn).unwrap_err();
    assert!(
        matches!(err, DbError::NewerThanApp { found: 99, .. }),
        "{err}"
    );
}

#[test]
fn a_failed_migration_rolls_back_completely() {
    let mut conn = open_in_memory().unwrap();
    let broken = [Migration {
        version: 1,
        name: "broken",
        sql: "CREATE TABLE ok_table (x INTEGER) STRICT; THIS IS NOT SQL;",
    }];
    assert!(migrate::run_migrations(&mut conn, &broken).is_err());
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'ok_table'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    assert_eq!(migrate::current_version(&conn).unwrap(), 0);
}

#[test]
fn file_databases_use_wal_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("v2.db")).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    assert_eq!(fk, 1);
}

#[test]
fn the_printing_migration_applies_to_a_database_in_use() {
    // A database at version 9 with stock and a finalized invoice, then 0010 on top.
    let mut conn = crate::db::open_in_memory().unwrap();
    migrate::run_migrations(&mut conn, &migrate::MIGRATIONS[..9]).unwrap();
    seed_basics(&conn);
    insert_movement(&conn, "m1", "opening", 10_000, 42_000).unwrap();
    finalized_invoice(&conn, "i1", "INV/26-27/00001");
    let outcome = migrate::run(&mut conn).unwrap();
    assert_eq!(outcome.applied, vec![10, 11, 12]);
    assert_eq!(outcome.version, 12);

    let (qty, invoices): (i64, i64) = conn
        .query_row(
            "SELECT (SELECT qty FROM stock_levels WHERE variant_id = 'v1'), (SELECT count(*) FROM sales_invoices)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((qty, invoices), (10_000, 1), "existing data is untouched");
    let templates: i64 = conn
        .query_row("SELECT count(*) FROM label_templates", [], |r| r.get(0))
        .unwrap();
    assert_eq!(templates, 2);
    let grants: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.code || ':' || rp.permission_code FROM role_permissions rp JOIN roles r ON r.id = rp.role_id
                 WHERE rp.permission_code IN ('printing.manage', 'labels.print', 'sales.open_drawer') ORDER BY 1",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(
        grants,
        [
            "manager:labels.print",
            "manager:printing.manage",
            "manager:sales.open_drawer",
            "owner:labels.print",
            "owner:printing.manage",
            "owner:sales.open_drawer",
            "stock_keeper:labels.print",
        ]
    );
    // A printed job needs its time; the status list is enforced.
    let bad = conn.execute(
        "INSERT INTO print_jobs (id, printer_id, printer_name, title, doc_type, data, status, created_at, updated_at)
         VALUES ('j1', 'p', 'P', 'T', 'receipt', x'00', 'printed', '2026-10-22', '2026-10-22')",
        [],
    );
    assert!(err_text(bad).contains("CHECK"));
}

#[test]
fn the_inventory_depth_migration_keeps_weighted_average_history() {
    // A version-10 shop on weighted average: two receipts at different costs, a sale out of
    // stock and a price list with a slab, then 0011 on top.
    let mut conn = crate::db::open_in_memory().unwrap();
    migrate::run_migrations(&mut conn, &migrate::MIGRATIONS[..10]).unwrap();
    seed_basics(&conn);
    insert_movement(&conn, "m1", "opening", 10_000, 40_000).unwrap();
    insert_movement(&conn, "m2", "purchase", 10_000, 50_000).unwrap();
    insert_movement(&conn, "m3", "sale", -4_000, 45_000).unwrap();
    finalized_invoice(&conn, "i1", "INV/26-27/00001");
    conn.execute_batch(&format!(
        "INSERT INTO price_lists (id, name, created_at, updated_at) VALUES ('pl1', 'Wholesale', '{NOW}', '{NOW}');
         INSERT INTO price_list_items (id, price_list_id, variant_id, min_qty, price, created_at, updated_at)
           VALUES ('pi1', 'pl1', 'v1', 0, 9000, '{NOW}', '{NOW}'), ('pi2', 'pl1', 'v1', 10000, 8500, '{NOW}', '{NOW}');"
    ))
    .unwrap();
    let outcome = migrate::run(&mut conn).unwrap();
    assert_eq!(outcome.applied, vec![11, 12]);

    let (qty, value): (i64, i64) = conn
        .query_row(
            "SELECT sum(qty), sum(value) FROM stock_movements WHERE variant_id = 'v1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (qty, value),
        (16_000, 720_000),
        "the stock history is untouched"
    );
    let level: i64 = conn
        .query_row(
            "SELECT qty FROM stock_levels WHERE variant_id = 'v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(level, 16_000);
    // Weighted average stays in force: no layers, nothing to reconcile, old sales keep no cost value.
    let (layers, mismatches, costed): (i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM cost_layers), (SELECT count(*) FROM v_cost_layer_mismatches),
                    (SELECT count(*) FROM sales_invoice_items WHERE cost_value IS NOT NULL)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((layers, mismatches, costed), (0, 0, 0));
    // Prices survive the rebuilt unique index, valid always, and the list is a custom list.
    let prices: Vec<(i64, i64, Option<String>, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT i.min_qty, i.price, i.valid_from, l.kind FROM price_list_items i
                 JOIN price_lists l ON l.id = i.price_list_id ORDER BY i.min_qty",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(
        prices,
        [
            (0, 9_000, None, "custom".to_owned()),
            (10_000, 8_500, None, "custom".to_owned())
        ]
    );
    // The same slab from another date is allowed; the same slab and date twice is not.
    conn.execute(
        &format!("INSERT INTO price_list_items (id, price_list_id, variant_id, min_qty, price, valid_from, created_at, updated_at)
                  VALUES ('pi3', 'pl1', 'v1', 0, 8800, '2026-11-01', '{NOW}', '{NOW}')"),
        [],
    )
    .unwrap();
    let twice = conn.execute(
        &format!("INSERT INTO price_list_items (id, price_list_id, variant_id, min_qty, price, valid_from, created_at, updated_at)
                  VALUES ('pi4', 'pl1', 'v1', 0, 8700, '2026-11-01', '{NOW}', '{NOW}')"),
        [],
    );
    assert!(err_text(twice).contains("UNIQUE"));
    let grants: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.code || ':' || rp.permission_code FROM role_permissions rp JOIN roles r ON r.id = rp.role_id
                 WHERE rp.permission_code IN ('price_lists.manage', 'inventory.valuation') ORDER BY 1",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(
        grants,
        [
            "manager:price_lists.manage",
            "owner:inventory.valuation",
            "owner:price_lists.manage"
        ]
    );
}

#[test]
fn the_accounting_depth_migration_keeps_closed_days_and_guards_journals() {
    // A version-11 shop with a closed day and a finalized invoice, then 0012 on top.
    let mut conn = crate::db::open_in_memory().unwrap();
    migrate::run_migrations(&mut conn, &migrate::MIGRATIONS[..11]).unwrap();
    seed_basics(&conn);
    finalized_invoice(&conn, "i1", "INV/26-27/00001");
    conn.execute_batch(&format!(
        "INSERT INTO day_closes (id, branch_id, business_date, closed_at, closed_by, summary_json, created_at, updated_at)
         VALUES ('d1', 'b1', '{TODAY}', '{NOW}', 'u1', '{{}}', '{NOW}', '{NOW}');"
    ))
    .unwrap();
    let outcome = migrate::run(&mut conn).unwrap();
    assert_eq!(outcome.applied, vec![12]);

    let (status, invoices): (String, i64) = conn
        .query_row(
            "SELECT (SELECT status FROM day_closes WHERE id = 'd1'), (SELECT count(*) FROM sales_invoices)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (status.as_str(), invoices),
        ("closed", 1),
        "the closed day stays closed"
    );
    let retained: String = conn
        .query_row(
            "SELECT name FROM accounts WHERE code = '3100' AND account_type = 'equity'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, "Retained Earnings");

    // A draft journal's lines can change; once posted, neither the journal nor its lines can.
    conn.execute_batch(&format!(
        "INSERT INTO manual_journals (id, doc_no, number_period, doc_seq, business_date, narration, created_at, updated_at)
           VALUES ('j1', 'JV/26-27/00001', '26-27', 1, '{TODAY}', 'Fees', '{NOW}', '{NOW}');
         INSERT INTO manual_journal_lines (id, journal_id, line_no, account_id, debit, credit, created_at)
           VALUES ('l1', 'j1', 1, '{CASH_ACCOUNT}', 100, 0, '{NOW}');
         DELETE FROM manual_journal_lines WHERE id = 'l1';"
    ))
    .unwrap();
    let posted_without_entry = conn.execute(
        "UPDATE manual_journals SET status = 'posted', approved_by = 'u1' WHERE id = 'j1'",
        [],
    );
    assert!(err_text(posted_without_entry).contains("CHECK"));
    conn.execute_batch(&format!(
        "INSERT INTO journal_entries (id, business_date, source_type, source_id, narration, created_at)
           VALUES ('e1', '{TODAY}', 'manual_journal', 'j1', 'Fees', '{NOW}');
         UPDATE manual_journals SET status = 'posted', approved_by = 'u1', journal_entry_id = 'e1' WHERE id = 'j1';"
    ))
    .unwrap();
    let edit = conn.execute(
        "UPDATE manual_journals SET narration = 'x' WHERE id = 'j1'",
        [],
    );
    assert!(err_text(edit).contains("DOCUMENT_FINALIZED"));
    let add_line = conn.execute(
        &format!(
            "INSERT INTO manual_journal_lines (id, journal_id, line_no, account_id, debit, credit, created_at)
             VALUES ('l2', 'j1', 1, '{CASH_ACCOUNT}', 100, 0, '{NOW}')"
        ),
        [],
    );
    assert!(err_text(add_line).contains("DOCUMENT_FINALIZED"));
    let delete = conn.execute("DELETE FROM manual_journals WHERE id = 'j1'", []);
    assert!(err_text(delete).contains("DOCUMENT_FINALIZED"));
    // A finished bank reconciliation is history: append-only.
    conn.execute_batch(&format!(
        "INSERT INTO bank_reconciliations (id, account_id, statement_date, statement_balance, book_balance, uncleared_in, uncleared_out, difference, created_at)
         VALUES ('r1', '{CASH_ACCOUNT}', '{TODAY}', 0, 0, 0, 0, 0, '{NOW}');"
    ))
    .unwrap();
    let change = conn.execute(
        "UPDATE bank_reconciliations SET difference = 1 WHERE id = 'r1'",
        [],
    );
    assert!(err_text(change).contains("APPEND_ONLY"));
    let grants: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.code || ':' || rp.permission_code FROM role_permissions rp JOIN roles r ON r.id = rp.role_id
                 WHERE rp.permission_code IN ('journal.approve', 'period.override', 'period.manage', 'audit.export') ORDER BY 1",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(
        grants,
        [
            "accountant:audit.export",
            "manager:period.override",
            "owner:audit.export",
            "owner:journal.approve",
            "owner:period.manage",
            "owner:period.override",
        ]
    );
}

// ---------------------------------------------------------------- seed data

#[test]
fn seeded_permissions_match_the_typescript_constants() {
    let ts = include_str!("../../../packages/shared/src/constants.ts");
    let block = &ts
        [ts.find("export const PERMISSIONS").unwrap()..ts.find("export type Permission ").unwrap()];
    let mut expected: Vec<&str> = block
        .split('"')
        .filter(|s| s.contains('.') && !s.contains(' '))
        .collect();
    expected.sort_unstable();

    let conn = migrated();
    let mut stmt = conn
        .prepare("SELECT code FROM permissions ORDER BY code")
        .unwrap();
    let actual: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(actual, expected);

    let owner_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM role_permissions WHERE role_id = ?1",
            [OWNER_ROLE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(usize::try_from(owner_count).unwrap(), expected.len());
}

#[test]
fn seeds_system_accounts_and_roles() {
    let conn = migrated();
    let roles: i64 = conn
        .query_row("SELECT count(*) FROM roles WHERE is_system = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let cash: String = conn
        .query_row(
            "SELECT subtype FROM accounts WHERE code = '1000'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(roles, 5);
    assert_eq!(cash, "cash");
}

// ---------------------------------------------------------------- stock ledger

#[test]
fn stock_levels_follow_the_ledger() {
    let conn = migrated();
    seed_basics(&conn);
    insert_movement(&conn, "m1", "opening", 10_000, 42_000).unwrap();
    insert_movement(&conn, "m2", "sale", -2_000, 42_000).unwrap();

    let (qty, value): (i64, i64) = conn
        .query_row(
            "SELECT qty, value FROM stock_levels WHERE variant_id = 'v1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((qty, value), (8_000, 336_000));

    let mismatches: i64 = conn
        .query_row("SELECT count(*) FROM v_stock_level_mismatches", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mismatches, 0);
}

#[test]
fn stock_movements_are_append_only_and_signed_by_type() {
    let conn = migrated();
    seed_basics(&conn);
    insert_movement(&conn, "m1", "opening", 1_000, 100).unwrap();

    assert!(
        err_text(conn.execute("UPDATE stock_movements SET qty = 5 WHERE id = 'm1'", []))
            .contains("APPEND_ONLY")
    );
    assert!(
        err_text(conn.execute("DELETE FROM stock_movements WHERE id = 'm1'", []))
            .contains("APPEND_ONLY")
    );
    // A sale must reduce stock; a purchase must add it.
    assert!(err_text(insert_movement(&conn, "m2", "sale", 1_000, 100)).contains("CHECK"));
    assert!(err_text(insert_movement(&conn, "m3", "purchase", -1_000, 100)).contains("CHECK"));
}

#[test]
fn a_tampered_stock_cache_is_detected() {
    let conn = migrated();
    seed_basics(&conn);
    insert_movement(&conn, "m1", "opening", 5_000, 100).unwrap();
    conn.execute("UPDATE stock_levels SET qty = 4000", [])
        .unwrap();
    let mismatches: i64 = conn
        .query_row("SELECT count(*) FROM v_stock_level_mismatches", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mismatches, 1);
}

// ---------------------------------------------------------------- catalogue

#[test]
fn skus_are_unique_among_live_products_only() {
    let conn = migrated();
    seed_basics(&conn);
    let insert = |id: &str| {
        conn.execute(
            "INSERT INTO product_variants (id, product_id, sku, created_at, updated_at) VALUES (?1, 'p1', 'rice-5', ?2, ?2)",
            params![id, NOW],
        )
    };
    assert!(err_text(insert("v2")).contains("UNIQUE"));
    conn.execute(
        "UPDATE product_variants SET deleted_at = ?1 WHERE id = 'v1'",
        [NOW],
    )
    .unwrap();
    insert("v2").unwrap();
}

#[test]
fn product_search_finds_prefixes_and_tracks_renames() {
    let conn = migrated();
    seed_basics(&conn);
    let search = |q: &str| -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT variant_id FROM product_search WHERE product_search MATCH ?1")
            .unwrap();
        stmt.query_map([q], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(search("basm*"), vec!["v1"]);
    assert_eq!(search("sku:RICE*"), vec!["v1"]);
    conn.execute(
        "UPDATE products SET name = 'Sona Masoori 5kg' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    assert!(search("basm*").is_empty());
    assert_eq!(search("sona*"), vec!["v1"]);
}

// ---------------------------------------------------------------- invoices

#[test]
fn finalized_invoices_are_immutable() {
    let conn = migrated();
    seed_basics(&conn);
    finalized_invoice(&conn, "i1", "INV/26-27/000001");

    let locked = |sql: &str| {
        assert!(
            err_text(conn.execute(sql, [])).contains("DOCUMENT_FINALIZED"),
            "{sql}"
        )
    };
    locked("UPDATE sales_invoices SET total = 1, taxable = 1, tax_total = 0 WHERE id = 'i1'");
    locked("UPDATE sales_invoices SET customer_name = 'Someone else' WHERE id = 'i1'");
    locked("UPDATE sales_invoice_items SET qty = 1000 WHERE invoice_id = 'i1'");
    locked("DELETE FROM sales_invoice_items WHERE invoice_id = 'i1'");
    locked("DELETE FROM sales_invoices WHERE id = 'i1'");
    let add_line = conn.execute(
        "INSERT INTO sales_invoice_items (id, invoice_id, line_no, variant_id, unit_id, item_name, qty, base_qty,
            unit_price, taxable, line_total, created_at, updated_at)
         VALUES ('x', 'i1', 2, 'v1', 'pcs', 'Extra', 1000, 1000, 100, 100, 100, ?1, ?1)",
        [NOW],
    );
    assert!(err_text(add_line).contains("DOCUMENT_FINALIZED"));

    // Voiding is the only allowed change, and it can't be undone.
    conn.execute(
        "UPDATE sales_invoices SET status = 'voided', voided_at = ?1, void_reason = 'test' WHERE id = 'i1'",
        [NOW],
    )
    .unwrap();
    assert!(err_text(conn.execute(
        "UPDATE sales_invoices SET status = 'finalized' WHERE id = 'i1'",
        []
    ))
    .contains("INVALID_STATUS"));
}

#[test]
fn invoice_totals_and_numbers_are_checked() {
    let conn = migrated();
    seed_basics(&conn);
    finalized_invoice(&conn, "i1", "INV-1");
    let duplicate = conn.execute(
        "INSERT INTO sales_invoices (id, branch_id, warehouse_id, doc_no, business_date, status, customer_name, created_at, updated_at)
         VALUES ('i2', 'b1', 'w1', 'INV-1', ?1, 'draft', 'Walk-in', ?2, ?2)",
        params![TODAY, NOW],
    );
    assert!(err_text(duplicate).contains("UNIQUE"));

    let wrong_total = conn.execute(
        "INSERT INTO sales_invoices (id, branch_id, warehouse_id, business_date, customer_name, taxable, tax_total, total, created_at, updated_at)
         VALUES ('i3', 'b1', 'w1', ?1, 'Walk-in', 100, 18, 120, ?2, ?2)",
        params![TODAY, NOW],
    );
    assert!(err_text(wrong_total).contains("CHECK"));

    let unnumbered_final = conn.execute(
        "INSERT INTO sales_invoices (id, branch_id, warehouse_id, business_date, status, customer_name, created_at, updated_at)
         VALUES ('i4', 'b1', 'w1', ?1, 'finalized', 'Walk-in', ?2, ?2)",
        params![TODAY, NOW],
    );
    assert!(err_text(unnumbered_final).contains("CHECK"));
}

// ---------------------------------------------------------------- payments

#[test]
fn allocations_keep_invoice_paid_amounts_in_step() {
    let conn = migrated();
    seed_basics(&conn);
    finalized_invoice(&conn, "i1", "INV-1");
    payment(&conn, "pay1", "in", 10_000);
    allocate(&conn, "a1", "pay1", "sales_invoice", "i1", 10_000).unwrap();
    assert_eq!(amount_paid(&conn, "i1"), (10_000, "partial".into()));

    payment(&conn, "pay2", "in", 13_600);
    allocate(&conn, "a2", "pay2", "sales_invoice", "i1", 13_600).unwrap();
    assert_eq!(amount_paid(&conn, "i1"), (23_600, "paid".into()));

    // Voiding a payment gives the amount back to the invoice.
    conn.execute(
        "UPDATE payments SET status = 'voided', voided_at = ?1 WHERE id = 'pay1'",
        [NOW],
    )
    .unwrap();
    assert_eq!(amount_paid(&conn, "i1"), (13_600, "partial".into()));
}

#[test]
fn allocations_are_validated() {
    let conn = migrated();
    seed_basics(&conn);
    finalized_invoice(&conn, "i1", "INV-1");
    payment(&conn, "small", "in", 5_000);
    payment(&conn, "out", "out", 5_000);
    payment(&conn, "big", "in", 30_000);

    // More than the payment.
    assert!(
        err_text(allocate(&conn, "a1", "small", "sales_invoice", "i1", 6_000))
            .contains("exceed the payment")
    );
    // Money going out can't pay a sales invoice.
    assert!(
        err_text(allocate(&conn, "a2", "out", "sales_invoice", "i1", 1_000))
            .contains("wrong direction")
    );
    // Unknown target.
    assert!(err_text(allocate(
        &conn,
        "a3",
        "small",
        "sales_invoice",
        "nope",
        1_000
    ))
    .contains("not found"));
    // More than the invoice total (amount_paid CHECK).
    assert!(
        err_text(allocate(&conn, "a4", "big", "sales_invoice", "i1", 30_000)).contains("CHECK")
    );
    // Payments can't be edited or deleted.
    assert!(
        err_text(conn.execute("UPDATE payments SET amount = 1 WHERE id = 'small'", []))
            .contains("DOCUMENT_FINALIZED")
    );
    assert!(
        err_text(conn.execute("DELETE FROM payments WHERE id = 'small'", []))
            .contains("DOCUMENT_FINALIZED")
    );
}

// ---------------------------------------------------------------- ledgers

#[test]
fn party_balance_follows_the_party_ledger() {
    let conn = migrated();
    seed_basics(&conn);
    let post = |id: &str, debit: i64, credit: i64| {
        conn.execute(
            "INSERT INTO party_ledger (id, party_id, business_date, entry_type, source_type, source_id, debit, credit, created_at)
             VALUES (?1, 'c1', ?2, 'adjustment', 'test', ?1, ?3, ?4, ?5)",
            params![id, TODAY, debit, credit, NOW],
        )
    };
    post("l1", 23_600, 0).unwrap();
    post("l2", 0, 10_000).unwrap();
    let balance: i64 = conn
        .query_row("SELECT balance FROM parties WHERE id = 'c1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(balance, 13_600);
    assert!(err_text(post("l3", 100, 100)).contains("CHECK"));
    assert!(
        err_text(conn.execute("DELETE FROM party_ledger WHERE id = 'l1'", []))
            .contains("APPEND_ONLY")
    );
    let mismatches: i64 = conn
        .query_row("SELECT count(*) FROM v_party_balance_mismatches", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mismatches, 0);
}

#[test]
fn journal_is_append_only_and_imbalance_is_visible() {
    let conn = migrated();
    conn.execute_batch(&format!(
        "INSERT INTO journal_entries (id, business_date, source_type, narration, created_at)
           VALUES ('j1', '{TODAY}', 'test', 'Balanced', '{NOW}'), ('j2', '{TODAY}', 'test', 'Unbalanced', '{NOW}');
         INSERT INTO journal_lines (id, entry_id, line_no, account_id, debit, credit, created_at) VALUES
           ('l1', 'j1', 1, '{CASH_ACCOUNT}', 500, 0, '{NOW}'),
           ('l2', 'j1', 2, '01920000-0000-7000-8001-000000004000', 0, 500, '{NOW}'),
           ('l3', 'j2', 1, '{CASH_ACCOUNT}', 500, 0, '{NOW}');"
    ))
    .unwrap();
    let unbalanced: Vec<String> = conn
        .prepare("SELECT entry_id FROM v_unbalanced_journal_entries")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(unbalanced, vec!["j2"]);
    assert!(
        err_text(conn.execute("UPDATE journal_lines SET debit = 1 WHERE id = 'l1'", []))
            .contains("APPEND_ONLY")
    );
    assert!(err_text(conn.execute(
        "UPDATE journal_entries SET narration = 'x' WHERE id = 'j1'",
        []
    ))
    .contains("APPEND_ONLY"));
    conn.execute(
        "UPDATE journal_entries SET status = 'reversed' WHERE id = 'j1'",
        [],
    )
    .unwrap();
}

#[test]
fn audit_log_is_append_only() {
    let conn = migrated();
    conn.execute(
        "INSERT INTO audit_log (id, occurred_at, action, entity_type) VALUES ('a1', ?1, 'LOGIN', 'users')",
        [NOW],
    )
    .unwrap();
    assert!(
        err_text(conn.execute("UPDATE audit_log SET action = 'X'", [])).contains("APPEND_ONLY")
    );
    assert!(err_text(conn.execute("DELETE FROM audit_log", [])).contains("APPEND_ONLY"));
}

#[test]
fn money_columns_reject_floats() {
    let conn = migrated();
    seed_basics(&conn);
    let float_price = conn.execute(
        "INSERT INTO product_variants (id, product_id, sku, sale_price, created_at, updated_at) VALUES ('v9', 'p1', 'X', 49.99, ?1, ?1)",
        [NOW],
    );
    assert!(err_text(float_price)
        .to_lowercase()
        .contains("cannot store real value"));
}
