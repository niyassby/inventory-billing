//! Append-only audit trail for sensitive actions (Master Prompt §7): logins, price changes,
//! discounts and overrides, voids, deletes, stock adjustments, settings, users.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::domain::ids::new_id;
use crate::error::AppResult;

pub struct AuditEntry<'a> {
    pub user_id: Option<&'a str>,
    pub device_id: Option<&'a str>,
    pub approved_by: Option<&'a str>,
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<&'a str>,
    pub summary: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl<'a> AuditEntry<'a> {
    pub fn new(action: &'a str, entity_type: &'a str, entity_id: Option<&'a str>) -> Self {
        Self {
            user_id: None,
            device_id: None,
            approved_by: None,
            action,
            entity_type,
            entity_id,
            summary: None,
            before: None,
            after: None,
        }
    }

    pub fn by(mut self, actor: &'a super::Actor) -> Self {
        self.user_id = Some(&actor.user_id);
        self.device_id = actor.device_id.as_deref();
        self
    }

    pub fn approved_by(mut self, approver: Option<&'a str>) -> Self {
        self.approved_by = approver;
        self
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn before(mut self, value: Value) -> Self {
        self.before = Some(value);
        self
    }

    pub fn after(mut self, value: Value) -> Self {
        self.after = Some(value);
        self
    }

    pub fn write(self, conn: &Connection, at: &str) -> AppResult<()> {
        conn.execute(
            "INSERT INTO audit_log (id, occurred_at, user_id, device_id, approved_by, action, entity_type,
                entity_id, summary, before_json, after_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                new_id(),
                at,
                self.user_id,
                self.device_id,
                self.approved_by,
                self.action,
                self.entity_type,
                self.entity_id,
                self.summary,
                self.before.map(|v| v.to_string()),
                self.after.map(|v| v.to_string()),
            ],
        )?;
        Ok(())
    }
}
