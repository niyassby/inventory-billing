//! Who is acting, with which permissions, and (optionally) which manager approved.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::json;

use crate::error::{AppError, AppResult, ErrorCode};

#[derive(Debug, Clone)]
pub struct Actor {
    pub user_id: String,
    pub name: String,
    pub role_code: String,
    pub permissions: HashSet<String>,
    /// Largest discount (ppm of the bill) this user may give without approval.
    pub max_discount_rate: i64,
    pub session_id: Option<String>,
    pub device_id: Option<String>,
}

impl Actor {
    pub fn can(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    pub fn require(&self, permission: &str) -> AppResult<()> {
        if self.can(permission) {
            Ok(())
        } else {
            Err(AppError::forbidden(permission))
        }
    }
}

/// A manager who approved this request on the same screen (PIN or password verified by the API
/// layer before the transaction starts).
#[derive(Debug, Clone)]
pub struct Approver {
    pub user_id: String,
    pub name: String,
    pub permissions: HashSet<String>,
    pub max_discount_rate: i64,
}

pub struct Ctx<'a> {
    pub conn: &'a mut Connection,
    pub actor: &'a Actor,
    pub approver: Option<&'a Approver>,
    pub now: DateTime<Utc>,
}

impl Ctx<'_> {
    pub fn require(&self, permission: &str) -> AppResult<()> {
        self.actor.require(permission)
    }

    pub fn now_str(&self) -> String {
        self.now.to_rfc3339()
    }

    /// The actor's own permission, or a present approver's. Returns who approved (if anyone) so it
    /// can be written to the audit log. Without either: APPROVAL_REQUIRED, which the UI answers by
    /// asking a manager for their PIN and retrying the same request.
    pub fn approve(&self, permission: &str, reason: &str) -> AppResult<Option<String>> {
        approve(self.actor, self.approver, permission, reason)
    }
}

pub fn approve(
    actor: &Actor,
    approver: Option<&Approver>,
    permission: &str,
    reason: &str,
) -> AppResult<Option<String>> {
    if actor.can(permission) {
        return Ok(None);
    }
    if let Some(approver) = approver {
        if approver.permissions.contains(permission)
            && approver.permissions.contains("approvals.grant")
        {
            return Ok(Some(approver.user_id.clone()));
        }
        return Err(AppError::new(
            ErrorCode::Forbidden,
            format!("{} can't approve this.", approver.name),
        )
        .with_details(json!({ "permission": permission, "reason": reason })));
    }
    Err(AppError::new(
        ErrorCode::ApprovalRequired,
        format!("Manager approval needed: {reason}"),
    )
    .with_details(json!({ "permission": permission, "reason": reason })))
}
