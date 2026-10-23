//! Notices from background jobs (low stock, failed backup, data check) for the dashboard.

use rusqlite::params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::users::IdInput;
use super::Ctx;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub kind: String,
    /// "info", "warning" or "error".
    pub severity: String,
    pub title: String,
    pub body: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub created_at: String,
    pub read_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationQuery {
    #[serde(default)]
    pub include_read: bool,
}

/// Backup and data-check notices are only for people who can act on them.
fn visible_kinds(ctx: &Ctx<'_>) -> Vec<&'static str> {
    let mut kinds = vec!["low_stock"];
    if ctx.actor.can("backup.manage") {
        kinds.extend(["backup_failed", "backup_mirror_failed"]);
    }
    if ctx.actor.can("settings.manage") {
        kinds.push("integrity");
    }
    kinds
}

pub fn list(ctx: &Ctx<'_>, query: &NotificationQuery) -> AppResult<Vec<Notification>> {
    ctx.require("dashboard.view")?;
    let kinds = visible_kinds(ctx);
    let placeholders = vec!["?"; kinds.len()].join(", ");
    let read = if query.include_read {
        ""
    } else {
        "AND read_at IS NULL"
    };
    let rows = ctx
        .conn
        .prepare(&format!(
            "SELECT id, kind, severity, title, body, entity_type, entity_id, created_at, read_at FROM notifications
             WHERE kind IN ({placeholders}) {read} ORDER BY created_at DESC LIMIT 100"
        ))?
        .query_map(rusqlite::params_from_iter(kinds.iter()), |r| {
            Ok(Notification {
                id: r.get(0)?,
                kind: r.get(1)?,
                severity: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                entity_type: r.get(5)?,
                entity_id: r.get(6)?,
                created_at: r.get(7)?,
                read_at: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn mark_read(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("dashboard.view")?;
    let kinds = visible_kinds(ctx);
    let kind: Option<String> = ctx
        .conn
        .query_row(
            "SELECT kind FROM notifications WHERE id = ?1",
            [&input.id],
            |r| r.get(0),
        )
        .ok();
    if !kind.is_some_and(|k| kinds.contains(&k.as_str())) {
        return Err(AppError::not_found("Notification"));
    }
    ctx.conn.execute(
        "UPDATE notifications SET read_at = ?1 WHERE id = ?2 AND read_at IS NULL",
        params![ctx.now_str(), input.id],
    )?;
    Ok(())
}

/// A notice raised by the app itself. `key` de-duplicates: while a notice with the same key is
/// unread, raising it again does nothing.
pub(crate) struct Notice<'a> {
    pub kind: &'a str,
    pub severity: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub key: &'a str,
    pub entity: Option<(&'a str, &'a str)>,
}

/// Raises a notice; returns whether a new one was written.
pub(crate) fn raise(conn: &rusqlite::Connection, n: &Notice<'_>, now: &str) -> AppResult<bool> {
    let inserted = conn.execute(
        "INSERT INTO notifications (id, kind, severity, title, body, entity_type, entity_id, dedupe_key, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT DO NOTHING",
        rusqlite::params![
            crate::domain::ids::new_id(),
            n.kind,
            n.severity,
            n.title,
            n.body,
            n.entity.map(|e| e.0),
            n.entity.map(|e| e.1),
            n.key,
            now
        ],
    )?;
    Ok(inserted > 0)
}
