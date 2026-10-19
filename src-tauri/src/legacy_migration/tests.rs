//! End-to-end tests: build a v1 database with the real v1 code (its migrations and
//! repositories, bugs included), migrate it, and check that nothing was lost and every balance
//! still adds up.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use super::{migrate_legacy, MigrateError, MigrateOptions};
use crate::models::InvoiceItem;
use crate::repositories as v1;

struct Fixture {
    _dir: tempfile::TempDir,
    legacy: PathBuf,
    target: PathBuf,
    backups: PathBuf,
}

impl Fixture {
    fn options(&self, dry_run: bool) -> MigrateOptions {
        MigrateOptions {
            legacy_path: self.legacy.clone(),
            target_path: self.target.clone(),
            backup_dir: self.backups.clone(),
            dry_run,
            currency_minor_digits: 2,
        }
    }
}

fn line(product_id: &str, quantity: f64, unit_price: f64) -> InvoiceItem {
    InvoiceItem {
        id: String::new(),
        invoice_id: String::new(),
        product_id: product_id.into(),
        product_name: None,
        quantity,
        unit_price,
        total: quantity * unit_price,
        tax_rate: None,
    }
}

fn purchase_line(product_id: &str, quantity: f64, unit_price: f64) -> crate::models::PurchaseItem {
    crate::models::PurchaseItem {
        id: String::new(),
        purchase_id: String::new(),
        product_id: product_id.into(),
        product_name: None,
        quantity,
        unit_price,
        total: quantity * unit_price,
    }
}

#[allow(clippy::too_many_lines)]
fn build_legacy_database(path: &Path) {
    let conn = Connection::open(path).unwrap();
    crate::database::migrations::run_migrations(&conn).unwrap();
    let admin: String = conn
        .query_row("SELECT id FROM users", [], |r| r.get(0))
        .unwrap();
    let actor = Some(admin.as_str());
    let gst: String = conn
        .query_row("SELECT id FROM tax_rates", [], |r| r.get(0))
        .unwrap();

    let grocery = v1::create_category(&conn, "Grocery", actor).unwrap();
    v1::create_category(&conn, "grocery", actor).unwrap(); // only differs by case
    let brand = v1::create_brand(&conn, "Acme", actor).unwrap();

    let rice = v1::create_product(
        &conn,
        "Basmati Rice 5kg",
        "RICE-5",
        Some("8901234567890"),
        None,
        Some(&grocery.id),
        Some(&brand.id),
        420.0,
        499.0,
        10.0,
        2.0,
        0,
        None,
        "pcs",
        None,
        None,
        Some(&gst),
        None,
        Some(449.0),
        actor,
    )
    .unwrap();
    let dal = v1::create_product(
        &conn,
        "Toor Dal",
        "DAL-1",
        None,
        None,
        None,
        None,
        33.333_333,
        99.99,
        2.5,
        0.0,
        1,
        Some("2027-03-31"),
        "kg",
        None,
        None,
        None,
        None,
        None,
        actor,
    )
    .unwrap();
    let old = v1::create_product(
        &conn,
        "Discontinued Soap",
        "SOAP-1",
        None,
        None,
        None,
        None,
        10.0,
        20.0,
        0.0,
        0.0,
        0,
        None,
        "pcs",
        None,
        None,
        None,
        None,
        None,
        actor,
    )
    .unwrap();

    let ravi = v1::create_customer(
        &conn,
        "Ravi Traders",
        None,
        Some("9876543210"),
        Some("MG Road"),
        actor,
    )
    .unwrap();
    let walkin = v1::create_customer(&conn, "Walk-in Customer", None, None, None, actor).unwrap();
    let metro = v1::create_supplier(&conn, "Metro Wholesale", None, None, None, actor).unwrap();

    // 1. Normal paid sale: 2 × 449 + 18% tax.
    v1::create_invoice(
        &conn,
        &ravi.id,
        898.0,
        0.0,
        161.64,
        1059.64,
        1059.64,
        Some("CASH"),
        vec![line(&rice.id, 2.0, 449.0)],
        actor,
    )
    .unwrap();
    // 2. Overpaid sale (v1 bug B-4): 150 handed over for 99.99.
    v1::create_invoice(
        &conn,
        &walkin.id,
        99.99,
        0.0,
        0.0,
        99.99,
        150.0,
        Some("CASH"),
        vec![line(&dal.id, 1.0, 99.99)],
        actor,
    )
    .unwrap();
    // 3. Part-paid sale with a bill discount.
    v1::create_invoice(
        &conn,
        &ravi.id,
        449.0,
        49.0,
        72.0,
        472.0,
        100.0,
        Some("UPI"),
        vec![line(&rice.id, 1.0, 449.0)],
        actor,
    )
    .unwrap();
    // 4. Sale that is later cancelled (v1 restocks and clears the due).
    let cancelled = v1::create_invoice(
        &conn,
        &ravi.id,
        449.0,
        0.0,
        0.0,
        449.0,
        0.0,
        Some("CREDIT"),
        vec![line(&rice.id, 1.0, 449.0)],
        actor,
    )
    .unwrap();
    conn.execute(
        "UPDATE invoices SET payment_status = 'Cancel', due_amount = 0 WHERE id = ?1",
        [&cancelled.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE customers SET due_amount = due_amount - 449 WHERE id = ?1",
        [&ravi.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE products SET stock = stock + 1 WHERE id = ?1",
        [&rice.id],
    )
    .unwrap();

    v1::create_payment(
        &conn,
        "RECEIVE",
        &ravi.id,
        200.0,
        "Cash",
        Some("RCPT-9"),
        actor,
    )
    .unwrap();

    // Purchases: one received (stock in, weighted cost), one still on order.
    let po = v1::create_purchase(
        &conn,
        &metro.id,
        2000.0,
        "PENDING",
        vec![purchase_line(&rice.id, 5.0, 400.0)],
        actor,
    )
    .unwrap();
    v1::receive_purchase(&conn, &po.id, actor).unwrap();
    v1::create_purchase(
        &conn,
        &metro.id,
        300.0,
        "PENDING",
        vec![purchase_line(&dal.id, 3.0, 100.0)],
        actor,
    )
    .unwrap();
    v1::create_payment(&conn, "PAY", &metro.id, 500.0, "Bank", None, actor).unwrap();

    v1::create_quotation(
        &conn,
        &ravi.id,
        898.0,
        "PENDING",
        vec![crate::models::QuotationItem {
            id: String::new(),
            quotation_id: String::new(),
            product_id: rice.id.clone(),
            product_name: None,
            quantity: 2.0,
            unit_price: 449.0,
            total: 898.0,
        }],
        actor,
    )
    .unwrap();
    v1::create_expense(
        &conn,
        "Shop rent",
        "Rent",
        5000.0,
        "2026-10-01",
        None,
        actor,
    )
    .unwrap();
    v1::create_lead(
        &conn,
        "Hotel Annapurna",
        Some("Mr. Rao"),
        None,
        None,
        "Proposal",
        25000.0,
        None,
        actor,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO held_orders (id, customer_name, discount, tax_rate, items, created_at) VALUES ('h1', 'Table 4', 0, 18, '[{\"id\":\"x\"}]', ?1)",
        [chrono::Utc::now().to_rfc3339()],
    )
    .unwrap();
    v1::delete_product(&conn, &old.id, actor).unwrap();

    // A manual edit of a customer's due in v1 (audit B-14): v2 must keep what v1 showed.
    v1::update_customer(
        &conn,
        &walkin.id,
        "Walk-in Customer",
        None,
        None,
        None,
        25.0,
        actor,
    )
    .unwrap();
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("inventory_app.db");
    build_legacy_database(&legacy);
    Fixture {
        legacy,
        target: dir.path().join("inventory_v2.db"),
        backups: dir.path().join("backups"),
        _dir: dir,
    }
}

fn sha256(path: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}

fn scalar<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn migrates_everything_and_balances_add_up() {
    let f = fixture();
    let legacy_hash = sha256(&f.legacy);
    let outcome = migrate_legacy(&f.options(false)).unwrap();
    let report = &outcome.report;

    // v1 untouched; the backup's checksum is recorded; report written.
    assert_eq!(sha256(&f.legacy), legacy_hash);
    assert_eq!(report.backup_sha256, sha256(Path::new(&report.backup_path)));
    assert!(outcome.report_path.is_file());
    assert_eq!(report.schema_version, crate::db::migrate::latest_version());

    let v2 = crate::db::open(&f.target).unwrap();
    let v1 = Connection::open(&f.legacy).unwrap();

    // Row counts.
    for (table, expected) in [
        ("invoices", 4),
        ("products", 3),
        ("customers", 2),
        ("suppliers", 1),
        ("purchases", 2),
        ("payments", 2),
        ("expenses", 1),
    ] {
        assert_eq!(report.tables[table].migrated_rows, expected, "{table}");
        assert_eq!(report.tables[table].skipped_rows, 0, "{table}");
    }

    // Stock: every product's v2 stock equals v1's.
    let mut stmt = v1.prepare("SELECT id, stock FROM products").unwrap();
    let stocks: Vec<(String, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for (product_id, stock) in stocks {
        let qty: Option<i64> = v2
            .query_row(
                "SELECT sum(l.qty) FROM stock_levels l JOIN product_variants v ON v.id = l.variant_id WHERE v.product_id = ?1",
                [&product_id],
                |r| r.get(0),
            )
            .unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let expected = (stock * 1000.0).round() as i64;
        assert_eq!(qty.unwrap_or(0), expected, "stock of {product_id}");
    }

    // Party balances equal what v1 showed (customers +due, suppliers -balance).
    let mut stmt = v1
        .prepare(
            "SELECT id, due_amount FROM customers UNION ALL SELECT id, -balance FROM suppliers",
        )
        .unwrap();
    let balances: Vec<(String, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for (party_id, balance) in balances {
        let v2_balance: i64 = v2
            .query_row(
                "SELECT balance FROM parties WHERE id = ?1",
                [&party_id],
                |r| r.get(0),
            )
            .unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let expected = (balance * 100.0).round() as i64;
        assert_eq!(v2_balance, expected, "balance of {party_id}");
    }
    // Only the hand-edited walk-in due differs from the documents; it is carried over and reported.
    assert_eq!(report.balance_adjustments.len(), 1);
    let adjustment = &report.balance_adjustments[0];
    assert_eq!(
        (
            adjustment.name.as_str(),
            adjustment.from_documents_minor,
            adjustment.adjustment_minor
        ),
        ("Walk-in Customer", 0, 2_500)
    );

    // Invoices: lines add up to headers; the cancelled one is voided; the overpayment became change.
    let header_vs_lines: i64 = scalar(
        &v2,
        "SELECT count(*) FROM sales_invoices i
         WHERE i.total <> (SELECT coalesce(sum(line_total), 0) FROM sales_invoice_items WHERE invoice_id = i.id)",
    );
    assert_eq!(header_vs_lines, 0);
    assert_eq!(
        scalar::<i64>(
            &v2,
            "SELECT count(*) FROM sales_invoices WHERE status = 'voided'"
        ),
        1
    );
    let (amount, tendered, change): (i64, i64, i64) = v2
        .query_row(
            "SELECT amount, tendered, change_given FROM payments WHERE change_given > 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((amount, tendered, change), (9_999, 15_000, 5_001));
    let paid_status: Vec<String> = v2
        .prepare(
            "SELECT payment_status FROM sales_invoices WHERE status = 'finalized' ORDER BY doc_no",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(paid_status, vec!["paid", "paid", "partial"]);
    assert_eq!(report.totals.sales_total_minor, 105_964 + 9_999 + 47_200);

    // Migrated invoices are finalized and therefore immutable.
    let edit = v2.execute(
        "UPDATE sales_invoices SET customer_name = 'x' WHERE status = 'finalized'",
        [],
    );
    assert!(edit.unwrap_err().to_string().contains("DOCUMENT_FINALIZED"));

    // Prices: v1 sold at the offer price, so it is the sale price and the list price is MRP.
    let (sale, mrp): (i64, Option<i64>) = v2
        .query_row(
            "SELECT sale_price, mrp FROM product_variants WHERE sku = 'RICE-5'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((sale, mrp), (44_900, Some(49_900)));
    assert!(report
        .lossy_amounts
        .iter()
        .any(|l| l.column == "cost" && (l.legacy_value - 33.333_333).abs() < 1e-9));

    // Expiry-tracked stock went into a batch; the deleted product is soft-deleted.
    assert_eq!(
        scalar::<i64>(
            &v2,
            "SELECT count(*) FROM batches WHERE expiry_date = '2027-03-31'"
        ),
        1
    );
    assert_eq!(
        scalar::<i64>(
            &v2,
            "SELECT count(*) FROM products WHERE deleted_at IS NOT NULL"
        ),
        1
    );

    // Purchases: received → bill (payable), pending → order.
    assert_eq!(
        scalar::<i64>(
            &v2,
            "SELECT count(*) FROM purchase_bills WHERE status = 'finalized'"
        ),
        1
    );
    assert_eq!(
        scalar::<i64>(
            &v2,
            "SELECT count(*) FROM purchase_orders WHERE status = 'open'"
        ),
        1
    );

    // Expense is paid; the default admin must change the v1 password.
    assert_eq!(
        scalar::<String>(
            &v2,
            "SELECT CASE WHEN amount_paid = total THEN 'paid' ELSE 'unpaid' END FROM expenses"
        ),
        "paid"
    );
    assert_eq!(
        report.users_requiring_password_change,
        vec!["admin@example.com"]
    );
    assert_eq!(
        scalar::<i64>(&v2, "SELECT must_change_password FROM users"),
        1
    );

    // Case-only duplicate category was renamed, not dropped.
    assert_eq!(scalar::<i64>(&v2, "SELECT count(*) FROM categories"), 2);
    assert!(report.warnings.iter().any(|w| w.contains("letter case")));

    // Everything from v1 is archived, without password hashes; sessions are not.
    let user_json: String = scalar(
        &v2,
        "SELECT row_json FROM legacy_records WHERE source_table = 'users'",
    );
    assert!(!user_json.contains("password_hash"));
    assert_eq!(
        scalar::<i64>(
            &v2,
            "SELECT count(*) FROM legacy_records WHERE source_table = 'sessions'"
        ),
        0
    );
    let archived_invoices: i64 = scalar(
        &v2,
        "SELECT count(*) FROM legacy_records WHERE source_table = 'invoices'",
    );
    assert_eq!(archived_invoices, 4);

    // Caches agree with ledgers, and v1's journal survived.
    assert_eq!(
        scalar::<i64>(&v2, "SELECT count(*) FROM v_stock_level_mismatches"),
        0
    );
    assert_eq!(
        scalar::<i64>(&v2, "SELECT count(*) FROM v_party_balance_mismatches"),
        0
    );
    let v1_entries: i64 = scalar(&v1, "SELECT count(*) FROM journal_entries");
    assert_eq!(
        scalar::<i64>(&v2, "SELECT count(*) FROM journal_entries"),
        v1_entries
    );
    // v1's overpayment journal debited cash by more than revenue (B-4) and is reported, not "fixed".
    assert_eq!(report.unbalanced_journal_entries.len(), 1);

    // Held bill and lead carried over.
    assert_eq!(
        scalar::<String>(&v2, "SELECT cart_format FROM held_bills"),
        "legacy-v1"
    );
    assert_eq!(scalar::<String>(&v2, "SELECT stage FROM leads"), "proposal");

    // The new database is at the latest schema and re-running migrations is a no-op.
    drop(v2);
    let mut v2 = crate::db::open(&f.target).unwrap();
    assert!(crate::db::migrate::run(&mut v2).unwrap().applied.is_empty());
}

#[test]
fn dry_run_writes_only_the_report() {
    let f = fixture();
    let outcome = migrate_legacy(&f.options(true)).unwrap();
    assert!(outcome.report.dry_run);
    assert!(outcome.report.target_path.is_none());
    assert!(!f.target.exists());
    assert!(outcome.report_path.is_file());
    assert!(Path::new(&outcome.report.backup_path).is_file());
    let leftovers = std::fs::read_dir(&f.backups)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("dry-run")
        })
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn replaces_the_empty_v2_database_the_app_created() {
    let f = fixture();
    {
        let mut conn = crate::db::open(&f.target).unwrap();
        crate::db::migrate::run(&mut conn).unwrap();
    }
    migrate_legacy(&f.options(false)).unwrap();
    let v2 = crate::db::open(&f.target).unwrap();
    assert_eq!(scalar::<i64>(&v2, "SELECT count(*) FROM sales_invoices"), 4);
    // A second run now refuses: the database holds real data.
    let err = migrate_legacy(&f.options(false)).err().unwrap();
    assert!(matches!(err, MigrateError::TargetExists(_)), "{err}");
}

#[test]
fn refuses_to_overwrite_an_existing_v2_database() {
    let f = fixture();
    std::fs::write(&f.target, b"existing").unwrap();
    let err = migrate_legacy(&f.options(false)).err().unwrap();
    assert!(matches!(err, MigrateError::TargetExists(_)), "{err}");
    assert_eq!(std::fs::read(&f.target).unwrap(), b"existing");
}

#[test]
fn reports_a_missing_v1_database() {
    let dir = tempfile::tempdir().unwrap();
    let options = MigrateOptions {
        legacy_path: dir.path().join("nope.db"),
        target_path: dir.path().join("v2.db"),
        backup_dir: dir.path().join("backups"),
        dry_run: false,
        currency_minor_digits: 2,
    };
    assert!(matches!(
        migrate_legacy(&options),
        Err(MigrateError::LegacyNotFound(_))
    ));
}

#[test]
fn a_failed_migration_leaves_no_v2_database() {
    let f = fixture();
    // v1 allowed two live users whose e-mails differ only by case; v2 does not. The migration must
    // stop with an error and clean up rather than keep a partial database.
    let conn = Connection::open(&f.legacy).unwrap();
    conn.execute(
        "INSERT INTO users (id, name, email, password_hash, role, created_at, updated_at, is_deleted)
         SELECT 'dup', name, upper(email), password_hash, role, created_at, updated_at, 0 FROM users LIMIT 1",
        [],
    )
    .unwrap();
    drop(conn);

    let err = migrate_legacy(&f.options(false)).err().unwrap();
    assert!(matches!(err, MigrateError::NeedsAttention(_)), "{err}");
    assert!(err.to_string().contains("admin@example.com"));
    assert!(
        !f.target.exists(),
        "a half-built v2 database must be removed"
    );
    let wal = PathBuf::from(format!("{}-wal", f.target.display()));
    assert!(!wal.exists());
}

#[test]
fn reader_handles_missing_tables_and_columns() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE products (id TEXT, name TEXT, price REAL);
                        INSERT INTO products VALUES ('p1', '  ', 12.5);",
    )
    .unwrap();
    let rows = super::reader::read_table(&conn, "products").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text("name"), None);
    assert!((rows[0].real("price") - 12.5).abs() < f64::EPSILON);
    assert!((rows[0].real("offer_price")).abs() < f64::EPSILON);
    assert!(super::reader::read_table(&conn, "held_orders")
        .unwrap()
        .is_empty());
    let none: Option<i64> = conn
        .query_row("SELECT 1 WHERE 0", params![], |r| r.get(0))
        .optional()
        .unwrap();
    assert!(none.is_none());
}

#[test]
fn first_start_import_checks_first_then_fills_the_open_database() {
    use crate::services::setup::needs_setup;
    use crate::services::upgrade::{import_legacy, ImportLegacyInput};
    let f = fixture();
    let legacy_hash = sha256(&f.legacy);
    // The app has already opened (and migrated) its empty v2 database next to the v1 file.
    let state = crate::app::AppState::open(&f.target, std::sync::Arc::new(crate::app::SystemClock))
        .unwrap();

    let dry = import_legacy(&state, &ImportLegacyInput { dry_run: true }).unwrap();
    assert!(!dry.applied);
    assert!(
        needs_setup(&state.conn().unwrap()).unwrap(),
        "a dry run changes nothing"
    );

    let done = import_legacy(&state, &ImportLegacyInput { dry_run: false }).unwrap();
    assert!(done.applied);
    let conn = state.conn().unwrap();
    assert!(!needs_setup(&conn).unwrap());
    assert_eq!(
        scalar::<i64>(&conn, "SELECT count(*) FROM sales_invoices"),
        4
    );
    assert_eq!(
        scalar::<i64>(
            &conn,
            "SELECT count(*) FROM audit_log WHERE action = 'LEGACY_IMPORTED'"
        ),
        1
    );
    drop(conn);
    assert_eq!(
        sha256(&f.legacy),
        legacy_hash,
        "the v1 file is never written"
    );
    let leftovers = std::fs::read_dir(f.target.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".legacy-import-")
        })
        .count();
    assert_eq!(leftovers, 0, "the temporary database is removed");

    // Once the business exists, importing again is refused.
    let again = import_legacy(&state, &ImportLegacyInput { dry_run: false });
    assert_eq!(again.unwrap_err().code, crate::error::ErrorCode::Conflict);
}
