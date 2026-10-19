//! mDNS: the hub announces itself as `_invhub._tcp.local.` so a new counter can list hubs on
//! the network instead of typing an address. The answer only helps to find the hub; the pinned
//! certificate fingerprint is what proves it is the right one.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const SERVICE: &str = "_invhub._tcp.local.";

/// Keeps the announcement alive while it is held.
pub struct Advertisement {
    daemon: ServiceDaemon,
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

pub fn advertise(
    business_name: &str,
    port: u16,
    fingerprint_short: &str,
) -> AppResult<Advertisement> {
    let daemon = ServiceDaemon::new().map_err(AppError::internal)?;
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "inventory-hub".into())
        .to_lowercase();
    let instance = if business_name.trim().is_empty() {
        host.clone()
    } else {
        business_name.trim().to_string()
    };
    let properties = HashMap::from([
        ("fp".to_string(), fingerprint_short.to_string()),
        ("v".to_string(), env!("CARGO_PKG_VERSION").to_string()),
    ]);
    let info = ServiceInfo::new(
        SERVICE,
        &instance,
        &format!("{host}.local."),
        "",
        port,
        properties,
    )
    .map_err(AppError::internal)?
    .enable_addr_auto();
    daemon.register(info).map_err(AppError::internal)?;
    Ok(Advertisement { daemon })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FoundHub {
    pub name: String,
    pub urls: Vec<String>,
    pub fingerprint_short: Option<String>,
}

/// Listens for hubs for `wait` and returns what answered.
pub fn browse(wait: Duration) -> AppResult<Vec<FoundHub>> {
    let daemon = ServiceDaemon::new().map_err(AppError::internal)?;
    let receiver = daemon.browse(SERVICE).map_err(AppError::internal)?;
    let deadline = Instant::now() + wait;
    let mut found: Vec<FoundHub> = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(left) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let name = info
                    .get_fullname()
                    .trim_end_matches(SERVICE)
                    .trim_end_matches('.')
                    .to_string();
                let urls = info
                    .get_addresses()
                    .iter()
                    .filter(|ip| ip.is_ipv4())
                    .map(|ip| format!("https://{ip}:{}", info.get_port()))
                    .collect();
                let hub = FoundHub {
                    name,
                    urls,
                    fingerprint_short: info.get_property_val_str("fp").map(String::from),
                };
                if !found.contains(&hub) {
                    found.push(hub);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    Ok(found)
}
