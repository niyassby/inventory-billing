//! Typed, validated, versioned settings (Master Prompt §8). Each section is one row in
//! `settings`; missing rows and missing fields fall back to defaults, so adding a field never
//! needs a data migration. Changes are audited with before/after values.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Selling or issuing a batch past its expiry date.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExpiredStockPolicy {
    /// Expired batches are never picked and can't be chosen (the default).
    Block,
    /// Unexpired batches first; expired ones only when nothing else is left, with a warning.
    Warn,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct InventorySettings {
    pub negative_stock_policy: NegativeStockPolicy,
    /// "weighted_average" or "fifo". Changed only through the audited switch
    /// (`inventory_valuation_switch`), never by saving settings.
    pub valuation_method: String,
    pub low_stock_alerts: bool,
    pub expired_stock_policy: ExpiredStockPolicy,
    /// Batches expiring within this many days are "near expiry" (report, dashboard, notices).
    pub expiry_alert_days: i64,
    pub expiry_alerts: bool,
}

impl Default for InventorySettings {
    fn default() -> Self {
        Self {
            negative_stock_policy: NegativeStockPolicy::Block,
            valuation_method: "weighted_average".into(),
            low_stock_alerts: true,
            expired_stock_policy: ExpiredStockPolicy::Block,
            expiry_alert_days: 30,
            expiry_alerts: true,
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

/// What a client counter may do while the hub can't be reached (docs/ARCHITECTURE.md §6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OfflineSalesPolicy {
    /// No sales while the hub is offline.
    Block,
    /// Sell with invoice numbers leased in advance; oversells are recorded, not refused.
    AllowReservedNumbers,
    /// As above, but not items with little cached stock or batch tracking.
    AllowRestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkSettings {
    pub offline_sales_policy: OfflineSalesPolicy,
    /// Invoice numbers leased to a counter at a time.
    pub lease_size: i64,
    /// Under `allow_restricted`: items with cached stock at or below this (thousandths) can't be
    /// sold offline.
    pub restricted_min_stock: i64,
    /// An offline invoice whose counter clock differs from the hub's by more than this is held
    /// for review instead of being posted.
    pub max_clock_skew_hours: i64,
    /// TCP port of the hub's HTTPS API.
    pub port: u16,
    /// Extra subnets allowed to reach the hub (CIDR). Private ranges are always allowed.
    pub allowed_subnets: Vec<String>,
    /// How often counters refresh their offline catalogue.
    pub catalog_refresh_minutes: i64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            offline_sales_policy: OfflineSalesPolicy::AllowReservedNumbers,
            lease_size: 50,
            restricted_min_stock: 5_000,
            max_clock_skew_hours: 24,
            port: 47900,
            allowed_subnets: Vec::new(),
            catalog_refresh_minutes: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BackupSettings {
    pub enabled: bool,
    /// Daily backup time, business timezone ("HH:MM").
    pub daily_at: String,
    /// Extra backups every N hours while the app runs (0 = daily only).
    pub interval_hours: i64,
    pub keep_daily: i64,
    pub keep_weekly: i64,
    pub keep_monthly: i64,
    /// Backup folder; default: "backups" next to the database.
    pub directory: Option<String>,
    /// Optional second copy (USB drive, NAS share).
    pub mirror_directory: Option<String>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            daily_at: "21:00".into(),
            interval_hours: 4,
            keep_daily: 7,
            keep_weekly: 4,
            keep_monthly: 12,
            directory: None,
            mirror_directory: None,
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
    pub network: NetworkSettings,
    pub backup: BackupSettings,
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

const SECTIONS: [&str; 7] = [
    "business",
    "tax",
    "inventory",
    "security",
    "invoice",
    "network",
    "backup",
];

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
        network: section(conn, "network")?,
        backup: section(conn, "backup")?,
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
            if !(1..=365).contains(&s.expiry_alert_days) {
                return Err(AppError::field(
                    "expiryAlertDays",
                    "Warn 1 to 365 days before expiry.",
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
        "network" => {
            let s: NetworkSettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            if !(10..=1000).contains(&s.lease_size)
                || !(0..=1_000_000_000).contains(&s.restricted_min_stock)
                || !(1..=168).contains(&s.max_clock_skew_hours)
                || s.port < 1024
                || !(1..=1440).contains(&s.catalog_refresh_minutes)
            {
                return Err(AppError::validation(
                    "A network setting is outside its allowed range.",
                ));
            }
            for subnet in &s.allowed_subnets {
                if subnet.parse::<ipnet::IpNet>().is_err() {
                    return Err(AppError::field(
                        "allowedSubnets",
                        format!("'{subnet}' is not a subnet like 192.168.1.0/24."),
                    ));
                }
            }
            serde_json::to_value(s)?
        }
        "backup" => {
            let s: BackupSettings = serde_json::from_value(value.clone()).map_err(invalid)?;
            if chrono::NaiveTime::parse_from_str(&s.daily_at, "%H:%M").is_err() {
                return Err(AppError::field("dailyAt", "Use a time like 21:00."));
            }
            if !(0..=24).contains(&s.interval_hours)
                || !(1..=90).contains(&s.keep_daily)
                || !(0..=52).contains(&s.keep_weekly)
                || !(0..=120).contains(&s.keep_monthly)
            {
                return Err(AppError::validation(
                    "A backup setting is outside its allowed range.",
                ));
            }
            for (field, dir) in [
                ("directory", &s.directory),
                ("mirrorDirectory", &s.mirror_directory),
            ] {
                if let Some(d) = dir {
                    if !std::path::Path::new(d).is_absolute() {
                        return Err(AppError::field(field, "Use a full folder path."));
                    }
                }
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
    /// "business", "tax", "inventory", "security", "invoice", "network" or "backup".
    pub section: String,
    /// The whole section (fields left out take their defaults).
    pub value: Value,
}

pub fn update(ctx: &mut Ctx<'_>, input: &UpdateSettingsInput) -> AppResult<Settings> {
    ctx.require("settings.manage")?;
    if input.section == "backup" {
        ctx.require("backup.manage")?;
    }
    if !SECTIONS.contains(&input.section.as_str()) {
        return Err(AppError::field("section", "Unknown settings section."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let before = serde_json::to_value(load(tx)?)?[&input.section].clone();
        // The valuation method needs its switch routine (layers are built or dropped there).
        if input.section == "inventory"
            && input
                .value
                .get("valuationMethod")
                .is_some_and(|m| *m != before["valuationMethod"])
        {
            return Err(AppError::field(
                "valuationMethod",
                "Change the valuation method with Inventory → Valuation method (it checks and converts the stock).",
            ));
        }
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PublicInventory {
    pub negative_stock_policy: NegativeStockPolicy,
    #[serde(default = "default_expired_policy")]
    pub expired_stock_policy: ExpiredStockPolicy,
    #[serde(default)]
    pub valuation_method: String,
}

fn default_expired_policy() -> ExpiredStockPolicy {
    ExpiredStockPolicy::Block
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub business: BusinessSettings,
    pub tax: TaxSettings,
    pub invoice: InvoiceSettings,
    pub inventory: PublicInventory,
    pub offline_sales_policy: OfflineSalesPolicy,
}

impl From<&Settings> for PublicSettings {
    fn from(s: &Settings) -> Self {
        Self {
            business: s.business.clone(),
            tax: s.tax.clone(),
            invoice: s.invoice.clone(),
            inventory: PublicInventory {
                negative_stock_policy: s.inventory.negative_stock_policy,
                expired_stock_policy: s.inventory.expired_stock_policy,
                valuation_method: s.inventory.valuation_method.clone(),
            },
            offline_sales_policy: s.network.offline_sales_policy,
        }
    }
}

/// Settings every signed-in user may read (currency, tax defaults, business name for receipts).
pub fn public(conn: &Connection) -> AppResult<PublicSettings> {
    Ok(PublicSettings::from(&load(conn)?))
}
