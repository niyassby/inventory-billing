//! Backups and restore (Master Prompt §8, docs/ARCHITECTURE.md §8).
//!
//! A backup is a consistent SQLite online-backup copy, integrity-checked, compressed with zstd and
//! — once the owner has set a backup passphrase — encrypted with AES-256-GCM (key derived from the
//! passphrase with Argon2id). File layout:
//!
//! ```text
//! "INVBAK01" | header length (u32 LE) | header JSON | payload
//! payload = zstd(db)  or  AES-256-GCM(zstd(db)), the header authenticated as associated data
//! ```
//!
//! The header carries the SHA-256 of the database file, so a restore proves it got back exactly
//! what was saved. Restore never overwrites anything unchecked: the file is decrypted, verified
//! and integrity-checked in a temporary copy, the current database is backed up first
//! (`pre_restore`), and only then copied over the live database with SQLite's backup API.

use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use rand::RngCore;
use rusqlite::{params, Connection, DatabaseName, OpenFlags};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::audit::AuditEntry;
use super::context::Ctx;
use super::settings::{self, BackupSettings, Settings};
use crate::app::AppState;
use crate::db;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::secrets::BACKUP_KEY;

const MAGIC: &[u8; 8] = b"INVBAK01";
pub const EXTENSION: &str = "invbak";
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupHeader {
    pub format: u32,
    pub created_at: String,
    pub app_version: String,
    pub schema_version: i64,
    pub kind: String,
    /// SHA-256 of the database file inside.
    pub sha256: String,
    pub db_bytes: u64,
    pub encrypted: bool,
    pub kdf_salt: Option<String>,
    pub nonce: Option<String>,
}

// ------------------------------------------------------------------ keys

#[derive(Serialize, Deserialize)]
struct StoredKey {
    salt: String,
    key: String,
}

fn derive_key(passphrase: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut key = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(AppError::internal)?;
    Ok(key)
}

fn decode_hex<const N: usize>(value: &str) -> AppResult<[u8; N]> {
    let bytes = hex::decode(value).map_err(|_| AppError::validation("Damaged backup header."))?;
    bytes
        .try_into()
        .map_err(|_| AppError::validation("Damaged backup header."))
}

fn stored_key(state: &AppState) -> AppResult<Option<([u8; 16], [u8; 32])>> {
    let Some(raw) = state.secrets.get(BACKUP_KEY)? else {
        return Ok(None);
    };
    let stored: StoredKey = serde_json::from_str(&raw).map_err(AppError::internal)?;
    Ok(Some((decode_hex(&stored.salt)?, decode_hex(&stored.key)?)))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPassphraseInput {
    pub passphrase: String,
}

/// Sets (or changes) the backup passphrase. From now on every backup is encrypted. Older
/// backups keep their own passphrase. Without the passphrase, an encrypted backup can't be
/// restored on another PC: write it down and keep it safe.
pub fn set_passphrase(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &SetPassphraseInput,
) -> AppResult<()> {
    ctx.require("backup.manage")?;
    let passphrase = input.passphrase.trim();
    if passphrase.chars().count() < 10 {
        return Err(AppError::field(
            "passphrase",
            "Use at least 10 characters (a short sentence works well).",
        ));
    }
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt)?;
    let stored = StoredKey {
        salt: hex::encode(salt),
        key: hex::encode(key),
    };
    state
        .secrets
        .set(BACKUP_KEY, &serde_json::to_string(&stored)?)?;
    let now = ctx.now_str();
    AuditEntry::new("BACKUP_PASSPHRASE_SET", "settings", Some("backup"))
        .by(ctx.actor)
        .write(ctx.conn, &now)
}

// ------------------------------------------------------------------ create

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupRun {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: String,
    pub file_name: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub encrypted: bool,
    pub schema_version: Option<i64>,
    pub mirrored: bool,
    pub error: Option<String>,
    /// The file is still in the backup folder.
    pub file_exists: bool,
}

pub fn backup_dir(state: &AppState, settings: &BackupSettings) -> PathBuf {
    settings
        .directory
        .as_ref()
        .map_or_else(|| state.data_dir().join("backups"), PathBuf::from)
}

fn local_stamp(settings: &Settings, at: DateTime<Utc>) -> String {
    at.with_timezone(&settings.timezone())
        .format("%Y%m%d-%H%M%S")
        .to_string()
}

fn schema_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(max(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )?)
}

fn integrity_ok(conn: &Connection) -> AppResult<bool> {
    let result: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    Ok(result == "ok")
}

fn encode(
    db_bytes: &[u8],
    header: &mut BackupHeader,
    key: Option<([u8; 16], [u8; 32])>,
) -> AppResult<Vec<u8>> {
    let compressed = zstd::encode_all(db_bytes, 3).map_err(AppError::internal)?;
    let payload = match key {
        None => compressed,
        Some((salt, key)) => {
            let mut nonce = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut nonce);
            header.encrypted = true;
            header.kdf_salt = Some(hex::encode(salt));
            header.nonce = Some(hex::encode(nonce));
            let aad = serde_json::to_vec(header)?;
            Aes256Gcm::new_from_slice(&key)
                .map_err(AppError::internal)?
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &compressed,
                        aad: &aad,
                    },
                )
                .map_err(AppError::internal)?
        }
    };
    let header_json = serde_json::to_vec(header)?;
    let mut out = Vec::with_capacity(12 + header_json.len() + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(
        &u32::try_from(header_json.len())
            .map_err(AppError::internal)?
            .to_le_bytes(),
    );
    out.extend_from_slice(&header_json);
    out.extend_from_slice(&payload);
    Ok(out)
}

fn notify(conn: &Connection, kind: &str, severity: &str, title: &str, body: &str, now: &str) {
    let result = conn.execute(
        "INSERT INTO notifications (id, kind, severity, title, body, dedupe_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?2, ?6)
         ON CONFLICT DO NOTHING",
        params![new_id(), kind, severity, title, body, now],
    );
    if let Err(e) = result {
        tracing::warn!("could not record notification {kind}: {e}");
    }
}

struct Written {
    file_name: String,
    size: u64,
    sha256: String,
    encrypted: bool,
    schema_version: i64,
    mirrored: bool,
}

fn write_backup(
    state: &AppState,
    settings: &Settings,
    kind: &str,
    now: DateTime<Utc>,
) -> AppResult<Written> {
    let dir = backup_dir(state, &settings.backup);
    fs::create_dir_all(&dir).map_err(AppError::internal)?;
    // Never overwrite an earlier backup made in the same second.
    let stamp = local_stamp(settings, now);
    let mut file_name = format!("inventory-{stamp}-{kind}.{EXTENSION}");
    let mut n = 2;
    while dir.join(&file_name).exists() {
        file_name = format!("inventory-{stamp}-{n}-{kind}.{EXTENSION}");
        n += 1;
    }
    let tmp_db = dir.join(format!(".tmp-{}.sqlite", new_id()));
    let cleanup = |p: &Path| {
        let _ = fs::remove_file(p);
    };
    let result = (|| {
        state
            .conn()?
            .backup(DatabaseName::Main, &tmp_db, None)
            .map_err(AppError::from)?;
        let schema_version = {
            // The copy inherits WAL mode; a backup file must be one self-contained file.
            let copy = Connection::open(&tmp_db)?;
            copy.query_row("PRAGMA journal_mode = DELETE", [], |r| {
                r.get::<_, String>(0)
            })?;
            if !integrity_ok(&copy)? {
                return Err(AppError::internal("backup copy failed its integrity check"));
            }
            schema_version(&copy)?
        };
        let db_bytes = fs::read(&tmp_db).map_err(AppError::internal)?;
        let sha256 = hex::encode(Sha256::digest(&db_bytes));
        let mut header = BackupHeader {
            format: 1,
            created_at: now.to_rfc3339(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version,
            kind: kind.to_string(),
            sha256: sha256.clone(),
            db_bytes: db_bytes.len() as u64,
            encrypted: false,
            kdf_salt: None,
            nonce: None,
        };
        let bytes = encode(&db_bytes, &mut header, stored_key(state)?)?;
        let target = dir.join(&file_name);
        let part = dir.join(format!("{file_name}.part"));
        fs::write(&part, &bytes).map_err(AppError::internal)?;
        fs::rename(&part, &target).map_err(AppError::internal)?;
        let mirrored = match &settings.backup.mirror_directory {
            Some(mirror) => match fs::create_dir_all(mirror)
                .and_then(|()| fs::copy(&target, Path::new(mirror).join(&file_name)))
            {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!("backup mirror copy to {mirror} failed: {e}");
                    notify(
                        &*state.conn()?,
                        "backup_mirror_failed",
                        "warning",
                        "Backup copy to the second folder failed",
                        &format!("{mirror}: {e}"),
                        &now.to_rfc3339(),
                    );
                    false
                }
            },
            None => false,
        };
        Ok(Written {
            file_name: file_name.clone(),
            size: bytes.len() as u64,
            sha256,
            encrypted: header.encrypted,
            schema_version,
            mirrored,
        })
    })();
    cleanup(&tmp_db);
    result
}

fn record_run(conn: &Connection, run: &BackupRun, user_id: Option<&str>) -> AppResult<()> {
    conn.execute(
        "INSERT INTO backup_runs (id, kind, status, started_at, finished_at, file_name, size_bytes, sha256, encrypted,
            schema_version, mirrored, error, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            run.id,
            run.kind,
            run.status,
            run.started_at,
            run.finished_at,
            run.file_name,
            run.size_bytes,
            run.sha256,
            run.encrypted,
            run.schema_version,
            run.mirrored,
            run.error,
            user_id
        ],
    )?;
    Ok(())
}

/// Makes a backup now. Failures are recorded (and shown as a notification) and returned.
pub fn create(state: &AppState, kind: &str, user_id: Option<&str>) -> AppResult<BackupRun> {
    let started = state.now();
    let settings = settings::load(&*state.conn()?)?;
    let outcome = write_backup(state, &settings, kind, started);
    let finished = state.now().to_rfc3339();
    let run = match &outcome {
        Ok(w) => BackupRun {
            id: new_id(),
            kind: kind.into(),
            status: "ok".into(),
            started_at: started.to_rfc3339(),
            finished_at: finished.clone(),
            file_name: Some(w.file_name.clone()),
            size_bytes: i64::try_from(w.size).ok(),
            sha256: Some(w.sha256.clone()),
            encrypted: w.encrypted,
            schema_version: Some(w.schema_version),
            mirrored: w.mirrored,
            error: None,
            file_exists: true,
        },
        Err(e) => BackupRun {
            id: new_id(),
            kind: kind.into(),
            status: "failed".into(),
            started_at: started.to_rfc3339(),
            finished_at: finished.clone(),
            file_name: None,
            size_bytes: None,
            sha256: None,
            encrypted: false,
            schema_version: None,
            mirrored: false,
            error: Some(e.message.clone()),
            file_exists: false,
        },
    };
    let conn = state.conn()?;
    record_run(&conn, &run, user_id)?;
    match outcome {
        Ok(_) => {
            // A good backup clears an earlier failure notice.
            conn.execute(
                "UPDATE notifications SET read_at = ?1 WHERE dedupe_key = 'backup_failed' AND read_at IS NULL",
                [&finished],
            )?;
            Ok(run)
        }
        Err(e) => {
            notify(
                &conn,
                "backup_failed",
                "error",
                "Backup failed",
                &e.message,
                &finished,
            );
            Err(e)
        }
    }
}

// ------------------------------------------------------------------ status

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub directory: String,
    pub encrypted: bool,
    pub settings: BackupSettings,
    pub last_success: Option<BackupRun>,
    pub runs: Vec<BackupRun>,
}

pub fn status(state: &AppState, ctx: &Ctx<'_>) -> AppResult<BackupStatus> {
    ctx.require("backup.manage")?;
    let settings = settings::load(ctx.conn)?;
    let dir = backup_dir(state, &settings.backup);
    let runs = ctx
        .conn
        .prepare(
            "SELECT id, kind, status, started_at, finished_at, file_name, size_bytes, sha256, encrypted, schema_version,
                mirrored, error
             FROM backup_runs ORDER BY started_at DESC LIMIT 50",
        )?
        .query_map([], |r| {
            let file_name: Option<String> = r.get(5)?;
            Ok(BackupRun {
                id: r.get(0)?,
                kind: r.get(1)?,
                status: r.get(2)?,
                started_at: r.get(3)?,
                finished_at: r.get(4)?,
                file_exists: file_name.as_ref().is_some_and(|f| dir.join(f).is_file()),
                file_name,
                size_bytes: r.get(6)?,
                sha256: r.get(7)?,
                encrypted: r.get(8)?,
                schema_version: r.get(9)?,
                mirrored: r.get(10)?,
                error: r.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BackupStatus {
        directory: dir.display().to_string(),
        encrypted: stored_key(state)?.is_some(),
        settings: settings.backup,
        last_success: runs.iter().find(|r| r.status == "ok").cloned(),
        runs,
    })
}

// ------------------------------------------------------------------ verify and restore

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreInput {
    /// Full path of a .invbak file.
    pub path: String,
    /// Needed for encrypted backups made with a different passphrase than the current one.
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedBackup {
    pub header: BackupHeader,
    pub users: i64,
    pub products: i64,
    pub invoices: i64,
    pub last_invoice_date: Option<String>,
}

fn bad_file(message: &str) -> AppError {
    AppError::field("path", message)
}

/// Reads, decrypts and checks a backup file into a temporary database. Returns the header and
/// the path of the checked copy (the caller deletes it).
fn open_backup(state: &AppState, input: &RestoreInput) -> AppResult<(BackupHeader, PathBuf)> {
    let path = Path::new(&input.path);
    if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
        return Err(bad_file("Choose a .invbak backup file."));
    }
    let meta = fs::metadata(path).map_err(|_| bad_file("That file can't be opened."))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(bad_file("That file is too large to be a backup."));
    }
    let bytes = fs::read(path).map_err(|_| bad_file("That file can't be read."))?;
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return Err(bad_file("This is not a backup made by this app."));
    }
    let header_len =
        u32::from_le_bytes(bytes[8..12].try_into().map_err(AppError::internal)?) as usize;
    let header_end = 12usize
        .checked_add(header_len)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| bad_file("The backup file is damaged."))?;
    let header_bytes = &bytes[12..header_end];
    let header: BackupHeader = serde_json::from_slice(header_bytes)
        .map_err(|_| bad_file("The backup file is damaged."))?;
    if header.format != 1 {
        return Err(bad_file(
            "This backup was made by a newer version of the app.",
        ));
    }
    let payload = &bytes[header_end..];
    let compressed = if header.encrypted {
        let salt: [u8; 16] = decode_hex(header.kdf_salt.as_deref().unwrap_or_default())?;
        let nonce: [u8; 12] = decode_hex(header.nonce.as_deref().unwrap_or_default())?;
        let key = match (&input.passphrase, stored_key(state)?) {
            (Some(p), _) => derive_key(p, &salt)?,
            (None, Some((stored_salt, key))) if stored_salt == salt => key,
            (None, _) => {
                return Err(AppError::new(
                    ErrorCode::Validation,
                    "This backup is encrypted. Enter the backup passphrase it was made with.",
                )
                .with_details(
                    serde_json::json!({ "field": "passphrase", "needsPassphrase": true }),
                ))
            }
        };
        // The header bytes are authenticated: changing any of them makes decryption fail.
        Aes256Gcm::new_from_slice(&key)
            .map_err(AppError::internal)?
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: payload,
                    aad: header_bytes,
                },
            )
            .map_err(|_| {
                AppError::field(
                    "passphrase",
                    "The passphrase is wrong, or the backup file was changed.",
                )
            })?
    } else {
        payload.to_vec()
    };
    let db_bytes = zstd::decode_all(compressed.as_slice())
        .map_err(|_| bad_file("The backup file is damaged."))?;
    if hex::encode(Sha256::digest(&db_bytes)) != header.sha256 {
        return Err(bad_file("The backup file is damaged (checksum mismatch)."));
    }
    let tmp = state
        .data_dir()
        .join(format!(".restore-{}.sqlite", new_id()));
    fs::write(&tmp, &db_bytes).map_err(AppError::internal)?;
    let checked = (|| {
        // Our own temporary copy; read-write because FTS5's integrity check writes.
        let conn = Connection::open(&tmp)?;
        if !integrity_ok(&conn)? {
            return Err(bad_file("The database inside the backup is damaged."));
        }
        let version = schema_version(&conn)?;
        if version > db::migrate::latest_version() {
            return Err(bad_file(
                "This backup comes from a newer version of the app. Update the app first.",
            ));
        }
        let tables: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('users', 'settings', 'sales_invoices', 'stock_movements')",
            [],
            |r| r.get(0),
        )?;
        if tables < 4 {
            return Err(bad_file("This file is not an inventory database backup."));
        }
        Ok(())
    })();
    if let Err(e) = checked {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok((header, tmp))
}

/// Checks a backup file without restoring it (and shows what it contains).
pub fn verify(state: &AppState, ctx: &Ctx<'_>, input: &RestoreInput) -> AppResult<VerifiedBackup> {
    ctx.require("backup.manage")?;
    let (header, tmp) = open_backup(state, input)?;
    let summary = (|| {
        let conn = Connection::open_with_flags(&tmp, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
        Ok::<_, AppError>(VerifiedBackup {
            header: header.clone(),
            users: count("SELECT count(*) FROM users WHERE deleted_at IS NULL")?,
            products: count("SELECT count(*) FROM products WHERE deleted_at IS NULL")?,
            invoices: count("SELECT count(*) FROM sales_invoices WHERE status = 'finalized'")?,
            last_invoice_date: conn.query_row(
                "SELECT max(business_date) FROM sales_invoices WHERE status = 'finalized'",
                [],
                |r| r.get(0),
            )?,
        })
    })();
    let _ = fs::remove_file(&tmp);
    summary
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored: VerifiedBackup,
    /// Backup of the database as it was just before the restore.
    pub safety_backup: BackupRun,
}

/// Replaces the live database with a verified backup. Everyone is signed out afterwards.
pub fn restore(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &RestoreInput,
) -> AppResult<RestoreResult> {
    ctx.require("backup.manage")?;
    let restored = verify(state, ctx, input)?;
    let safety = create(state, "pre_restore", Some(&ctx.actor.user_id))?;
    let (header, tmp) = open_backup(state, input)?;
    let outcome = (|| {
        let mut conn = state.conn()?;
        conn.restore(
            DatabaseName::Main,
            &tmp,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        // A backup from an older version is brought up to the current schema.
        db::migrate::run(&mut conn)?;
        let now = state.now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET revoked_at = ?1 WHERE revoked_at IS NULL",
            [&now],
        )?;
        // The restored database doesn't know about the safety backup or this restore yet.
        record_run(&conn, &safety, Some(&ctx.actor.user_id)).ok();
        let run = BackupRun {
            id: new_id(),
            kind: "restore".into(),
            status: "ok".into(),
            started_at: now.clone(),
            finished_at: now.clone(),
            file_name: Path::new(&input.path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string()),
            size_bytes: None,
            sha256: Some(header.sha256.clone()),
            encrypted: header.encrypted,
            schema_version: Some(header.schema_version),
            mirrored: false,
            error: None,
            file_exists: true,
        };
        record_run(&conn, &run, None)?;
        AuditEntry::new("BACKUP_RESTORED", "backup_runs", Some(&run.id))
            .summary(format!(
                "restored {} (made {}), safety backup {}",
                run.file_name.clone().unwrap_or_default(),
                header.created_at,
                safety.file_name.clone().unwrap_or_default()
            ))
            .write(&conn, &now)?;
        Ok::<_, AppError>(())
    })();
    let _ = fs::remove_file(&tmp);
    outcome?;
    tracing::warn!("database restored from {}", input.path);
    Ok(RestoreResult {
        restored,
        safety_backup: safety,
    })
}

// ------------------------------------------------------------------ retention

/// Parses "inventory-YYYYMMDD-HHMMSS[-n]-kind.invbak".
fn parse_name(name: &str) -> Option<(NaiveDateTime, String)> {
    let rest = name
        .strip_prefix("inventory-")?
        .strip_suffix(&format!(".{EXTENSION}"))?;
    let stamp = rest.get(..15)?;
    // "…-HHMMSS-kind" or, for a second backup in the same second, "…-HHMMSS-2-kind".
    let kind = rest.rsplit('-').next()?.to_string();
    let at = NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    Some((at, kind))
}

/// Which scheduled backups to keep: the newest of each of the last `keep_daily` days, of the
/// last `keep_weekly` ISO weeks and of the last `keep_monthly` months. Safety (pre_restore)
/// backups: the newest 5. Manual backups are never deleted automatically.
pub fn files_to_prune(names: &[String], settings: &BackupSettings) -> Vec<String> {
    let mut scheduled: Vec<(NaiveDateTime, &String)> = Vec::new();
    let mut safety: Vec<(NaiveDateTime, &String)> = Vec::new();
    for name in names {
        match parse_name(name) {
            Some((at, kind)) if kind == "scheduled" => scheduled.push((at, name)),
            Some((at, kind)) if kind == "pre_restore" => safety.push((at, name)),
            _ => {}
        }
    }
    scheduled.sort_by_key(|b| std::cmp::Reverse(b.0));
    safety.sort_by_key(|b| std::cmp::Reverse(b.0));
    let mut keep = std::collections::HashSet::new();
    let mut bucket = |key: &dyn Fn(&NaiveDateTime) -> (i32, u32), limit: i64| {
        let mut seen = Vec::new();
        for (at, name) in &scheduled {
            let k = key(at);
            if !seen.contains(&k) {
                if seen.len() as i64 >= limit {
                    break;
                }
                seen.push(k);
                keep.insert((*name).clone());
            }
        }
    };
    bucket(&|d| (d.year(), d.ordinal()), settings.keep_daily);
    bucket(
        &|d| (d.iso_week().year(), d.iso_week().week()),
        settings.keep_weekly,
    );
    bucket(&|d| (d.year(), d.month()), settings.keep_monthly);
    if let Some((_, newest)) = scheduled.first() {
        keep.insert((*newest).clone());
    }
    let mut prune: Vec<String> = scheduled
        .iter()
        .filter(|(_, n)| !keep.contains(*n))
        .map(|(_, n)| (*n).clone())
        .collect();
    prune.extend(safety.iter().skip(5).map(|(_, n)| (*n).clone()));
    prune
}

/// Applies the retention rules to the backup folder. Returns how many files were removed.
pub fn prune(state: &AppState) -> AppResult<usize> {
    let settings = settings::load(&*state.conn()?)?.backup;
    let dir = backup_dir(state, &settings);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(0);
    };
    let names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    let mut removed = 0;
    for name in files_to_prune(&names, &settings) {
        match fs::remove_file(dir.join(&name)) {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!("could not remove old backup {name}: {e}"),
        }
    }
    Ok(removed)
}
