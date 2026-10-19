//! Client mode: this PC is a counter working through a hub (docs/ARCHITECTURE.md §6).
//!
//! Every v2 command from the window is forwarded to the hub (`call`). When the hub can't be
//! reached, the few commands that make sense offline are answered from the local store
//! (`offline`), and sales go to the outbox. A background loop keeps the connection state:
//!
//! ```text
//! Connected ──1 failed heartbeat──▶ Degraded ──3 failures (≈15 s)──▶ HubOffline
//!     ▲                                │                                  │
//!     └────────── outbox drained ◀── Syncing ◀──────── hub reachable ─────┘
//! ```
//!
//! and, while connected, drains the outbox in order, refreshes the catalogue and tops up the
//! leased invoice numbers.

pub mod hub_api;
pub mod offline;
pub mod store;

use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::hub_api::{HubApi, HubError};
use crate::app::Clock;
use crate::error::{AppError, AppResult};
use crate::node::HubLink;
use crate::services::auth::{ApprovalInput, AuthSession, SessionUser};
use crate::services::sync::{ItemStatus, OutboxItem, PushRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    Connected,
    /// A heartbeat or call just failed; still trying the hub first.
    Degraded,
    HubOffline,
    /// Back online and sending queued sales.
    Syncing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub link: Link,
    pub hub_url: String,
    pub business_name: String,
    /// Offline sales waiting to be sent.
    pub pending: i64,
    /// Offline sales the hub refused (a manager must retry or discard them).
    pub rejected: i64,
    pub offline_numbers_left: i64,
    pub catalogue_at: Option<String>,
    pub last_contact_at: Option<String>,
    pub clock_skew_seconds: Option<i64>,
    pub last_error: Option<String>,
}

pub type StatusListener = Box<dyn Fn(&ClientStatus) + Send + Sync>;

pub struct ClientRuntime {
    link: HubLink,
    api: HubApi,
    store: Mutex<Connection>,
    clock: Arc<dyn Clock>,
    status: RwLock<ClientStatus>,
    failures: Mutex<u32>,
    listener: RwLock<Option<StatusListener>>,
    /// Only one sync pass at a time.
    syncing: tokio::sync::Mutex<()>,
}

const HEARTBEAT: Duration = Duration::from_secs(5);
const OFFLINE_AFTER: u32 = 3;

impl ClientRuntime {
    pub fn new(
        data_dir: &Path,
        link: HubLink,
        device_token: String,
        clock: Arc<dyn Clock>,
    ) -> AppResult<Arc<Self>> {
        let api = HubApi::new(&link.url, &link.fingerprint, Some(device_token))?;
        let store = store::open(&data_dir.join("client.db"))?;
        let status = ClientStatus {
            link: Link::Degraded,
            hub_url: link.url.clone(),
            business_name: link.business_name.clone(),
            pending: 0,
            rejected: 0,
            offline_numbers_left: 0,
            catalogue_at: None,
            last_contact_at: None,
            clock_skew_seconds: None,
            last_error: None,
        };
        let runtime = Arc::new(Self {
            link,
            api,
            store: Mutex::new(store),
            clock,
            status: RwLock::new(status),
            failures: Mutex::new(0),
            listener: RwLock::new(None),
            syncing: tokio::sync::Mutex::new(()),
        });
        runtime.refresh_counts()?;
        Ok(runtime)
    }

    pub fn link(&self) -> &HubLink {
        &self.link
    }

    pub fn on_status(&self, listener: StatusListener) {
        if let Ok(mut l) = self.listener.write() {
            *l = Some(listener);
        }
    }

    pub fn status(&self) -> ClientStatus {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }

    fn update(&self, f: impl FnOnce(&mut ClientStatus)) {
        let changed = {
            let Ok(mut status) = self.status.write() else {
                return;
            };
            let before = status.clone();
            f(&mut status);
            (*status != before).then(|| status.clone())
        };
        if let Some(status) = changed {
            if let Ok(listener) = self.listener.read() {
                if let Some(listener) = listener.as_ref() {
                    listener(&status);
                }
            }
        }
    }

    fn with_store<T>(&self, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self.store.lock().map_err(AppError::internal_lock)?;
        f(&mut conn)
    }

    fn refresh_counts(&self) -> AppResult<()> {
        let now = self.clock.now();
        let (pending, rejected, left, catalogue_at) = self.with_store(|c| {
            let ctx = store::context(c)?;
            let left = match &ctx {
                Some(ctx) => store::numbers_left(
                    c,
                    &ctx.settings.fy_label(&ctx.settings.business_date(now)),
                )?,
                None => 0,
            };
            Ok((
                store::count(c, "pending")?,
                store::count(c, "rejected")?,
                left,
                ctx.map(|c| c.generated_at),
            ))
        })?;
        self.update(|s| {
            s.pending = pending;
            s.rejected = rejected;
            s.offline_numbers_left = left;
            s.catalogue_at = catalogue_at;
        });
        Ok(())
    }

    fn contact_ok(&self) {
        if let Ok(mut f) = self.failures.lock() {
            *f = 0;
        }
        let now = self.clock.now().to_rfc3339();
        self.update(|s| {
            s.last_contact_at = Some(now);
            s.last_error = None;
            if matches!(s.link, Link::Degraded | Link::HubOffline) {
                s.link = if s.pending > 0 {
                    Link::Syncing
                } else {
                    Link::Connected
                };
            }
        });
    }

    fn contact_failed(&self, why: &str) {
        let failures = self.failures.lock().map_or(OFFLINE_AFTER, |mut f| {
            *f += 1;
            *f
        });
        let why = why.to_string();
        self.update(|s| {
            s.last_error = Some(why);
            s.link = if failures >= OFFLINE_AFTER {
                Link::HubOffline
            } else {
                Link::Degraded
            };
        });
    }

    pub fn is_offline(&self) -> bool {
        self.status().link == Link::HubOffline
    }

    /// A v2 command from the window: `{ token, input, approval }` as Tauri received it.
    pub async fn call(&self, name: &str, payload: Value) -> AppResult<Value> {
        let token = payload
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let input = payload.get("input").cloned().unwrap_or(Value::Null);
        let approval: Option<ApprovalInput> = payload
            .get("approval")
            .filter(|a| !a.is_null())
            .map(|a| serde_json::from_value(a.clone()))
            .transpose()
            .map_err(|e| AppError::validation(format!("Invalid approval: {e}")))?;
        if !self.is_offline() {
            match self.api.call(name, &token, &input, approval.as_ref()).await {
                Ok(value) => {
                    self.contact_ok();
                    self.after_online_call(name, &token, &value);
                    return Ok(value);
                }
                Err(HubError::App(e)) => {
                    self.contact_ok();
                    return Err(e);
                }
                Err(HubError::Unreachable(why)) => self.contact_failed(&why),
            }
        }
        let now = self.clock.now();
        let result = self.with_store(|c| Ok(offline::handle(c, name, &token, &input, now)));
        match result? {
            Some(outcome) => {
                let value = outcome?;
                if name == "sales_create_invoice" {
                    self.refresh_counts()?;
                }
                Ok(value)
            }
            None => Err(AppError::hub_offline()),
        }
    }

    /// Keeps what offline mode needs: who is signed in (for permission checks).
    fn after_online_call(&self, name: &str, token: &str, value: &Value) {
        let result = self.with_store(|c| {
            match name {
                "auth_login" | "auth_pin_login" | "setup_create_business" => {
                    if let Ok(session) = serde_json::from_value::<AuthSession>(value.clone()) {
                        store::remember_session(
                            c,
                            &session.token,
                            &session.user,
                            &session.expires_at,
                        )?;
                    }
                }
                "auth_me" => {
                    if let Ok(user) = serde_json::from_value::<SessionUser>(value.clone()) {
                        store::remember_session(c, token, &user, "")?;
                    }
                }
                "auth_logout" => store::forget_session(c, token)?,
                _ => {}
            }
            Ok(())
        });
        if let Err(e) = result {
            tracing::warn!("could not cache {name} for offline use: {}", e.message);
        }
    }

    // ------------------------------------------------------------------ background work

    /// One pass: heartbeat, then (when reachable) outbox, catalogue and leases.
    pub async fn sync_once(&self) -> AppResult<ClientStatus> {
        let _guard = self.syncing.lock().await;
        match self.api.health().await {
            Ok(health) => {
                self.contact_ok();
                if let Ok(hub_time) = DateTime::parse_from_rfc3339(&health.hub_time) {
                    let skew = (hub_time.with_timezone(&Utc) - self.clock.now()).num_seconds();
                    self.update(|s| s.clock_skew_seconds = Some(skew));
                }
            }
            Err(HubError::Unreachable(why)) => {
                self.contact_failed(&why);
                return Ok(self.status());
            }
            Err(HubError::App(e)) => {
                self.update(|s| s.last_error = Some(e.message.clone()));
                return Ok(self.status());
            }
        }
        if let Err(e) = self.drain().await {
            tracing::warn!("outbox: {}", e.message);
        }
        if let Err(e) = self.refresh_catalogue(false).await {
            tracing::warn!("catalogue refresh: {}", e.message);
        }
        if let Err(e) = self.top_up_leases().await {
            tracing::warn!("number lease: {}", e.message);
        }
        self.refresh_counts()?;
        self.update(|s| {
            if s.link == Link::Syncing && s.pending == 0 {
                s.link = Link::Connected;
            }
        });
        Ok(self.status())
    }

    /// Sends queued sales in order until the outbox is empty or the hub refuses for a reason
    /// that may pass (then they wait for the next pass).
    pub async fn drain(&self) -> AppResult<usize> {
        let mut sent = 0;
        loop {
            let items = self.with_store(|c| store::pending(c, 50))?;
            if items.is_empty() {
                return Ok(sent);
            }
            self.update(|s| s.link = Link::Syncing);
            let request = PushRequest {
                client_now: self.clock.now().to_rfc3339(),
                items: items
                    .iter()
                    .map(|i| OutboxItem {
                        idempotency_key: i.idempotency_key.clone(),
                        device_seq: i.device_seq,
                        command: i.command.clone(),
                        payload: i.payload.clone(),
                        session_token: i.session_token.clone(),
                        client_created_at: i.client_created_at.clone(),
                        approval: i.approval.clone(),
                    })
                    .collect(),
            };
            let response = match self.api.push(&request).await {
                Ok(r) => r,
                Err(HubError::Unreachable(why)) => {
                    self.contact_failed(&why);
                    return Ok(sent);
                }
                Err(HubError::App(e)) => return Err(e),
            };
            let now = self.clock.now().to_rfc3339();
            let mut stalled = false;
            self.with_store(|c| {
                for result in &response.results {
                    match (&result.status, &result.error) {
                        (ItemStatus::Applied | ItemStatus::Duplicate, _) => {
                            store::mark_done(
                                c,
                                &result.idempotency_key,
                                result.invoice.as_ref().map(|i| i.doc_no.as_str()),
                                &now,
                            )?;
                            sent += 1;
                        }
                        (ItemStatus::Rejected, Some(error)) => {
                            let temporary = matches!(
                                error.code,
                                crate::error::ErrorCode::Internal
                                    | crate::error::ErrorCode::RateLimited
                            );
                            store::mark_failed(
                                c,
                                &result.idempotency_key,
                                error,
                                !temporary,
                                &now,
                            )?;
                            stalled |= temporary;
                        }
                        (ItemStatus::Rejected, None) => stalled = true,
                    }
                }
                Ok(())
            })?;
            self.refresh_counts()?;
            if stalled || response.results.len() < items.len() {
                return Ok(sent);
            }
        }
    }

    /// Fetches the catalogue when it is older than `network.catalogRefreshMinutes` (or `force`).
    pub async fn refresh_catalogue(&self, force: bool) -> AppResult<bool> {
        let now = self.clock.now();
        let due = force
            || self.with_store(|c| {
                Ok(match store::context(c)? {
                    None => true,
                    Some(ctx) => {
                        DateTime::parse_from_rfc3339(&ctx.generated_at).map_or(true, |t| {
                            now - t.with_timezone(&Utc)
                                >= chrono::Duration::minutes(
                                    ctx.settings.network.catalog_refresh_minutes,
                                )
                        })
                    }
                })
            })?;
        if !due {
            return Ok(false);
        }
        let snapshot = self.api.catalogue().await.map_err(AppError::from)?;
        self.with_store(|c| store::save_catalogue(c, &snapshot))?;
        Ok(true)
    }

    /// Keeps at least half a lease of offline numbers for the current financial year.
    pub async fn top_up_leases(&self) -> AppResult<()> {
        let now = self.clock.now();
        let Some((period, size, policy)) = self.with_store(|c| {
            Ok(store::context(c)?.map(|ctx| {
                (
                    ctx.settings.fy_label(&ctx.settings.business_date(now)),
                    ctx.settings.network.lease_size,
                    ctx.settings.network.offline_sales_policy,
                )
            }))
        })?
        else {
            return Ok(());
        };
        if policy == crate::services::settings::OfflineSalesPolicy::Block {
            return Ok(());
        }
        let left = self.with_store(|c| store::numbers_left(c, &period))?;
        if left * 2 >= size {
            return Ok(());
        }
        let lease = self.api.lease().await.map_err(AppError::from)?;
        self.with_store(|c| store::add_lease(c, &lease))?;
        Ok(())
    }

    pub fn outbox(&self, include_done: bool) -> AppResult<Vec<store::OutboxEntry>> {
        self.with_store(|c| store::list(c, include_done))
    }

    /// The signed-in user as this counter knows them (online or from the cache).
    pub fn cached_user(&self, token: &str) -> AppResult<Option<SessionUser>> {
        let now = self.clock.now().to_rfc3339();
        self.with_store(|c| store::session(c, token, &now))
    }

    pub fn cached_customers(
        &self,
        term: &str,
    ) -> AppResult<Vec<crate::services::sync::CachedCustomer>> {
        self.with_store(|c| store::customers(c, term))
    }

    pub fn retry(&self, id: &str, approval: Option<&ApprovalInput>) -> AppResult<()> {
        let now = self.clock.now().to_rfc3339();
        self.with_store(|c| store::retry(c, id, approval, &now))?;
        self.refresh_counts()
    }

    pub fn discard(&self, id: &str) -> AppResult<()> {
        let now = self.clock.now().to_rfc3339();
        self.with_store(|c| store::discard(c, id, &now))?;
        self.refresh_counts()
    }

    /// The queued items exactly as `drain` would send them (tests replay them by hand).
    #[cfg(test)]
    pub fn pending_items(&self) -> AppResult<Vec<OutboxItem>> {
        Ok(self
            .with_store(|c| store::pending(c, 1000))?
            .into_iter()
            .map(|i| OutboxItem {
                idempotency_key: i.idempotency_key,
                device_seq: i.device_seq,
                command: i.command,
                payload: i.payload,
                session_token: i.session_token,
                client_created_at: i.client_created_at,
                approval: i.approval,
            })
            .collect())
    }

    /// Runs the background loop until the process ends.
    pub fn spawn(self: &Arc<Self>) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if let Err(e) = runtime.sync_once().await {
                    tracing::warn!("client sync: {}", e.message);
                }
                tokio::time::sleep(HEARTBEAT).await;
            }
        });
    }
}
