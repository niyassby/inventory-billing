//! A counter's own small database (`client.db`): the cached catalogue for offline billing, the
//! signed-in users (for offline permission checks), leased invoice numbers and the outbox of sales
//! made while the hub was unreachable. The hub's database stays the only source of truth; this
//! file only ever holds copies and not-yet-sent sales.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::services::auth::{ApprovalInput, SessionUser};
use crate::services::catalog::SearchItem;
use crate::services::sync::{CachedCustomer, CatalogueSnapshot, Lease};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS cache_items (
  id          INTEGER PRIMARY KEY,
  variant_id  TEXT NOT NULL,
  unit_id     TEXT NOT NULL,
  barcode     TEXT,
  sku         TEXT NOT NULL,
  search      TEXT NOT NULL,
  json        TEXT NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS ix_cache_items_variant ON cache_items (variant_id, unit_id);
CREATE INDEX IF NOT EXISTS ix_cache_items_barcode ON cache_items (barcode);
CREATE INDEX IF NOT EXISTS ix_cache_items_sku ON cache_items (sku COLLATE NOCASE);
CREATE TABLE IF NOT EXISTS cache_customers (id TEXT PRIMARY KEY, name TEXT NOT NULL, json TEXT NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS sessions (
  token_hash  TEXT PRIMARY KEY,
  user_json   TEXT NOT NULL,
  expires_at  TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS leases (
  lease_id    TEXT PRIMARY KEY,
  json        TEXT NOT NULL,
  period      TEXT NOT NULL,
  next_seq    INTEGER NOT NULL,
  to_seq      INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS outbox (
  id               TEXT PRIMARY KEY,
  idempotency_key  TEXT NOT NULL UNIQUE,
  device_seq       INTEGER NOT NULL UNIQUE,
  command          TEXT NOT NULL,
  payload_json     TEXT NOT NULL,
  session_token    TEXT NOT NULL,
  client_created_at TEXT NOT NULL,
  doc_no           TEXT,
  total            INTEGER NOT NULL DEFAULT 0,
  status           TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done', 'rejected', 'discarded')),
  attempts         INTEGER NOT NULL DEFAULT 0,
  approval_json    TEXT,
  last_error_json  TEXT,
  result_doc_no    TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS ix_outbox_status ON outbox (status, device_seq);
";

pub fn open(path: &Path) -> AppResult<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(AppError::internal)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA busy_timeout = 5000;",
    )?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn get_meta(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ------------------------------------------------------------------ catalogue

/// The snapshot without its item and customer lists (kept in their own tables).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedContext {
    pub generated_at: String,
    pub settings: crate::services::settings::Settings,
    pub branch_id: String,
    pub branch_state: Option<String>,
    pub warehouse_id: String,
}

pub fn save_catalogue(conn: &mut Connection, snapshot: &CatalogueSnapshot) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM cache_items", [])?;
    tx.execute("DELETE FROM cache_customers", [])?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO cache_items (variant_id, unit_id, barcode, sku, search, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for item in &snapshot.items {
            let search = format!("{} {}", item.name, item.sku).to_lowercase();
            insert.execute(params![
                item.variant_id,
                item.unit_id,
                item.barcode,
                item.sku,
                search,
                serde_json::to_string(item)?
            ])?;
        }
        let mut customers =
            tx.prepare("INSERT INTO cache_customers (id, name, json) VALUES (?1, ?2, ?3)")?;
        for c in &snapshot.customers {
            customers.execute(params![c.id, c.name, serde_json::to_string(c)?])?;
        }
    }
    let context = CachedContext {
        generated_at: snapshot.generated_at.clone(),
        settings: snapshot.settings.clone(),
        branch_id: snapshot.branch_id.clone(),
        branch_state: snapshot.branch_state.clone(),
        warehouse_id: snapshot.warehouse_id.clone(),
    };
    set_meta(&tx, "context", &serde_json::to_string(&context)?)?;
    tx.commit()?;
    Ok(())
}

pub fn context(conn: &Connection) -> AppResult<Option<CachedContext>> {
    get_meta(conn, "context")?
        .map(|c| serde_json::from_str(&c).map_err(AppError::internal))
        .transpose()
}

fn items(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> AppResult<Vec<SearchItem>> {
    let rows = conn
        .prepare(sql)?
        .query_map(args, |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter()
        .map(|j| serde_json::from_str(j).map_err(AppError::internal))
        .collect()
}

/// Exact barcode, then SKU (same order as the hub's lookup).
pub fn lookup(conn: &Connection, code: &str) -> AppResult<Option<SearchItem>> {
    let code = code.trim();
    if code.is_empty() {
        return Ok(None);
    }
    let by_barcode = items(
        conn,
        "SELECT json FROM cache_items WHERE barcode = ?1 LIMIT 1",
        [code],
    )?;
    if let Some(item) = by_barcode.into_iter().next() {
        return Ok(Some(item));
    }
    Ok(items(
        conn,
        "SELECT json FROM cache_items WHERE sku = ?1 COLLATE NOCASE ORDER BY id LIMIT 1",
        [code],
    )?
    .into_iter()
    .next())
}

/// Every word of the term must appear in the name or SKU.
pub fn search(conn: &Connection, term: &str, limit: u32) -> AppResult<Vec<SearchItem>> {
    let words: Vec<String> = term
        .to_lowercase()
        .split_whitespace()
        .take(5)
        .map(|w| {
            format!(
                "%{}%",
                w.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        })
        .collect();
    let mut results = Vec::new();
    if let Some(exact) = lookup(conn, term)? {
        results.push(exact);
    }
    if words.is_empty() {
        return Ok(results);
    }
    let filter = vec!["search LIKE ? ESCAPE '\\'"; words.len()].join(" AND ");
    let found = items(
        conn,
        &format!(
            "SELECT json FROM cache_items WHERE {filter} ORDER BY search LIMIT {}",
            limit.clamp(1, 50)
        ),
        rusqlite::params_from_iter(words.iter()),
    )?;
    for item in found {
        if !results
            .iter()
            .any(|r: &SearchItem| r.variant_id == item.variant_id && r.unit_id == item.unit_id)
        {
            results.push(item);
        }
    }
    results.truncate(limit.clamp(1, 50) as usize);
    Ok(results)
}

/// The cached entry for a variant in a unit (`None`: its base unit).
pub fn item(
    conn: &Connection,
    variant_id: &str,
    unit_id: Option<&str>,
) -> AppResult<Option<SearchItem>> {
    let all = items(
        conn,
        "SELECT json FROM cache_items WHERE variant_id = ?1 ORDER BY id",
        [variant_id],
    )?;
    Ok(match unit_id {
        None => all
            .into_iter()
            .find(|i| i.factor_num == 1 && i.factor_den == 1),
        Some(unit) => {
            let base = all
                .iter()
                .find(|i| i.factor_num == 1 && i.factor_den == 1)
                .map(|i| i.unit_id.clone());
            if base.as_deref() == Some(unit) {
                all.into_iter()
                    .find(|i| i.factor_num == 1 && i.factor_den == 1)
            } else {
                all.into_iter().find(|i| i.unit_id == unit)
            }
        }
    })
}

pub fn customer(conn: &Connection, id: &str) -> AppResult<Option<CachedCustomer>> {
    conn.query_row(
        "SELECT json FROM cache_customers WHERE id = ?1",
        [id],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|j| serde_json::from_str(&j).map_err(AppError::internal))
    .transpose()
}

pub fn customers(conn: &Connection, term: &str) -> AppResult<Vec<CachedCustomer>> {
    let pattern = format!("%{}%", term.trim().to_lowercase());
    let rows = conn
        .prepare("SELECT json FROM cache_customers WHERE lower(name) LIKE ?1 OR json LIKE ?1 ORDER BY name LIMIT 30")?
        .query_map([pattern], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter()
        .map(|j| serde_json::from_str(j).map_err(AppError::internal))
        .collect()
}

// ------------------------------------------------------------------ sessions

pub fn remember_session(
    conn: &Connection,
    token: &str,
    user: &SessionUser,
    expires_at: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO sessions (token_hash, user_json, expires_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (token_hash) DO UPDATE SET user_json = excluded.user_json,
           expires_at = CASE WHEN excluded.expires_at = '' THEN sessions.expires_at ELSE excluded.expires_at END",
        params![crate::auth::hash_token(token), serde_json::to_string(user)?, expires_at],
    )?;
    Ok(())
}

pub fn forget_session(conn: &Connection, token: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM sessions WHERE token_hash = ?1",
        [crate::auth::hash_token(token)],
    )?;
    Ok(())
}

/// The signed-in user behind a token, if the session hasn't expired.
pub fn session(conn: &Connection, token: &str, now: &str) -> AppResult<Option<SessionUser>> {
    conn.query_row(
        "SELECT user_json FROM sessions WHERE token_hash = ?1 AND (expires_at = '' OR expires_at > ?2)",
        params![crate::auth::hash_token(token), now],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|j| serde_json::from_str(&j).map_err(AppError::internal))
    .transpose()
}

// ------------------------------------------------------------------ leases

pub fn add_lease(conn: &Connection, lease: &Lease) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO leases (lease_id, json, period, next_seq, to_seq) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![lease.lease_id, serde_json::to_string(lease)?, lease.period, lease.from_seq, lease.to_seq],
    )?;
    Ok(())
}

/// Numbers still unused in leases for `period`.
pub fn numbers_left(conn: &Connection, period: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(sum(to_seq - next_seq + 1), 0) FROM leases WHERE period = ?1 AND next_seq <= to_seq",
        [period],
        |r| r.get(0),
    )?)
}

/// Takes the next offline number (in the caller's transaction).
pub fn take_number(conn: &Connection, period: &str) -> AppResult<Option<(Lease, i64)>> {
    let row: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT lease_id, json, next_seq FROM leases WHERE period = ?1 AND next_seq <= to_seq
             ORDER BY next_seq LIMIT 1",
            [period],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((lease_id, json, seq)) = row else {
        return Ok(None);
    };
    conn.execute(
        "UPDATE leases SET next_seq = next_seq + 1 WHERE lease_id = ?1",
        [&lease_id],
    )?;
    Ok(Some((
        serde_json::from_str(&json).map_err(AppError::internal)?,
        seq,
    )))
}

// ------------------------------------------------------------------ outbox

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxEntry {
    pub id: String,
    pub device_seq: i64,
    pub command: String,
    pub doc_no: Option<String>,
    pub total: i64,
    /// "pending", "done", "rejected" or "discarded".
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<AppError>,
    pub client_created_at: String,
}

pub struct NewOutboxItem<'a> {
    pub id: &'a str,
    pub idempotency_key: &'a str,
    pub command: &'a str,
    pub payload: &'a Value,
    pub session_token: &'a str,
    pub client_created_at: &'a str,
    pub doc_no: &'a str,
    pub total: i64,
}

pub fn enqueue(conn: &Connection, item: &NewOutboxItem<'_>) -> AppResult<i64> {
    let seq: i64 = conn.query_row(
        "SELECT coalesce(max(device_seq), 0) + 1 FROM outbox",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO outbox (id, idempotency_key, device_seq, command, payload_json, session_token, client_created_at,
            doc_no, total, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?7, ?7)",
        params![
            item.id,
            item.idempotency_key,
            seq,
            item.command,
            item.payload.to_string(),
            item.session_token,
            item.client_created_at,
            item.doc_no,
            item.total
        ],
    )?;
    Ok(seq)
}

pub struct PendingItem {
    pub id: String,
    pub idempotency_key: String,
    pub device_seq: i64,
    pub command: String,
    pub payload: Value,
    pub session_token: String,
    pub client_created_at: String,
    pub approval: Option<ApprovalInput>,
}

pub fn pending(conn: &Connection, limit: i64) -> AppResult<Vec<PendingItem>> {
    let rows = conn
        .prepare(
            "SELECT id, idempotency_key, device_seq, command, payload_json, session_token, client_created_at, approval_json
             FROM outbox WHERE status = 'pending' ORDER BY device_seq LIMIT ?1",
        )?
        .query_map([limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, key, seq, command, payload, token, at, approval)| {
            Ok(PendingItem {
                id,
                idempotency_key: key,
                device_seq: seq,
                command,
                payload: serde_json::from_str(&payload).map_err(AppError::internal)?,
                session_token: token,
                client_created_at: at,
                approval: approval
                    .map(|a| serde_json::from_str(&a).map_err(AppError::internal))
                    .transpose()?,
            })
        })
        .collect()
}

/// Applied on the hub: the session token is no longer needed and is wiped.
pub fn mark_done(conn: &Connection, key: &str, doc_no: Option<&str>, now: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE outbox SET status = 'done', session_token = '', approval_json = NULL, last_error_json = NULL,
            result_doc_no = ?1, attempts = attempts + 1, updated_at = ?2 WHERE idempotency_key = ?3",
        params![doc_no, now, key],
    )?;
    Ok(())
}

pub fn mark_failed(
    conn: &Connection,
    key: &str,
    error: &AppError,
    rejected: bool,
    now: &str,
) -> AppResult<()> {
    conn.execute(
        "UPDATE outbox SET status = CASE WHEN ?1 THEN 'rejected' ELSE status END, approval_json = NULL,
            last_error_json = ?2, attempts = attempts + 1, updated_at = ?3 WHERE idempotency_key = ?4",
        params![rejected, serde_json::to_string(error)?, now, key],
    )?;
    Ok(())
}

pub fn count(conn: &Connection, status: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM outbox WHERE status = ?1",
        [status],
        |r| r.get(0),
    )?)
}

pub fn list(conn: &Connection, include_done: bool) -> AppResult<Vec<OutboxEntry>> {
    let filter = if include_done {
        "1 = 1"
    } else {
        "status IN ('pending', 'rejected')"
    };
    let rows = conn
        .prepare(&format!(
            "SELECT id, device_seq, command, coalesce(result_doc_no, doc_no), total, status, attempts, last_error_json,
                client_created_at
             FROM outbox WHERE {filter} ORDER BY device_seq DESC LIMIT 500"
        ))?
        .query_map([], |r| {
            Ok(OutboxEntry {
                id: r.get(0)?,
                device_seq: r.get(1)?,
                command: r.get(2)?,
                doc_no: r.get(3)?,
                total: r.get(4)?,
                status: r.get(5)?,
                attempts: r.get(6)?,
                last_error: r
                    .get::<_, Option<String>>(7)?
                    .and_then(|e| serde_json::from_str(&e).ok()),
                client_created_at: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Puts a rejected item back in the queue, optionally with a manager's approval for the hub.
pub fn retry(
    conn: &Connection,
    id: &str,
    approval: Option<&ApprovalInput>,
    now: &str,
) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE outbox SET status = 'pending', approval_json = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'rejected'",
        params![approval.map(serde_json::to_string).transpose()?, now, id],
    )?;
    if changed == 0 {
        return Err(AppError::not_found("Rejected offline sale"));
    }
    Ok(())
}

pub fn discard(conn: &Connection, id: &str, now: &str) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE outbox SET status = 'discarded', session_token = '', updated_at = ?1 WHERE id = ?2 AND status = 'rejected'",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(AppError::not_found("Rejected offline sale"));
    }
    Ok(())
}
