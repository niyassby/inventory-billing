//! Typed, validated, versioned settings (Master Prompt §8). Each section is one row in
//! `settings`; missing rows and missing fields fall back to defaults, so adding a field never
//! needs a data migration. Changes are audited with before/after values.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::audit::AuditEntry;
use super::{write, Ctx};
use crate::domain::money::RoundingMode;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BusinessSettings {
    pub name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub gstin: Option<String>,
    /// Two-digit GST state code of the business (decides CGST+SGST vs IGST).
    pub state_code: Option<String>,
    pub currency: String,
    pub currency_minor_digits: u32,
    /// IANA timezone used for business dates ("today").
    pub timezone: String,
    /// Month the financial year starts (India: 4 = April).
    pub fy_start_month: u32,
    pub locale: String,
}

impl Default for BusinessSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            address: None,
            phone: None,
            email: None,
            gstin: None,
            state_code: None,
            currency: "INR".into(),
            currency_minor_digits: 2,
            timezone: "Asia/Kolkata".into(),
            fy_start_month: 4,
            locale: "en-IN".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxRegime {
    Gst,
    Vat,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    HalfUp,
    HalfEven,
}

impl From<Rounding> for RoundingMode {
    fn from(r: Rounding) -> Self {
        match r {
            Rounding::HalfUp => RoundingMode::HalfUp,
            Rounding::HalfEven => RoundingMode::HalfEven,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TaxSettings {
    pub regime: TaxRegime,
    /// Default for new sales documents: catalogue prices include tax (retail MRP style).
    pub prices_include_tax: bool,
    /// Round grand totals to this many minor units (100 = nearest rupee); 0 = no round-off.
    pub round_total_to: i64,
    pub rounding: Rounding,
}

impl Default for TaxSettings {
    fn default() -> Self {
        Self {
            regime: TaxRegime::Gst,
            prices_include_tax: true,
            round_total_to: 100,
            rounding: Rounding::HalfUp,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NegativeStockPolicy {
    /// Refuse a sale or issue that would take stock below zero (safest; the default).
    Block,
    /// Allow it but return a warning.
    Warn,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct InventorySettings {
    pub negative_stock_policy: NegativeStockPolicy,
    /// "weighted_average" (implemented) or "fifo" (reports only, Phase 6).
    pub valuation_method: String,
    pub low_stock_alerts: bool,
}

impl Default for InventorySettings {
    fn default() -> Self {
        Self {
            negative_stock_policy: NegativeStockPolicy::Block,
            valuation_method: "weighted_average".into(),
            low_stock_alerts: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SecuritySettings {
    pub session_hours: i64,
    pub idle_timeout_minutes: i64,
    pub max_failed_logins: i64,
    pub lockout_minutes: i64,
    pub min_password_length: usize,
    pub pin_login_enabled: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            session_hours: 12,
            idle_timeout_minutes: 30,
            max_failed_logins: 5,
            lockout_minutes: 15,
            min_password_length: 10,
            pin_login_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct InvoiceSettings {
    pub footer_text: Option<String>,
    pub terms: Option<String>,
    /// "a4", "thermal_80" or "thermal_58".
    pub paper: String,
    pub show_qr: bool,
}

impl Default for InvoiceSettings {
    fn default() -> Self {
        Self {
            footer_text: None,
            terms: None,
            paper: "thermal_80".into(),
            show_qr: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub business: BusinessSettings,
    pub tax: TaxSettings,
    pub inventory: InventorySettings,
    pub security: SecuritySettings,
    pub invoice: InvoiceSettings,
}

impl Settings {
    pub fn timezone(&self) -> chrono_tz::Tz {
        self.business
            .timezone
            .parse()
            .unwrap_or(chrono_tz::Asia::Kolkata)
    }

    /// The calendar date of `now` in the business timezone ("YYYY-MM-DD").
    pub fn business_date(&self, now: DateTime<Utc>) -> String {
        now.with_timezone(&self.timezone())
            .format("%Y-%m-%d")
            .to_string()
    }

    /// Financial-year label for a business date: "25-26" (or "2025" for a calendar-year FY).
    pub fn fy_label(&self, business_date: &str) -> String {
        crate::domain::numbering::financial_year(business_date, self.business.fy_start_month)
    }
}

const SECTIONS: [&str; 5] = ["business", "tax", "inventory", "security", "invoice"];

fn section<T: for<'de> Deserialize<'de> + Default>(conn: &Connection, key: &str) -> AppResult<T> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match raw {
        // A stored section that no longer parses (should never happen) falls back to defaults
        // rather than locking the business out; the problem is logged.
        Some(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
            tracing::warn!("settings section {key} is invalid ({e}); using defaults");
            T::default()
        }),
        None => T::default(),
    })
}

pub fn load(conn: &Connection) -> AppResult<Settings> {
    Ok(Settings {
        business: section(conn, "business")?,
        tax: section(conn, "tax")?,
        inventory: section(conn, "inventory")?,
        security: section(conn, "security")?,
        invoice: section(conn, "invoice")?,
    })
}

/// Validates a whole section value and returns it normalised.
fn validate(section: &str, value: &Value) -> AppResult<Value> {
    let invalid =
        |e: serde_json::Error| AppError::validation(format!("Invalid {section} settings: {e}"));
    let normalised = match section {
        "business" => {
            let mut s: BusinessSettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            s.name = super::common::required("name", &s.name, 120)?;
            s.gstin = super::common::optional_gstin("gstin", s.gstin.as_deref())?;
            s.email = super::common::optional_email("email", s.email.as_deref())?;
            if s.state_code.is_none() {
                s.state_code = s.gstin.as_ref().map(|g| g[..2].to_string());
            }
            if s.timezone.parse::<chrono_tz::Tz>().is_err() {
                return Err(AppError::field("timezone", "Unknown timezone."));
            }
            if !(1..=12).contains(&s.fy_start_month) {
                return Err(AppError::field(
                    "fyStartMonth",
                    "Choose a month between 1 and 12.",
                ));
            }
            if s.currency_minor_digits > 3 {
                return Err(AppError::field(
                    "currencyMinorDigits",
                    "Currencies use 0 to 3 decimals.",
                ));
            }
            serde_json::to_value(s)?
        }
        "tax" => {
            let s: TaxSettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            if !(0..=100_000).contains(&s.round_total_to) {
                return Err(AppError::field(
                    "roundTotalTo",
                    "Round-off must be between 0 and 100000 minor units.",
                ));
            }
            serde_json::to_value(s)?
        }
        "inventory" => {
            let s: InventorySettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            if !matches!(s.valuation_method.as_str(), "weighted_average" | "fifo") {
                return Err(AppError::field(
                    "valuationMethod",
                    "Use weighted_average or fifo.",
                ));
            }
            serde_json::to_value(s)?
        }
        "security" => {
            let s: SecuritySettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            if !(1..=72).contains(&s.session_hours)
                || !(1..=480).contains(&s.idle_timeout_minutes)
                || !(3..=20).contains(&s.max_failed_logins)
                || !(1..=1440).contains(&s.lockout_minutes)
                || !(8..=64).contains(&s.min_password_length)
            {
                return Err(AppError::validation(
                    "A security setting is outside its allowed range.",
                ));
            }
            serde_json::to_value(s)?
        }
        "invoice" => {
            let s: InvoiceSettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            if !matches!(s.paper.as_str(), "a4" | "thermal_80" | "thermal_58") {
                return Err(AppError::field(
                    "paper",
                    "Use a4, thermal_80 or thermal_58.",
                ));
            }
            serde_json::to_value(s)?
        }
        other => {
            return Err(AppError::validation(format!(
                "Unknown settings section '{other}'."
            )))
        }
    };
    Ok(normalised)
}

/// Writes a section without permission checks (setup and tests).
pub fn store(
    conn: &Connection,
    section: &str,
    value: &Value,
    user_id: Option<&str>,
    now: &str,
) -> AppResult<Value> {
    let value = validate(section, value)?;
    conn.execute(
        "INSERT INTO settings (key, value_json, version, updated_at, updated_by) VALUES (?1, ?2, 1, ?3, ?4)
         ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, version = version + 1,
           updated_at = excluded.updated_at, updated_by = excluded.updated_by",
        params![section, value.to_string(), now, user_id],
    )?;
    Ok(value)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsInput {
    /// "business", "tax", "inventory", "security" or "invoice".
    pub section: String,
    /// The whole section (fields left out take their defaults).
    pub value: Value,
}

pub fn update(ctx: &mut Ctx<'_>, input: &UpdateSettingsInput) -> AppResult<Settings> {
    ctx.require("settings.manage")?;
    if !SECTIONS.contains(&input.section.as_str()) {
        return Err(AppError::field("section", "Unknown settings section."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let before = serde_json::to_value(load(tx)?)?[&input.section].clone();
        let after = store(tx, &input.section, &input.value, Some(&actor.user_id), &now)?;
        AuditEntry::new("SETTINGS_UPDATE", "settings", Some(&input.section))
            .by(actor)
            .before(before)
            .after(after)
            .write(tx, &now)?;
        load(tx)
    })
}

pub fn get(ctx: &Ctx<'_>) -> AppResult<Settings> {
    ctx.require("settings.view")?;
    load(ctx.conn)
}

/// Settings every signed-in user may read (currency, tax defaults, business name for receipts).
pub fn public(conn: &Connection) -> AppResult<Value> {
    let s = load(conn)?;
    Ok(
        json!({ "business": s.business, "tax": s.tax, "invoice": s.invoice,
               "inventory": { "negativeStockPolicy": s.inventory.negative_stock_policy } }),
    )
}
