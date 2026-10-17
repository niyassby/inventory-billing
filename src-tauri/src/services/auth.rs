//! Sign-in, sessions and approvals.
//!
//! * Passwords and PINs: Argon2id (crate::auth). Hashing never happens inside a write transaction.
//! * Brute force: an in-memory rate limit per e-mail / device, plus a persistent lockout after
//!   `max_failed_logins` failures. Unknown e-mails cost the same time as known ones.
//! * Sessions: random 256-bit token, stored as SHA-256; absolute expiry and an idle timeout.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::context::{Actor, Approver};
use super::settings::{self, SecuritySettings};
use super::write;
use crate::app::RateLimiter;
use crate::auth as crypto;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinLoginInput {
    pub email: String,
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionUser {
    pub id: String,
    pub name: String,
    pub email: String,
    pub role_code: String,
    pub role_name: String,
    pub permissions: Vec<String>,
    pub max_discount_rate: i64,
    pub has_pin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub token: String,
    pub expires_at: String,
    pub must_change_password: bool,
    pub user: SessionUser,
}

/// A manager's credentials typed on the cashier's screen to approve one action.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalInput {
    pub email: String,
    /// Either the approver's PIN or their password.
    pub pin: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordInput {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPinInput {
    pub current_password: String,
    pub pin: String,
}

struct UserAuthRow {
    id: String,
    name: String,
    password_hash: String,
    pin_hash: Option<String>,
    is_active: bool,
    must_change_password: bool,
    failed_login_count: i64,
    locked_until: Option<String>,
}

fn invalid_credentials() -> AppError {
    AppError::new(ErrorCode::Unauthorized, "Email or password is incorrect.")
}

/// A real Argon2 hash used to spend the same time on unknown e-mails as on known ones.
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| crypto::hash_password("timing-equaliser").unwrap_or_default())
}

pub fn validate_password(password: &str, email: &str, policy: &SecuritySettings) -> AppResult<()> {
    if password.chars().count() < policy.min_password_length {
        return Err(AppError::field(
            "password",
            format!("Use at least {} characters.", policy.min_password_length),
        ));
    }
    if password.chars().count() > 128 {
        return Err(AppError::field("password", "Use at most 128 characters."));
    }
    if !password.chars().any(char::is_alphabetic) || !password.chars().any(|c| c.is_ascii_digit()) {
        return Err(AppError::field(
            "password",
            "Include at least one letter and one number.",
        ));
    }
    if password.eq_ignore_ascii_case(email) || password.to_lowercase() == "password123" {
        return Err(AppError::field(
            "password",
            "Choose a less guessable password.",
        ));
    }
    Ok(())
}

pub fn validate_pin(pin: &str) -> AppResult<()> {
    if (4..=6).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit()) {
        let same = pin.chars().all(|c| pin.starts_with(c));
        if same || "0123456789".contains(pin) || "9876543210".contains(pin) {
            return Err(AppError::field("pin", "Choose a less guessable PIN."));
        }
        Ok(())
    } else {
        Err(AppError::field("pin", "PIN must be 4 to 6 digits."))
    }
}

fn load_user_by_email(conn: &Connection, email: &str) -> AppResult<Option<UserAuthRow>> {
    Ok(conn
        .query_row(
            "SELECT id, name, password_hash, pin_hash, is_active, must_change_password, failed_login_count, locked_until
             FROM users WHERE lower(email) = lower(?1) AND deleted_at IS NULL",
            [email.trim()],
            |r| {
                Ok(UserAuthRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    password_hash: r.get(2)?,
                    pin_hash: r.get(3)?,
                    is_active: r.get(4)?,
                    must_change_password: r.get(5)?,
                    failed_login_count: r.get(6)?,
                    locked_until: r.get(7)?,
                })
            },
        )
        .optional()?)
}

fn check_lock(user: &UserAuthRow, now: DateTime<Utc>) -> AppResult<()> {
    if let Some(until) = user
        .locked_until
        .as_deref()
        .and_then(|u| DateTime::parse_from_rfc3339(u).ok())
    {
        if until > now {
            return Err(AppError::new(
                ErrorCode::LockedOut,
                "Too many failed attempts. The account is locked for a while.",
            )
            .with_details(json!({ "lockedUntil": until.to_rfc3339() })));
        }
    }
    Ok(())
}

/// Records a failed attempt; locks the account when the limit is reached.
fn record_failure(
    conn: &mut Connection,
    user: &UserAuthRow,
    policy: &SecuritySettings,
    now: DateTime<Utc>,
    action: &str,
) -> AppResult<AppError> {
    let failures = user.failed_login_count + 1;
    let lock = failures >= policy.max_failed_logins;
    let now_str = now.to_rfc3339();
    write(conn, |tx| {
        if lock {
            let until = (now + chrono::Duration::minutes(policy.lockout_minutes)).to_rfc3339();
            tx.execute(
                "UPDATE users SET failed_login_count = 0, locked_until = ?1 WHERE id = ?2",
                params![until, user.id],
            )?;
        } else {
            tx.execute(
                "UPDATE users SET failed_login_count = ?1 WHERE id = ?2",
                params![failures, user.id],
            )?;
        }
        let mut entry = AuditEntry::new(action, "users", Some(&user.id)).summary(if lock {
            "account locked"
        } else {
            "wrong credentials"
        });
        entry.user_id = Some(&user.id);
        entry.write(tx, &now_str)
    })?;
    Ok(if lock {
        AppError::new(
            ErrorCode::LockedOut,
            "Too many failed attempts. The account is locked for a while.",
        )
    } else {
        invalid_credentials()
    })
}

pub fn session_user(conn: &Connection, user_id: &str) -> AppResult<SessionUser> {
    let (id, name, email, role_id, role_code, role_name, max_discount_rate, has_pin): (String, String, String, String, String, String, i64, bool) =
        conn.query_row(
            "SELECT u.id, u.name, u.email, r.id, r.code, r.name, r.max_discount_rate, u.pin_hash IS NOT NULL
             FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = ?1",
            [user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )?;
    let permissions = conn
        .prepare("SELECT permission_code FROM role_permissions WHERE role_id = ?1 ORDER BY permission_code")?
        .query_map([&role_id], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(SessionUser {
        id,
        name,
        email,
        role_code,
        role_name,
        permissions,
        max_discount_rate,
        has_pin,
    })
}

fn create_session(
    conn: &mut Connection,
    user: &UserAuthRow,
    policy: &SecuritySettings,
    now: DateTime<Utc>,
    action: &str,
) -> AppResult<AuthSession> {
    let token = crypto::generate_session_token();
    let expires = now + chrono::Duration::hours(policy.session_hours);
    let now_str = now.to_rfc3339();
    write(conn, |tx| {
        tx.execute(
            "UPDATE users SET failed_login_count = 0, locked_until = NULL, last_login_at = ?1 WHERE id = ?2",
            params![now_str, user.id],
        )?;
        tx.execute(
            "INSERT INTO sessions (id, user_id, token_hash, created_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5, ?4)",
            params![new_id(), user.id, crypto::hash_token(&token), now_str, expires.to_rfc3339()],
        )?;
        let mut entry = AuditEntry::new(action, "users", Some(&user.id));
        entry.user_id = Some(&user.id);
        entry.write(tx, &now_str)
    })?;
    Ok(AuthSession {
        token,
        expires_at: expires.to_rfc3339(),
        must_change_password: user.must_change_password,
        user: session_user(conn, &user.id)?,
    })
}

pub fn login(
    conn: &mut Connection,
    limiter: &RateLimiter,
    now: DateTime<Utc>,
    input: &LoginInput,
) -> AppResult<AuthSession> {
    let email = input.email.trim().to_lowercase();
    limiter.check(&format!("login:{email}"), 10, Duration::from_secs(300))?;
    let policy = settings::load(conn)?.security;
    let Some(user) = load_user_by_email(conn, &email)? else {
        let _ = crypto::verify_password(dummy_hash(), &input.password);
        return Err(invalid_credentials());
    };
    check_lock(&user, now)?;
    if !crypto::verify_password(&user.password_hash, &input.password) {
        return Err(record_failure(conn, &user, &policy, now, "LOGIN_FAILED")?);
    }
    if !user.is_active {
        return Err(AppError::new(
            ErrorCode::Unauthorized,
            "This account has been deactivated.",
        ));
    }
    limiter.reset(&format!("login:{email}"));
    create_session(conn, &user, &policy, now, "LOGIN")
}

/// Quick cashier switch with a PIN (only when enabled in Settings → Security).
pub fn pin_login(
    conn: &mut Connection,
    limiter: &RateLimiter,
    now: DateTime<Utc>,
    input: &PinLoginInput,
) -> AppResult<AuthSession> {
    let email = input.email.trim().to_lowercase();
    limiter.check(&format!("pin:{email}"), 5, Duration::from_secs(300))?;
    let policy = settings::load(conn)?.security;
    if !policy.pin_login_enabled {
        return Err(AppError::new(
            ErrorCode::Forbidden,
            "PIN sign-in is turned off.",
        ));
    }
    let user = load_user_by_email(conn, &email)?
        .ok_or_else(|| AppError::new(ErrorCode::Unauthorized, "Email or PIN is incorrect."))?;
    check_lock(&user, now)?;
    let Some(pin_hash) = user.pin_hash.as_deref() else {
        return Err(AppError::new(
            ErrorCode::Unauthorized,
            "This user has no PIN. Sign in with the password.",
        ));
    };
    if !crypto::verify_password(pin_hash, &input.pin) {
        let _ = record_failure(conn, &user, &policy, now, "PIN_LOGIN_FAILED")?;
        return Err(AppError::new(
            ErrorCode::Unauthorized,
            "Email or PIN is incorrect.",
        ));
    }
    if !user.is_active {
        return Err(AppError::new(
            ErrorCode::Unauthorized,
            "This account has been deactivated.",
        ));
    }
    create_session(conn, &user, &policy, now, "PIN_LOGIN")
}

pub struct Authenticated {
    pub actor: Actor,
    pub must_change_password: bool,
}

/// Resolves a session token to an actor with permissions, enforcing expiry and idle timeout.
pub fn authenticate(
    conn: &Connection,
    token: &str,
    now: DateTime<Utc>,
) -> AppResult<Authenticated> {
    let row: Option<(String, String, String, String, bool)> = conn
        .query_row(
            "SELECT s.id, s.user_id, s.expires_at, coalesce(s.last_seen_at, s.created_at), u.must_change_password
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1 AND s.revoked_at IS NULL AND u.is_active = 1 AND u.deleted_at IS NULL",
            [crypto::hash_token(token)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((session_id, user_id, expires_at, last_seen, must_change)) = row else {
        return Err(AppError::unauthorized());
    };
    let parse = |t: &str| {
        DateTime::parse_from_rfc3339(t)
            .map(|d| d.with_timezone(&Utc))
            .map_err(AppError::internal)
    };
    let policy = settings::load(conn)?.security;
    let idle_limit = parse(&last_seen)? + chrono::Duration::minutes(policy.idle_timeout_minutes);
    if parse(&expires_at)? <= now || idle_limit <= now {
        conn.execute(
            "UPDATE sessions SET revoked_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), session_id],
        )?;
        return Err(AppError::unauthorized());
    }
    // Touch at most once a minute to keep reads cheap.
    if parse(&last_seen)? + chrono::Duration::seconds(60) <= now {
        conn.execute(
            "UPDATE sessions SET last_seen_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), session_id],
        )?;
    }
    let user = session_user(conn, &user_id)?;
    Ok(Authenticated {
        actor: Actor {
            user_id: user.id,
            name: user.name,
            role_code: user.role_code,
            permissions: user.permissions.into_iter().collect::<HashSet<_>>(),
            max_discount_rate: user.max_discount_rate,
            session_id: Some(session_id),
            device_id: None,
        },
        must_change_password: must_change,
    })
}

pub fn logout(conn: &Connection, token: &str, now: DateTime<Utc>) -> AppResult<()> {
    conn.execute(
        "UPDATE sessions SET revoked_at = ?1 WHERE token_hash = ?2 AND revoked_at IS NULL",
        params![now.to_rfc3339(), crypto::hash_token(token)],
    )?;
    Ok(())
}

/// Verifies a manager's credentials for an on-screen approval.
pub fn verify_approver(
    conn: &mut Connection,
    limiter: &RateLimiter,
    now: DateTime<Utc>,
    input: &ApprovalInput,
) -> AppResult<Approver> {
    let email = input.email.trim().to_lowercase();
    limiter.check(&format!("approve:{email}"), 5, Duration::from_secs(300))?;
    let policy = settings::load(conn)?.security;
    let denied = || AppError::new(ErrorCode::Forbidden, "Approval credentials are incorrect.");
    let user = load_user_by_email(conn, &email)?.ok_or_else(denied)?;
    check_lock(&user, now)?;
    let ok = match (&input.pin, &input.password, &user.pin_hash) {
        (Some(pin), _, Some(hash)) => crypto::verify_password(hash, pin),
        (_, Some(password), _) => crypto::verify_password(&user.password_hash, password),
        _ => false,
    };
    if !ok || !user.is_active {
        let _ = record_failure(conn, &user, &policy, now, "APPROVAL_FAILED")?;
        return Err(denied());
    }
    let session_user = session_user(conn, &user.id)?;
    Ok(Approver {
        user_id: user.id,
        name: user.name,
        permissions: session_user.permissions.into_iter().collect(),
        max_discount_rate: session_user.max_discount_rate,
    })
}

pub fn change_password(
    conn: &mut Connection,
    actor: &Actor,
    now: DateTime<Utc>,
    input: &ChangePasswordInput,
) -> AppResult<()> {
    let (email, hash): (String, String) = conn.query_row(
        "SELECT email, password_hash FROM users WHERE id = ?1",
        [&actor.user_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if !crypto::verify_password(&hash, &input.current_password) {
        return Err(AppError::field(
            "currentPassword",
            "The current password is incorrect.",
        ));
    }
    validate_password(&input.new_password, &email, &settings::load(conn)?.security)?;
    if crypto::verify_password(&hash, &input.new_password) {
        return Err(AppError::field(
            "newPassword",
            "Choose a password different from the current one.",
        ));
    }
    let new_hash = crypto::hash_password(&input.new_password).map_err(AppError::internal)?;
    let now_str = now.to_rfc3339();
    write(conn, |tx| {
        tx.execute(
            "UPDATE users SET password_hash = ?1, must_change_password = 0, password_changed_at = ?2, updated_at = ?2, updated_by = ?3 WHERE id = ?3",
            params![new_hash, now_str, actor.user_id],
        )?;
        // Other sessions of this user end; the current one stays.
        tx.execute(
            "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL AND id IS NOT ?3",
            params![now_str, actor.user_id, actor.session_id],
        )?;
        AuditEntry::new("PASSWORD_CHANGED", "users", Some(&actor.user_id))
            .by(actor)
            .write(tx, &now_str)
    })
}

pub fn set_pin(
    conn: &mut Connection,
    actor: &Actor,
    now: DateTime<Utc>,
    input: &SetPinInput,
) -> AppResult<()> {
    let hash: String = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        [&actor.user_id],
        |r| r.get(0),
    )?;
    if !crypto::verify_password(&hash, &input.current_password) {
        return Err(AppError::field(
            "currentPassword",
            "The current password is incorrect.",
        ));
    }
    validate_pin(&input.pin)?;
    let pin_hash = crypto::hash_password(&input.pin).map_err(AppError::internal)?;
    let now_str = now.to_rfc3339();
    write(conn, |tx| {
        tx.execute(
            "UPDATE users SET pin_hash = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?3",
            params![pin_hash, now_str, actor.user_id],
        )?;
        AuditEntry::new("PIN_SET", "users", Some(&actor.user_id))
            .by(actor)
            .write(tx, &now_str)
    })
}
