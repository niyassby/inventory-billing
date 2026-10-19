//! The one error type every v2 command returns. Serialises to `{ code, message, details }`, which
//! the frontend turns into an `ApiError` (src/api/errors.ts). Codes mirror `ERROR_CODES` in
//! packages/shared/src/constants.ts; a test keeps them in sync.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    ApprovalRequired,
    Validation,
    NotFound,
    Conflict,
    InsufficientStock,
    TotalsMismatch,
    DocumentFinalized,
    RateLimited,
    LockedOut,
    HubOffline,
    SetupRequired,
    PasswordChangeRequired,
    Internal,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 15] = [
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::ApprovalRequired,
        ErrorCode::Validation,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::InsufficientStock,
        ErrorCode::TotalsMismatch,
        ErrorCode::DocumentFinalized,
        ErrorCode::RateLimited,
        ErrorCode::LockedOut,
        ErrorCode::HubOffline,
        ErrorCode::SetupRequired,
        ErrorCode::PasswordChangeRequired,
        ErrorCode::Internal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Unauthorized => "UNAUTHORIZED",
            ErrorCode::Forbidden => "FORBIDDEN",
            ErrorCode::ApprovalRequired => "APPROVAL_REQUIRED",
            ErrorCode::Validation => "VALIDATION",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::Conflict => "CONFLICT",
            ErrorCode::InsufficientStock => "INSUFFICIENT_STOCK",
            ErrorCode::TotalsMismatch => "TOTALS_MISMATCH",
            ErrorCode::DocumentFinalized => "DOCUMENT_FINALIZED",
            ErrorCode::RateLimited => "RATE_LIMITED",
            ErrorCode::LockedOut => "LOCKED_OUT",
            ErrorCode::HubOffline => "HUB_OFFLINE",
            ErrorCode::SetupRequired => "SETUP_REQUIRED",
            ErrorCode::PasswordChangeRequired => "PASSWORD_CHANGE_REQUIRED",
            ErrorCode::Internal => "INTERNAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message)
    }

    /// Validation error tied to one input field (the UI shows it next to the field).
    pub fn field(field: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self::new(ErrorCode::Validation, message.clone())
            .with_details(json!({ "field": field, "message": message }))
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(ErrorCode::NotFound, format!("{what} was not found"))
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            ErrorCode::Unauthorized,
            "Your session has ended. Please sign in again.",
        )
    }

    pub fn forbidden(permission: &str) -> Self {
        Self::new(
            ErrorCode::Forbidden,
            "You don't have permission to do this.",
        )
        .with_details(json!({ "permission": permission }))
    }

    /// Unexpected failure. The cause goes to the log with an incident id; the user sees only the id.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        let incident = crate::domain::ids::new_id();
        tracing::error!(incident = %incident, "internal error: {cause}");
        Self::new(
            ErrorCode::Internal,
            "Something went wrong. Please try again.",
        )
        .with_details(json!({ "incident": incident }))
    }

    /// A poisoned mutex (a thread panicked while holding it).
    pub fn internal_lock<T>(_: std::sync::PoisonError<T>) -> Self {
        Self::internal("lock poisoned")
    }

    pub fn hub_offline() -> Self {
        Self::new(
            ErrorCode::HubOffline,
            "The hub PC can't be reached. This action needs a live connection to the hub.",
        )
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
            return AppError::not_found("The record");
        }
        let text = error.to_string();
        // Trigger errors start with a stable code (docs/DATABASE.md §4).
        if text.contains("DOCUMENT_FINALIZED") {
            return AppError::new(
                ErrorCode::DocumentFinalized,
                "This document is finalized and can't be changed.",
            );
        }
        if text.contains("APPEND_ONLY") || text.contains("INVALID_STATUS") {
            return AppError::conflict("That change isn't allowed for this record.");
        }
        if text.contains("INVALID_ALLOCATION") {
            return AppError::validation("The payment allocation is not valid for this document.");
        }
        if text.contains("UNIQUE constraint failed") {
            return AppError::conflict("A record with the same unique value already exists.")
                .with_details(
                    json!({ "constraint": text.trim_start_matches("UNIQUE constraint failed: ") }),
                );
        }
        if text.contains("FOREIGN KEY constraint failed") {
            return AppError::validation("A referenced record does not exist or is still in use.");
        }
        if text.contains("CHECK constraint failed") {
            return AppError::validation("The data failed a consistency check.");
        }
        if text.contains("database is locked") || text.contains("database is busy") {
            return AppError::conflict("The database is busy. Please try again.");
        }
        AppError::internal(text)
    }
}

impl From<r2d2::Error> for AppError {
    fn from(error: r2d2::Error) -> Self {
        AppError::internal(format!("connection pool: {error}"))
    }
}

impl From<crate::db::DbError> for AppError {
    fn from(error: crate::db::DbError) -> Self {
        match error {
            crate::db::DbError::Sqlite(e) => e.into(),
            other => AppError::internal(other),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::internal(format!("json: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_the_typescript_constants() {
        let ts = include_str!("../../packages/shared/src/constants.ts");
        let block = &ts[ts.find("export const ERROR_CODES").unwrap()
            ..ts.find("export type ErrorCode").unwrap()];
        let ts_codes: Vec<&str> = block.split('"').skip(1).step_by(2).collect();
        let rust_codes: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(rust_codes, ts_codes);
        for code in ErrorCode::ALL {
            assert_eq!(serde_json::to_value(code).unwrap(), code.as_str());
        }
    }

    #[test]
    fn trigger_errors_map_to_codes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE t (x INTEGER);
             CREATE TRIGGER lock BEFORE UPDATE ON t BEGIN SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: t'); END;
             INSERT INTO t VALUES (1);",
        )
        .unwrap();
        let err: AppError = conn.execute("UPDATE t SET x = 2", []).unwrap_err().into();
        assert_eq!(err.code, ErrorCode::DocumentFinalized);

        let internal = AppError::internal("boom");
        assert_eq!(internal.code, ErrorCode::Internal);
        assert!(
            !internal.message.contains("boom"),
            "causes are logged, not shown"
        );
    }
}
