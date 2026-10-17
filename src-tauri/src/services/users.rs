//! Users and roles. Permissions are checked here (backend), never only in the UI (§9).

use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::auth::validate_password;
use super::common::{optional_email, optional_phone, rate, required};
use super::settings;
use super::{write, Ctx};
use crate::auth as crypto;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: String,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub role_id: String,
    pub role_code: String,
    pub role_name: String,
    pub is_active: bool,
    pub must_change_password: bool,
    pub has_pin: bool,
    pub last_login_at: Option<String>,
    pub locked_until: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserInput {
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub role_id: String,
    /// Temporary password; the user must change it at first sign-in.
    pub temporary_password: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserInput {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub role_id: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordInput {
    pub id: String,
    pub temporary_password: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdInput {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSummary {
    pub id: String,
    pub code: String,
    pub name: String,
    pub is_system: bool,
    pub max_discount_rate: i64,
    pub permissions: Vec<String>,
    pub user_count: i64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleInput {
    /// None to create a role.
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    pub max_discount_rate: i64,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionInfo {
    pub code: String,
    pub description: String,
}

const USER_SELECT: &str = "SELECT u.id, u.name, u.email, u.phone, r.id, r.code, r.name, u.is_active, u.must_change_password,
    u.pin_hash IS NOT NULL, u.last_login_at, u.locked_until, u.created_at
    FROM users u JOIN roles r ON r.id = u.role_id";

fn map_user(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserSummary> {
    Ok(UserSummary {
        id: r.get(0)?,
        name: r.get(1)?,
        email: r.get(2)?,
        phone: r.get(3)?,
        role_id: r.get(4)?,
        role_code: r.get(5)?,
        role_name: r.get(6)?,
        is_active: r.get(7)?,
        must_change_password: r.get(8)?,
        has_pin: r.get(9)?,
        last_login_at: r.get(10)?,
        locked_until: r.get(11)?,
        created_at: r.get(12)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<UserSummary> {
    conn.query_row(
        &format!("{USER_SELECT} WHERE u.id = ?1 AND u.deleted_at IS NULL"),
        [id],
        map_user,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The user"))
}

pub fn list(ctx: &Ctx<'_>) -> AppResult<Vec<UserSummary>> {
    ctx.require("users.manage")?;
    let mut stmt = ctx.conn.prepare(&format!(
        "{USER_SELECT} WHERE u.deleted_at IS NULL ORDER BY lower(u.name)"
    ))?;
    let users = stmt.query_map([], map_user)?.collect::<Result<_, _>>()?;
    Ok(users)
}

fn role_exists(conn: &Connection, role_id: &str) -> AppResult<String> {
    conn.query_row(
        "SELECT code FROM roles WHERE id = ?1 AND deleted_at IS NULL",
        [role_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::field("roleId", "Choose an existing role."))
}

/// Refuses changes that would leave the business without an active owner.
fn ensure_another_owner(conn: &Connection, excluding_user: &str) -> AppResult<()> {
    let owners: i64 = conn.query_row(
        "SELECT count(*) FROM users u JOIN roles r ON r.id = u.role_id
         WHERE r.code = 'owner' AND u.is_active = 1 AND u.deleted_at IS NULL AND u.id <> ?1",
        [excluding_user],
        |r| r.get(0),
    )?;
    if owners == 0 {
        return Err(AppError::conflict(
            "The business must keep at least one active owner.",
        ));
    }
    Ok(())
}

pub fn create(ctx: &mut Ctx<'_>, input: &CreateUserInput) -> AppResult<UserSummary> {
    ctx.require("users.manage")?;
    let name = required("name", &input.name, 120)?;
    let email = optional_email("email", Some(&input.email))?
        .ok_or_else(|| AppError::field("email", "This field is required."))?;
    let phone = optional_phone("phone", input.phone.as_deref())?;
    role_exists(ctx.conn, &input.role_id)?;
    validate_password(
        &input.temporary_password,
        &email,
        &settings::load(ctx.conn)?.security,
    )?;
    let hash = crypto::hash_password(&input.temporary_password).map_err(AppError::internal)?;
    let now = ctx.now_str();
    let id = new_id();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let taken: i64 = tx.query_row(
            "SELECT count(*) FROM users WHERE lower(email) = ?1 AND deleted_at IS NULL",
            [&email],
            |r| r.get(0),
        )?;
        if taken > 0 {
            return Err(AppError::field(
                "email",
                "A user with this email already exists.",
            ));
        }
        tx.execute(
            "INSERT INTO users (id, name, email, phone, password_hash, role_id, must_change_password, created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?8, ?8)",
            params![id, name, email, phone, hash, input.role_id, now, actor.user_id],
        )?;
        AuditEntry::new("USER_CREATED", "users", Some(&id))
            .by(actor)
            .after(json!({ "name": name, "email": email, "roleId": input.role_id }))
            .write(tx, &now)?;
        get(tx, &id)
    })
}

pub fn update(ctx: &mut Ctx<'_>, input: &UpdateUserInput) -> AppResult<UserSummary> {
    ctx.require("users.manage")?;
    let name = required("name", &input.name, 120)?;
    let phone = optional_phone("phone", input.phone.as_deref())?;
    let new_role_code = role_exists(ctx.conn, &input.role_id)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let before = get(tx, &input.id)?;
        let stops_being_owner =
            before.role_code == "owner" && (new_role_code != "owner" || !input.is_active);
        if stops_being_owner {
            ensure_another_owner(tx, &input.id)?;
        }
        tx.execute(
            "UPDATE users SET name = ?1, phone = ?2, role_id = ?3, is_active = ?4, updated_at = ?5, updated_by = ?6 WHERE id = ?7",
            params![name, phone, input.role_id, input.is_active, now, actor.user_id, input.id],
        )?;
        if !input.is_active {
            tx.execute(
                "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL",
                params![now, input.id],
            )?;
        }
        let after = get(tx, &input.id)?;
        AuditEntry::new("USER_UPDATED", "users", Some(&input.id))
            .by(actor)
            .before(json!({ "name": before.name, "roleId": before.role_id, "isActive": before.is_active }))
            .after(json!({ "name": after.name, "roleId": after.role_id, "isActive": after.is_active }))
            .write(tx, &now)?;
        Ok(after)
    })
}

pub fn reset_password(ctx: &mut Ctx<'_>, input: &ResetPasswordInput) -> AppResult<()> {
    ctx.require("users.manage")?;
    let user = get(ctx.conn, &input.id)?;
    validate_password(
        &input.temporary_password,
        &user.email,
        &settings::load(ctx.conn)?.security,
    )?;
    let hash = crypto::hash_password(&input.temporary_password).map_err(AppError::internal)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "UPDATE users SET password_hash = ?1, must_change_password = 1, failed_login_count = 0, locked_until = NULL,
               updated_at = ?2, updated_by = ?3 WHERE id = ?4",
            params![hash, now, actor.user_id, input.id],
        )?;
        tx.execute(
            "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL",
            params![now, input.id],
        )?;
        AuditEntry::new("PASSWORD_RESET", "users", Some(&input.id))
            .by(actor)
            .write(tx, &now)
    })
}

pub fn delete(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("users.manage")?;
    if input.id == ctx.actor.user_id {
        return Err(AppError::conflict("You can't delete your own account."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let user = get(tx, &input.id)?;
        if user.role_code == "owner" {
            ensure_another_owner(tx, &input.id)?;
        }
        tx.execute(
            "UPDATE users SET deleted_at = ?1, is_active = 0, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        tx.execute(
            "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL",
            params![now, input.id],
        )?;
        AuditEntry::new("USER_DELETED", "users", Some(&input.id))
            .by(actor)
            .summary(user.email)
            .write(tx, &now)
    })
}

pub fn list_permissions(ctx: &Ctx<'_>) -> AppResult<Vec<PermissionInfo>> {
    ctx.require("users.manage")?;
    let mut stmt = ctx
        .conn
        .prepare("SELECT code, description FROM permissions ORDER BY code")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(PermissionInfo {
                code: r.get(0)?,
                description: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn role(conn: &Connection, id: &str) -> AppResult<RoleSummary> {
    let mut role = conn
        .query_row(
            "SELECT id, code, name, is_system, max_discount_rate,
               (SELECT count(*) FROM users u WHERE u.role_id = roles.id AND u.deleted_at IS NULL)
             FROM roles WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| {
                Ok(RoleSummary {
                    id: r.get(0)?,
                    code: r.get(1)?,
                    name: r.get(2)?,
                    is_system: r.get(3)?,
                    max_discount_rate: r.get(4)?,
                    permissions: Vec::new(),
                    user_count: r.get(5)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("The role"))?;
    role.permissions = conn
        .prepare("SELECT permission_code FROM role_permissions WHERE role_id = ?1 ORDER BY permission_code")?
        .query_map([id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(role)
}

pub fn list_roles(ctx: &Ctx<'_>) -> AppResult<Vec<RoleSummary>> {
    ctx.require("users.manage")?;
    let ids: Vec<String> = ctx
        .conn
        .prepare("SELECT id FROM roles WHERE deleted_at IS NULL ORDER BY is_system DESC, name")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    ids.iter().map(|id| role(ctx.conn, id)).collect()
}

pub fn save_role(ctx: &mut Ctx<'_>, input: &RoleInput) -> AppResult<RoleSummary> {
    ctx.require("users.manage")?;
    let name = required("name", &input.name, 60)?;
    let code = required("code", &input.code, 40)?.to_lowercase();
    if !code
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::field(
            "code",
            "Use lowercase letters, digits and _.",
        ));
    }
    let max_discount_rate = rate("maxDiscountRate", input.max_discount_rate)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        for p in &input.permissions {
            let exists: i64 = tx.query_row(
                "SELECT count(*) FROM permissions WHERE code = ?1",
                [p],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(AppError::field(
                    "permissions",
                    format!("Unknown permission '{p}'."),
                ));
            }
        }
        let id = match &input.id {
            Some(id) => {
                let existing = role(tx, id)?;
                if existing.code == "owner" {
                    return Err(AppError::conflict(
                        "The owner role always has every permission and can't be edited.",
                    ));
                }
                tx.execute(
                    "UPDATE roles SET name = ?1, max_discount_rate = ?2, updated_at = ?3, updated_by = ?4 WHERE id = ?5",
                    params![name, max_discount_rate, now, actor.user_id, id],
                )?;
                tx.execute("DELETE FROM role_permissions WHERE role_id = ?1", [id])?;
                id.clone()
            }
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO roles (id, code, name, max_discount_rate, created_at, updated_at, created_by, updated_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?6)",
                    params![id, code, name, max_discount_rate, now, actor.user_id],
                )?;
                id
            }
        };
        for p in &input.permissions {
            tx.execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_code) VALUES (?1, ?2)",
                params![id, p],
            )?;
        }
        let saved = role(tx, &id)?;
        AuditEntry::new("ROLE_SAVED", "roles", Some(&id))
            .by(actor)
            .after(json!({ "name": saved.name, "permissions": saved.permissions, "maxDiscountRate": saved.max_discount_rate }))
            .write(tx, &now)?;
        Ok(saved)
    })
}

pub fn delete_role(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("users.manage")?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let r = role(tx, &input.id)?;
        if r.is_system {
            return Err(AppError::conflict("Built-in roles can't be deleted."));
        }
        if r.user_count > 0 {
            return Err(AppError::conflict(
                "Move this role's users to another role first.",
            ));
        }
        tx.execute(
            "UPDATE roles SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("ROLE_DELETED", "roles", Some(&input.id))
            .by(actor)
            .summary(r.name)
            .write(tx, &now)
    })
}
