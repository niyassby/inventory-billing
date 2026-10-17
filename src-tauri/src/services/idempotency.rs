//! Idempotent writes: a retried request (network blip, double click, outbox replay) returns the
//! original result instead of creating a second invoice. Same key with a different request body
//! is a conflict, never silently accepted.

use rusqlite::{params, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::Actor;
use crate::error::{AppError, AppResult};

pub fn validate_key(key: &str) -> AppResult<()> {
    let ok = (8..=100).contains(&key.len())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AppError::field(
            "idempotencyKey",
            "A request id (8–100 letters, digits, - or _) is required.",
        ))
    }
}

/// Runs `f` once per key. Must be called inside the write transaction that does the work, so the
/// stored response commits (or rolls back) together with it.
pub fn once<Req, Res>(
    tx: &Transaction<'_>,
    key: &str,
    command: &str,
    actor: &Actor,
    request: &Req,
    now: &str,
    f: impl FnOnce() -> AppResult<Res>,
) -> AppResult<Res>
where
    Req: Serialize,
    Res: Serialize + DeserializeOwned,
{
    validate_key(key)?;
    let hash = hex::encode(Sha256::digest(serde_json::to_vec(request)?));
    let stored: Option<(String, String, Option<String>)> = tx
        .query_row(
            "SELECT command, request_hash, response_json FROM idempotency_keys WHERE key = ?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((stored_command, stored_hash, response)) = stored {
        if stored_command != command || stored_hash != hash {
            return Err(AppError::conflict(
                "This request id was already used for a different request.",
            ));
        }
        let response =
            response.ok_or_else(|| AppError::internal("idempotency key without response"))?;
        return Ok(serde_json::from_str(&response)?);
    }
    let result = f()?;
    tx.execute(
        "INSERT INTO idempotency_keys (key, user_id, device_id, command, request_hash, response_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![key, actor.user_id, actor.device_id, command, hash, serde_json::to_string(&result)?, now],
    )?;
    Ok(result)
}
