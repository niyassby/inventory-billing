//! Backups (encrypted and not), the restore drill, retention, and the background job scheduler.

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::error::ErrorCode;
use crate::jobs::{self, Job};
use crate::services::backup::{self, RestoreInput, SetPassphraseInput};
use crate::services::sales::{self, PaymentInput, SaleInput, SaleLineInput};
use crate::services::settings::{self, BackupSettings, UpdateSettingsInput};
use crate::services::testkit::{code, Kit};

fn sell(kit: &Kit, key: &str) {
    let owner = kit.owner();
    let rice: String = kit
        .conn()
        .query_row(
            "SELECT id FROM product_variants WHERE sku = 'RICE-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut conn = kit.conn();
    sales::create_invoice(
        &mut kit.ctx(&mut conn, &owner),
        &SaleInput {
            idempotency_key: key.into(),
            warehouse_id: None,
            party_id: None,
            customer_name: None,
            customer_phone: None,
            customer_gstin: None,
            place_of_supply: None,
            business_date: None,
            prices_include_tax: None,
            lines: vec![SaleLineInput {
                variant_id: rice,
                unit_id: None,
                qty: 1000,
                unit_price: None,
                discount: None,
                batch_id: None,
            }],
            bill_discount: None,
            payments: vec![PaymentInput {
                mode: "cash".into(),
                amount: 11_800,
                tendered: None,
                account_id: None,
                reference: None,
            }],
            expected_total: Some(11_800),
            notes: None,
            source_document_id: None,
            register_session_id: None,
        },
    )
    .unwrap();
}

fn shop() -> Kit {
    let kit = Kit::new();
    let gst = kit.gst_rate(18);
    kit.product("Basmati Rice", "RICE-1", 10_000, 7_000, 10, Some(&gst));
    kit
}

fn invoices(kit: &Kit) -> i64 {
    kit.conn()
        .query_row("SELECT count(*) FROM sales_invoices", [], |r| r.get(0))
        .unwrap()
}

fn backup_path(kit: &Kit, run: &backup::BackupRun) -> PathBuf {
    let settings = settings::load(&kit.conn()).unwrap();
    backup::backup_dir(&kit.state, &settings.backup).join(run.file_name.as_ref().unwrap())
}

fn set_passphrase(kit: &Kit, passphrase: &str) {
    let owner = kit.owner();
    let mut conn = kit.conn();
    backup::set_passphrase(
        &kit.state,
        &mut kit.ctx(&mut conn, &owner),
        &SetPassphraseInput {
            passphrase: passphrase.into(),
        },
    )
    .unwrap();
}

fn restore(
    kit: &Kit,
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> Result<backup::RestoreResult, crate::error::AppError> {
    let owner = kit.owner();
    let mut conn = kit.conn();
    backup::restore(
        &kit.state,
        &mut kit.ctx(&mut conn, &owner),
        &RestoreInput {
            path: path.display().to_string(),
            passphrase: passphrase.map(String::from),
        },
    )
}

#[test]
fn restore_drill_brings_back_exactly_the_backup() {
    let kit = shop();
    sell(&kit, "sale-before-backup");
    set_passphrase(&kit, "correct horse battery staple");
    let run = backup::create(&kit.state, "manual", None).unwrap();
    assert!(run.encrypted);
    let path = backup_path(&kit, &run);
    assert!(path.is_file());

    // Work continues after the backup; then the backup is restored.
    sell(&kit, "sale-after-backup");
    let session_token: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM sessions WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(session_token > 0);
    assert_eq!(invoices(&kit), 2);
    let result = restore(&kit, &path, None).unwrap();
    assert_eq!((result.restored.invoices, result.restored.products), (1, 1));
    assert_eq!(
        invoices(&kit),
        1,
        "the later sale is gone, as in the backup"
    );
    assert!(
        backup_path(&kit, &result.safety_backup).is_file(),
        "a safety copy was made first"
    );

    // Everyone signs in again; the restore is on record; the books still balance.
    let open: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM sessions WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0);
    let audited: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM audit_log WHERE action = 'BACKUP_RESTORED'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audited, 1);
    kit.assert_books_balance();

    // The safety copy undoes the restore.
    restore(&kit, &backup_path(&kit, &result.safety_backup), None).unwrap();
    assert_eq!(invoices(&kit), 2);
    kit.assert_books_balance();
}

#[test]
fn encrypted_backups_need_their_passphrase_and_detect_tampering() {
    let kit = shop();
    set_passphrase(&kit, "correct horse battery staple");
    let run = backup::create(&kit.state, "manual", None).unwrap();
    let path = backup_path(&kit, &run);

    // Another PC (no stored key) needs the passphrase; a wrong one is refused.
    let other = Kit::new();
    let missing = restore(&other, &path, None).unwrap_err();
    assert_eq!(missing.code, ErrorCode::Validation);
    assert_eq!(missing.details.unwrap()["needsPassphrase"], json!(true));
    assert_eq!(
        code(restore(&other, &path, Some("wrong passphrase!!"))),
        ErrorCode::Validation
    );
    restore(&other, &path, Some("correct horse battery staple")).unwrap();
    let products: i64 = other
        .conn()
        .query_row("SELECT count(*) FROM products", [], |r| r.get(0))
        .unwrap();
    assert_eq!(products, 1, "the other PC now has the shop's data");

    // One changed byte anywhere and the file is refused.
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 10;
    bytes[last] ^= 0x55;
    let tampered = path.with_file_name("tampered.invbak");
    fs::write(&tampered, &bytes).unwrap();
    assert_eq!(code(restore(&kit, &tampered, None)), ErrorCode::Validation);
}

#[test]
fn bad_files_are_refused_before_anything_changes() {
    let kit = shop();
    sell(&kit, "sale-000001");
    let dir = tempfile::tempdir().unwrap();
    let not_backup = dir.path().join("notes.txt");
    fs::write(&not_backup, "hello").unwrap();
    assert_eq!(
        code(restore(&kit, &not_backup, None)),
        ErrorCode::Validation
    );
    let fake = dir.path().join("fake.invbak");
    fs::write(&fake, b"INVBAK01\x05\x00\x00\x00{oops").unwrap();
    assert_eq!(code(restore(&kit, &fake, None)), ErrorCode::Validation);
    assert_eq!(
        code(restore(&kit, &dir.path().join("missing.invbak"), None)),
        ErrorCode::Validation
    );
    assert_eq!(invoices(&kit), 1);

    // Without a passphrase, backups are compressed but not encrypted (and still verified).
    let run = backup::create(&kit.state, "manual", None).unwrap();
    assert!(!run.encrypted);
    let owner = kit.owner();
    let mut conn = kit.conn();
    let verified = backup::verify(
        &kit.state,
        &kit.ctx(&mut conn, &owner),
        &RestoreInput {
            path: backup_path(&kit, &run).display().to_string(),
            passphrase: None,
        },
    )
    .unwrap();
    assert_eq!(verified.invoices, 1);
    assert_eq!(
        verified.header.schema_version,
        crate::db::migrate::latest_version()
    );
}

#[test]
fn backup_management_is_for_the_owner() {
    let kit = shop();
    let manager = kit.user("manager");
    let mut conn = kit.conn();
    let denied = backup::status(&kit.state, &kit.ctx(&mut conn, &manager));
    assert_eq!(code(denied), ErrorCode::Forbidden);
    let update = settings::update(
        &mut kit.ctx(&mut conn, &manager),
        &UpdateSettingsInput {
            section: "backup".into(),
            value: json!({ "enabled": false }),
        },
    );
    assert_eq!(code(update), ErrorCode::Forbidden);
}

#[test]
fn retention_keeps_daily_weekly_and_monthly_copies() {
    let settings = BackupSettings {
        keep_daily: 3,
        keep_weekly: 2,
        keep_monthly: 2,
        ..BackupSettings::default()
    };
    let mut names: Vec<String> = Vec::new();
    // Two backups a day for 70 days, plus manual and safety copies.
    let start = chrono::NaiveDate::from_ymd_opt(2026, 8, 9).unwrap();
    for day in 0..70 {
        let date = start + chrono::Duration::days(day);
        for time in ["090000", "210000"] {
            names.push(format!(
                "inventory-{}-{time}-scheduled.invbak",
                date.format("%Y%m%d")
            ));
        }
    }
    names.push("inventory-20260901-120000-manual.invbak".into());
    for day in 1..=7 {
        names.push(format!("inventory-2026100{day}-120000-pre_restore.invbak"));
    }
    names.push("notes.txt".into());
    let prune = backup::files_to_prune(&names, &settings);
    let kept: Vec<&String> = names.iter().filter(|n| !prune.contains(n)).collect();
    let scheduled: Vec<&&String> = kept
        .iter()
        .filter(|n| n.ends_with("-scheduled.invbak"))
        .collect();
    // Newest of each of the last 3 days, 2 ISO weeks and 2 months (overlapping).
    assert!(scheduled.contains(&&&"inventory-20261017-210000-scheduled.invbak".to_string()));
    assert!(scheduled.contains(&&&"inventory-20261015-210000-scheduled.invbak".to_string()));
    assert!(scheduled.contains(&&&"inventory-20261011-210000-scheduled.invbak".to_string()));
    assert!(scheduled.contains(&&&"inventory-20260930-210000-scheduled.invbak".to_string()));
    assert!(!scheduled.contains(&&&"inventory-20261017-090000-scheduled.invbak".to_string()));
    assert_eq!(scheduled.len(), 5);
    assert!(
        kept.contains(&&"inventory-20260901-120000-manual.invbak".to_string()),
        "manual backups stay"
    );
    assert!(
        kept.contains(&&"notes.txt".to_string()),
        "other files are never touched"
    );
    assert_eq!(kept.iter().filter(|n| n.contains("pre_restore")).count(), 5);
}

#[test]
fn jobs_run_when_due_and_not_twice() {
    let kit = shop();
    kit.conn()
        .execute(
            "UPDATE product_variants SET reorder_level = 20000 WHERE sku = 'RICE-1'",
            [],
        )
        .unwrap();
    let ran = jobs::tick(&kit.state).unwrap();
    let names: Vec<Job> = ran.iter().map(|(j, _)| *j).collect();
    assert_eq!(
        names,
        Job::ALL.to_vec(),
        "everything runs on the first start"
    );
    assert!(ran.iter().all(|(_, r)| r.is_ok()), "{ran:?}");
    let backups: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM backup_runs WHERE kind = 'scheduled' AND status = 'ok'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(backups, 1);
    let low: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM notifications WHERE kind = 'low_stock' AND read_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(low, 1);

    // Nothing is due a minute later; four hours later the interval backup runs again.
    kit.clock.advance(chrono::Duration::minutes(1));
    assert!(jobs::tick(&kit.state).unwrap().is_empty());
    kit.clock.advance(chrono::Duration::hours(4));
    let ran = jobs::tick(&kit.state).unwrap();
    assert_eq!(
        ran.iter().map(|(j, _)| *j).collect::<Vec<_>>(),
        vec![Job::Backup, Job::LowStock, Job::Expiry]
    );
    let low_again: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM notifications WHERE kind = 'low_stock'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(low_again, 1, "an open notice is not repeated");

    // Restocked items clear their notice.
    kit.conn()
        .execute(
            "UPDATE product_variants SET reorder_level = 0 WHERE sku = 'RICE-1'",
            [],
        )
        .unwrap();
    kit.clock.advance(chrono::Duration::hours(1));
    jobs::tick(&kit.state).unwrap();
    let open: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM notifications WHERE kind = 'low_stock' AND read_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0);
}

#[test]
fn the_daily_backup_follows_the_configured_time() {
    let kit = shop();
    let mut s = settings::load(&kit.conn()).unwrap();
    s.backup.interval_hours = 0;
    // 11:00 IST now; daily backup at 21:00 IST (15:30 UTC).
    let last = kit.now();
    assert!(!jobs::is_due(
        Job::Backup,
        &s,
        Some(last),
        kit.now() + chrono::Duration::hours(9)
    ));
    assert!(jobs::is_due(
        Job::Backup,
        &s,
        Some(last),
        kit.now() + chrono::Duration::hours(10)
    ));
    s.backup.enabled = false;
    assert!(!jobs::is_due(Job::Backup, &s, None, kit.now()));
}

#[test]
fn a_failed_backup_is_recorded_and_shown() {
    let kit = shop();
    // The backup folder is a file: the backup can't be written.
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-folder");
    fs::write(&blocker, "x").unwrap();
    let owner = kit.owner();
    let mut conn = kit.conn();
    settings::update(
        &mut kit.ctx(&mut conn, &owner),
        &UpdateSettingsInput {
            section: "backup".into(),
            value: json!({ "directory": blocker.display().to_string() }),
        },
    )
    .unwrap();
    drop(conn);
    assert!(backup::create(&kit.state, "scheduled", None).is_err());
    let (failed, notices): (i64, i64) = kit
        .conn()
        .query_row(
            "SELECT (SELECT count(*) FROM backup_runs WHERE status = 'failed'),
                    (SELECT count(*) FROM notifications WHERE kind = 'backup_failed' AND read_at IS NULL)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((failed, notices), (1, 1));
}
