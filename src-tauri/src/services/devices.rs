//! Counters and other PCs paired with the hub (docs/ARCHITECTURE.md §7).
//!
//! Pairing: a manager shows a 6-digit code on the hub (valid 5 minutes, single use). The new
//! device sends it once and receives its own credential: a random 256-bit token kept in the
//! device's OS keychain; the hub stores only its hash. Every LAN request carries that token and
//! the user's session token. Revoking a device ends its sessions and number leases at once.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audit::AuditEntry;
use super::common::required;
use super::users::IdInput;
use super::{write, Ctx};
use crate::app::RateLimiter;
use crate::auth as crypto;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};

const CODE_MINUTES: i64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairingCode {
    pub code: String,
    pub expires_at: String,
}

/// Shows a new one-time pairing code on the hub (devices.manage).
pub fn create_pairing_code(ctx: &mut Ctx<'_>) -> AppResult<PairingCode> {
    ctx.require("devices.manage")?;
    let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
    let now = ctx.now_str();
    let expires_at = (ctx.now + chrono::Duration::minutes(CODE_MINUTES)).to_rfc3339();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "INSERT INTO pairing_codes (code_hash, created_by, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            params![crypto::hash_token(&code), actor.user_id, now, expires_at],
        )?;
        AuditEntry::new("PAIRING_CODE_CREATED", "devices", None)
            .by(actor)
            .write(tx, &now)
    })?;
    Ok(PairingCode { code, expires_at })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairInput {
    pub code: String,
    pub device_name: String,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub device_id: String,
    /// Prefix of the device's offline invoice series ("C1", "C2", …).
    pub short_code: String,
    /// The device credential. Shown exactly once; store it in the OS keychain.
    pub device_token: String,
    pub business_name: String,
}

fn next_short_code(conn: &Connection) -> AppResult<String> {
    let max: Option<i64> = conn.query_row(
        "SELECT max(CAST(substr(short_code, 2) AS INTEGER)) FROM devices WHERE short_code LIKE 'C%'",
        [],
        |r| r.get(0),
    )?;
    Ok(format!("C{}", max.unwrap_or(0) + 1))
}

/// Exchanges a valid pairing code for a device credential.
pub fn pair(
    conn: &mut Connection,
    limiter: &RateLimiter,
    now: DateTime<Utc>,
    input: &PairInput,
) -> AppResult<Paired> {
    // One shared budget: 10 guesses per 5 minutes against a 1-in-a-million code.
    limiter.check("pair", 10, Duration::from_secs(300))?;
    let name = required("deviceName", &input.device_name, 60)?;
    let code = input.code.trim();
    let now_str = now.to_rfc3339();
    let invalid = || {
        AppError::new(
            ErrorCode::Unauthorized,
            "That pairing code is wrong or has expired. Show a new code on the hub.",
        )
    };
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    let token = crypto::generate_session_token();
    let paired = write(conn, |tx| {
        let creator: Option<String> = tx
            .query_row(
                "SELECT created_by FROM pairing_codes WHERE code_hash = ?1 AND used_at IS NULL AND expires_at > ?2",
                params![crypto::hash_token(code), now_str],
                |r| r.get(0),
            )
            .optional()?;
        let Some(creator) = creator else {
            return Err(invalid());
        };
        let (branch_id, _) = super::setup::default_location(tx)?;
        let device_id = new_id();
        let short_code = next_short_code(tx)?;
        tx.execute(
            "INSERT INTO devices (id, name, mode, branch_id, token_hash, app_version, paired_at, short_code, created_at,
                updated_at, created_by)
             VALUES (?1, ?2, 'client', ?3, ?4, ?5, ?6, ?7, ?6, ?6, ?8)",
            params![device_id, name, branch_id, crypto::hash_token(&token), input.app_version, now_str, short_code, creator],
        )?;
        tx.execute(
            "UPDATE pairing_codes SET used_at = ?1, device_id = ?2 WHERE code_hash = ?3",
            params![now_str, device_id, crypto::hash_token(code)],
        )?;
        let mut entry = AuditEntry::new("DEVICE_PAIRED", "devices", Some(&device_id))
            .summary(format!("{name} ({short_code})"));
        entry.user_id = Some(&creator);
        entry.write(tx, &now_str)?;
        let business_name = super::settings::load(tx)?.business.name;
        Ok(Paired {
            device_id,
            short_code,
            device_token: token.clone(),
            business_name,
        })
    })?;
    limiter.reset("pair");
    Ok(paired)
}

/// A paired device, as the hub knows it.
#[derive(Debug, Clone)]
pub struct DeviceRef {
    pub id: String,
    pub name: String,
    pub short_code: String,
    pub branch_id: String,
}

/// Resolves a device credential; revoked or unknown devices get UNAUTHORIZED.
pub fn authenticate_device(
    conn: &Connection,
    token: &str,
    now: DateTime<Utc>,
) -> AppResult<DeviceRef> {
    let row: Option<(DeviceRef, Option<String>)> = conn
        .query_row(
            "SELECT id, name, coalesce(short_code, ''), coalesce(branch_id, ''), last_seen_at FROM devices
             WHERE token_hash = ?1 AND revoked_at IS NULL AND deleted_at IS NULL",
            [crypto::hash_token(token)],
            |r| {
                Ok((
                    DeviceRef {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        short_code: r.get(2)?,
                        branch_id: r.get(3)?,
                    },
                    r.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((device, last_seen)) = row else {
        return Err(AppError::new(
            ErrorCode::Unauthorized,
            "This device is not paired with the hub (or was removed). Pair it again.",
        )
        .with_details(serde_json::json!({ "reason": "device" })));
    };
    let stale = last_seen
        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .is_none_or(|t| t.with_timezone(&Utc) + chrono::Duration::seconds(60) <= now);
    if stale {
        conn.execute(
            "UPDATE devices SET last_seen_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), device.id],
        )?;
    }
    Ok(device)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub short_code: Option<String>,
    pub app_version: Option<String>,
    pub paired_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
    /// Offline sales waiting for review (oversells, changed prices).
    pub open_exceptions: i64,
}

pub fn list(ctx: &Ctx<'_>) -> AppResult<Vec<DeviceSummary>> {
    ctx.require("devices.manage")?;
    let rows = ctx
        .conn
        .prepare(
            "SELECT d.id, d.name, d.mode, d.short_code, d.app_version, d.paired_at, d.last_seen_at, d.revoked_at,
                (SELECT count(*) FROM sync_exceptions e WHERE e.device_id = d.id AND e.resolved_at IS NULL)
             FROM devices d WHERE d.deleted_at IS NULL ORDER BY d.revoked_at IS NOT NULL, d.paired_at",
        )?
        .query_map([], |r| {
            Ok(DeviceSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                mode: r.get(2)?,
                short_code: r.get(3)?,
                app_version: r.get(4)?,
                paired_at: r.get(5)?,
                last_seen_at: r.get(6)?,
                revoked_at: r.get(7)?,
                open_exceptions: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Removes a device: its credential stops working, its sessions end and its unused leased
/// numbers are released. Offline sales it already queued are refused from then on.
pub fn revoke(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("devices.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let changed = tx.execute(
            "UPDATE devices SET revoked_at = ?1, token_hash = NULL, updated_at = ?1, updated_by = ?2
             WHERE id = ?3 AND revoked_at IS NULL AND deleted_at IS NULL",
            params![now, actor.user_id, input.id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("Device"));
        }
        tx.execute(
            "UPDATE sessions SET revoked_at = ?1 WHERE device_id = ?2 AND revoked_at IS NULL",
            params![now, input.id],
        )?;
        tx.execute(
            "UPDATE number_series_leases SET released_at = ?1 WHERE device_id = ?2 AND released_at IS NULL",
            params![now, input.id],
        )?;
        AuditEntry::new("DEVICE_REVOKED", "devices", Some(&input.id))
            .by(actor)
            .write(tx, &now)
    })
}
