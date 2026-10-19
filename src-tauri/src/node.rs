//! This PC's role in the shop (docs/ARCHITECTURE.md §3, D-2): standalone (default), hub (also
//! serves other devices over the LAN) or client (a counter that works through a hub). Kept in
//! `device.json` next to the database, never in the shared database. The device credential of a
//! client lives in the OS keychain, not in this file.

use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceMode {
    #[default]
    Standalone,
    Hub,
    Client,
}

/// The hub a client counter is paired with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HubLink {
    /// For example "https://192.168.1.20:47900".
    pub url: String,
    /// SHA-256 of the hub's certificate, pinned at pairing.
    pub fingerprint: String,
    pub device_id: String,
    pub short_code: String,
    pub business_name: String,
    pub paired_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfig {
    pub mode: DeviceMode,
    pub device_name: String,
    pub hub: Option<HubLink>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            mode: DeviceMode::Standalone,
            device_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "This PC".into()),
            hub: None,
        }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join("device.json")
}

impl DeviceConfig {
    pub fn load(dir: &Path) -> Self {
        match fs::read_to_string(path(dir)) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("device.json is invalid ({e}); starting standalone");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Written to a temporary file first, so a crash never leaves half a config.
    pub fn save(&self, dir: &Path) -> AppResult<()> {
        fs::create_dir_all(dir).map_err(AppError::internal)?;
        let tmp = dir.join("device.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?).map_err(AppError::internal)?;
        fs::rename(&tmp, path(dir)).map_err(AppError::internal)
    }
}

/// What the hub tells its own window about the LAN server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HubInfo {
    pub running: bool,
    pub port: u16,
    /// Full certificate fingerprint; counters show the short form when pairing.
    pub fingerprint: String,
    pub fingerprint_short: String,
    /// Addresses counters can use ("https://192.168.1.20:47900").
    pub urls: Vec<String>,
    pub error: Option<String>,
}

/// "AB12-CD34-EF56-7890": the first 8 bytes of a fingerprint, for people to compare by eye.
pub fn short_fingerprint(fingerprint: &str) -> String {
    let hex: String = fingerprint
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(16)
        .collect();
    hex.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_uppercase())
        .collect::<Vec<_>>()
        .join("-")
}
