//! First-run onboarding. Replaces v1's seeded `admin@example.com / password123` (audit S-1): a
//! fresh database has no users at all until the owner creates the business and their account.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audit::AuditEntry;
use super::auth::validate_password;
use super::common::{optional, optional_email, optional_gstin, optional_phone, required};
use super::numbering::{ensure_default_series, DOC_TYPES};
use super::settings::{self, BusinessSettings, TaxRegime, TaxSettings};
use super::{accounts, write};
use crate::auth as crypto;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

pub const OWNER_ROLE: &str = "01920000-0000-7000-8000-000000000001";

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub needs_setup: bool,
    pub schema_version: i64,
    /// A v1 database exists that can be migrated instead of starting empty.
    pub legacy_database_found: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerInput {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupInput {
    pub business_name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub gstin: Option<String>,
    pub state_code: Option<String>,
    pub currency: Option<String>,
    pub currency_minor_digits: Option<u32>,
    pub timezone: Option<String>,
    pub fy_start_month: Option<u32>,
    pub tax_regime: Option<TaxRegime>,
    pub prices_include_tax: Option<bool>,
    pub owner: OwnerInput,
}

pub fn needs_setup(conn: &Connection) -> AppResult<bool> {
    let users: i64 = conn.query_row(
        "SELECT count(*) FROM users WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(users == 0)
}

pub fn status(conn: &Connection, legacy_database_found: bool) -> AppResult<SetupStatus> {
    Ok(SetupStatus {
        needs_setup: needs_setup(conn)?,
        schema_version: crate::db::migrate::current_version(conn)?,
        legacy_database_found,
    })
}

/// Creates the business, its first branch, store, register, numbering and the owner account.
/// Only allowed while there are no users.
pub fn create_business(
    conn: &mut Connection,
    now: DateTime<Utc>,
    input: &SetupInput,
) -> AppResult<String> {
    if !needs_setup(conn)? {
        return Err(AppError::conflict("This business is already set up."));
    }
    let defaults = BusinessSettings::default();
    let gstin = optional_gstin("gstin", input.gstin.as_deref())?;
    let business = BusinessSettings {
        name: required("businessName", &input.business_name, 120)?,
        address: optional("address", input.address.as_deref(), 500)?,
        phone: optional_phone("phone", input.phone.as_deref())?,
        email: optional_email("email", input.email.as_deref())?,
        state_code: optional("stateCode", input.state_code.as_deref(), 2)?
            .or_else(|| gstin.as_ref().map(|g| g[..2].to_string())),
        gstin,
        currency: input.currency.clone().unwrap_or(defaults.currency),
        currency_minor_digits: input
            .currency_minor_digits
            .unwrap_or(defaults.currency_minor_digits),
        timezone: input.timezone.clone().unwrap_or(defaults.timezone),
        fy_start_month: input.fy_start_month.unwrap_or(defaults.fy_start_month),
        locale: defaults.locale,
    };
    let tax = TaxSettings {
        regime: input.tax_regime.unwrap_or(TaxRegime::Gst),
        prices_include_tax: input.prices_include_tax.unwrap_or(true),
        ..TaxSettings::default()
    };
    let owner_name = required("owner.name", &input.owner.name, 120)?;
    let owner_email = optional_email("owner.email", Some(&input.owner.email))?
        .ok_or_else(|| AppError::field("owner.email", "This field is required."))?;
    validate_password(
        &input.owner.password,
        &owner_email,
        &settings::SecuritySettings::default(),
    )?;
    let password_hash = crypto::hash_password(&input.owner.password).map_err(AppError::internal)?;

    let now = now.to_rfc3339();
    let owner_id = new_id();
    write(conn, |tx| {
        settings::store(
            tx,
            "business",
            &serde_json::to_value(&business)?,
            None,
            &now,
        )?;
        settings::store(tx, "tax", &serde_json::to_value(&tax)?, None, &now)?;

        tx.execute(
            "INSERT INTO users (id, name, email, password_hash, role_id, password_changed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6)",
            params![owner_id, owner_name, owner_email, password_hash, OWNER_ROLE, now],
        )?;
        let branch_id = new_id();
        tx.execute(
            "INSERT INTO branches (id, code, name, address, phone, email, gstin, state_code, is_default, created_at, updated_at, created_by, updated_by)
             VALUES (?1, 'MAIN', ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8, ?9, ?9)",
            params![branch_id, business.name, business.address, business.phone, business.email, business.gstin, business.state_code, now, owner_id],
        )?;
        tx.execute(
            "INSERT INTO warehouses (id, branch_id, code, name, is_default, created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, 'MAIN', 'Main store', 1, ?3, ?3, ?4, ?4)",
            params![new_id(), branch_id, now, owner_id],
        )?;
        tx.execute(
            "INSERT INTO cash_registers (id, branch_id, name, cash_account_id, created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, 'Counter 1', ?3, ?4, ?4, ?5, ?5)",
            params![new_id(), branch_id, accounts::CASH, now, owner_id],
        )?;
        for doc_type in DOC_TYPES {
            ensure_default_series(tx, &branch_id, doc_type, &now)?;
        }
        let mut entry = AuditEntry::new("SETUP", "business", Some(&branch_id))
            .summary(format!("Business '{}' set up", business.name))
            .after(json!({ "business": business, "owner": owner_email }));
        entry.user_id = Some(&owner_id);
        entry.write(tx, &now)
    })?;
    Ok(owner_email)
}

/// The default branch and its default warehouse (every v2 database has them after setup or
/// migration).
pub fn default_location(conn: &Connection) -> AppResult<(String, String)> {
    conn.query_row(
        "SELECT b.id, w.id FROM branches b JOIN warehouses w ON w.branch_id = b.id AND w.is_default = 1 AND w.deleted_at IS NULL
         WHERE b.is_default = 1 AND b.deleted_at IS NULL",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|_| AppError::new(crate::error::ErrorCode::SetupRequired, "The business has no default branch yet."))
}
