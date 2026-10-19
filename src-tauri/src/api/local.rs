//! Commands about this PC itself (its mode, pairing, the counter's outbox). They always run
//! here, even in client mode, and are never sent to the hub. Same `{ token, input }` envelope as
//! the v2 commands; most work without a session because a counter must be paired before anyone
//! can sign in on it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app::AppState;
use crate::client::hub_api::{normalise_url, HubApi};
use crate::client::store::OutboxEntry;
use crate::client::{ClientRuntime, ClientStatus};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::hub::discovery::{Advertisement, FoundHub};
use crate::hub::HubServer;
use crate::node::{short_fingerprint, DeviceConfig, DeviceMode, HubInfo, HubLink};
use crate::secrets::DEVICE_TOKEN;
use crate::services::auth::{self, ApprovalInput};
use crate::services::common::NoInput;
use crate::services::devices::PairInput;
use crate::services::sync::CachedCustomer;
use crate::services::users::IdInput;

/// This PC's role and, by mode, its client runtime or hub server.
pub struct Node {
    pub data_dir: PathBuf,
    pub config: RwLock<DeviceConfig>,
    /// The mode the app started in (a change takes effect after a restart).
    pub running_mode: DeviceMode,
    pub client: Option<Arc<ClientRuntime>>,
    pub hub_server: Mutex<Option<HubServer>>,
    pub advertisement: Mutex<Option<Advertisement>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub mode: DeviceMode,
    pub running_mode: DeviceMode,
    pub device_name: String,
    pub hub_link: Option<HubLink>,
    /// Client mode: the connection to the hub.
    pub client: Option<ClientStatus>,
    /// Hub mode: the LAN server.
    pub hub: Option<HubInfo>,
    /// The saved mode differs from the running one: restart the app.
    pub restart_required: bool,
}

fn status(node: &Node, state: &AppState) -> AppResult<DeviceStatus> {
    let config = node.config.read().map_err(AppError::internal_lock)?.clone();
    Ok(DeviceStatus {
        mode: config.mode,
        running_mode: node.running_mode,
        device_name: config.device_name,
        hub_link: config.hub,
        client: node.client.as_ref().map(|c| c.status()),
        hub: (node.running_mode == DeviceMode::Hub)
            .then(|| state.hub.read().map(|h| h.clone()).unwrap_or_default()),
        restart_required: config.mode != node.running_mode,
    })
}

/// Changing what this PC is needs the owner when it already holds a business.
fn require_owner_if_set_up(state: &AppState, token: Option<&str>) -> AppResult<()> {
    let conn = state.conn()?;
    let users: i64 = conn.query_row(
        "SELECT count(*) FROM users WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    if users == 0 {
        return Ok(());
    }
    let session = auth::authenticate(&conn, token.unwrap_or_default(), state.now())?;
    session.actor.require("settings.manage")
}

fn client(node: &Node) -> AppResult<&Arc<ClientRuntime>> {
    node.client
        .as_ref()
        .ok_or_else(|| AppError::new(ErrorCode::Forbidden, "This PC is not a counter of a hub."))
}

#[tauri::command]
pub async fn device_status(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
) -> AppResult<DeviceStatus> {
    status(&node, &state)
}

#[tauri::command]
pub async fn device_discover_hubs() -> AppResult<Vec<FoundHub>> {
    tauri::async_runtime::spawn_blocking(|| crate::hub::discovery::browse(Duration::from_secs(3)))
        .await
        .unwrap_or_else(|e| Err(AppError::internal(e)))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInput {
    /// "192.168.1.20", "192.168.1.20:47900" or a full https URL.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub url: String,
    pub business_name: String,
    pub app_version: String,
    pub fingerprint: String,
    /// Compare with the code shown on the hub's screen before pairing.
    pub fingerprint_short: String,
}

#[tauri::command]
pub async fn device_probe_hub(input: ProbeInput) -> AppResult<ProbeResult> {
    let url = normalise_url(&input.url, 47900)?;
    let (health, fingerprint) = HubApi::probe(&url).await?;
    Ok(ProbeResult {
        url,
        business_name: health.business_name,
        app_version: health.app_version,
        fingerprint_short: short_fingerprint(&fingerprint),
        fingerprint,
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairDeviceInput {
    pub url: String,
    pub code: String,
    /// The fingerprint from `device_probe_hub` that the person confirmed.
    pub fingerprint: String,
    pub device_name: String,
}

/// Pairs this PC with a hub and makes it a counter (after a restart).
#[tauri::command]
pub async fn device_pair(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
    input: PairDeviceInput,
) -> AppResult<DeviceStatus> {
    require_owner_if_set_up(&state, token.as_deref())?;
    let url = normalise_url(&input.url, 47900)?;
    let api = HubApi::new(&url, &input.fingerprint, None)?;
    let paired = api
        .pair(&PairInput {
            code: input.code.clone(),
            device_name: input.device_name.clone(),
            app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        })
        .await
        .map_err(AppError::from)?;
    state.secrets.set(DEVICE_TOKEN, &paired.device_token)?;
    let config = DeviceConfig {
        mode: DeviceMode::Client,
        device_name: input.device_name.trim().to_string(),
        hub: Some(HubLink {
            url,
            fingerprint: input.fingerprint,
            device_id: paired.device_id,
            short_code: paired.short_code,
            business_name: paired.business_name,
            paired_at: state.now().to_rfc3339(),
        }),
    };
    config.save(&node.data_dir)?;
    *node.config.write().map_err(AppError::internal_lock)? = config;
    status(&node, &state)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetModeInput {
    /// "standalone" or "hub" (a counter is set up by pairing).
    pub mode: DeviceMode,
    pub device_name: Option<String>,
}

#[tauri::command]
pub async fn device_set_mode(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
    input: SetModeInput,
) -> AppResult<DeviceStatus> {
    if input.mode == DeviceMode::Client {
        return Err(AppError::field(
            "mode",
            "Pair with a hub to make this PC a counter.",
        ));
    }
    if let Some(client) = &node.client {
        let s = client.status();
        if s.pending > 0 || s.rejected > 0 {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "This counter still has offline sales that haven't reached the hub. Sync or review them first.",
            ));
        }
    } else {
        require_owner_if_set_up(&state, token.as_deref())?;
    }
    let mut config = node.config.read().map_err(AppError::internal_lock)?.clone();
    config.mode = input.mode;
    if let Some(name) = input
        .device_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        config.device_name = name.chars().take(60).collect();
    }
    if input.mode != DeviceMode::Client && config.hub.take().is_some() {
        state.secrets.delete(DEVICE_TOKEN)?;
    }
    config.save(&node.data_dir)?;
    *node.config.write().map_err(AppError::internal_lock)? = config;
    status(&node, &state)
}

#[tauri::command]
pub async fn device_restart<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<()> {
    app.restart()
}

#[tauri::command]
pub async fn client_sync_now(node: State<'_, Arc<Node>>) -> AppResult<ClientStatus> {
    client(&node)?.sync_once().await
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxQuery {
    #[serde(default)]
    pub include_done: bool,
}

fn signed_in(runtime: &ClientRuntime, token: Option<&str>) -> AppResult<auth::SessionUser> {
    runtime
        .cached_user(token.unwrap_or_default())?
        .ok_or_else(AppError::unauthorized)
}

#[tauri::command]
pub async fn outbox_list(
    node: State<'_, Arc<Node>>,
    token: Option<String>,
    input: Option<OutboxQuery>,
) -> AppResult<Vec<OutboxEntry>> {
    let runtime = client(&node)?;
    signed_in(runtime, token.as_deref())?;
    runtime.outbox(input.unwrap_or_default().include_done)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxRetryInput {
    pub id: String,
}

/// Sends a refused offline sale again, optionally with a manager's approval (for example for a
/// discount above the cashier's limit).
#[tauri::command]
pub async fn outbox_retry(
    node: State<'_, Arc<Node>>,
    token: Option<String>,
    input: OutboxRetryInput,
    approval: Option<ApprovalInput>,
) -> AppResult<ClientStatus> {
    let runtime = client(&node)?;
    signed_in(runtime, token.as_deref())?;
    runtime.retry(&input.id, approval.as_ref())?;
    runtime.sync_once().await
}

/// Drops a refused offline sale (for example one that was billed online after all). Needs
/// `sync.review`; the hub never saw it, so nothing is posted.
#[tauri::command]
pub async fn outbox_discard(
    node: State<'_, Arc<Node>>,
    token: Option<String>,
    input: IdInput,
) -> AppResult<ClientStatus> {
    let runtime = client(&node)?;
    let user = signed_in(runtime, token.as_deref())?;
    if !user.permissions.iter().any(|p| p == "sync.review") {
        return Err(AppError::forbidden("sync.review"));
    }
    tracing::warn!(user = user.id, "discarded offline sale {}", input.id);
    runtime.discard(&input.id)?;
    Ok(runtime.status())
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerSearch {
    #[serde(default)]
    pub term: String,
}

/// Customers saved on this counter (for the POS while the hub is offline).
#[tauri::command]
pub async fn client_cached_customers(
    node: State<'_, Arc<Node>>,
    token: Option<String>,
    input: CustomerSearch,
) -> AppResult<Vec<CachedCustomer>> {
    let runtime = client(&node)?;
    signed_in(runtime, token.as_deref())?;
    runtime.cached_customers(&input.term)
}

pub const NAMES: &[&str] = &[
    "device_status",
    "device_discover_hubs",
    "device_probe_hub",
    "device_pair",
    "device_set_mode",
    "device_restart",
    "client_sync_now",
    "outbox_list",
    "outbox_retry",
    "outbox_discard",
    "client_cached_customers",
];

pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
{
    tauri::generate_handler![
        device_status,
        device_discover_hubs,
        device_probe_hub,
        device_pair,
        device_set_mode,
        device_restart,
        client_sync_now,
        outbox_list,
        outbox_retry,
        outbox_discard,
        client_cached_customers,
    ]
}

/// Contract entries for the local commands (none of them needs a hub session).
pub(crate) fn specs(gen: &mut schemars::gen::SchemaGenerator) -> Vec<super::contract::Spec> {
    use super::contract::Spec;
    vec![
        Spec::new::<NoInput, DeviceStatus>(gen, "device_status", false),
        Spec::new::<NoInput, Vec<FoundHub>>(gen, "device_discover_hubs", false),
        Spec::new::<ProbeInput, ProbeResult>(gen, "device_probe_hub", false),
        Spec::new::<PairDeviceInput, DeviceStatus>(gen, "device_pair", false),
        Spec::new::<SetModeInput, DeviceStatus>(gen, "device_set_mode", false),
        Spec::new::<NoInput, ()>(gen, "device_restart", false),
        Spec::new::<NoInput, ClientStatus>(gen, "client_sync_now", false),
        Spec::new::<OutboxQuery, Vec<OutboxEntry>>(gen, "outbox_list", true),
        Spec::new::<OutboxRetryInput, ClientStatus>(gen, "outbox_retry", true),
        Spec::new::<IdInput, ClientStatus>(gen, "outbox_discard", true),
        Spec::new::<CustomerSearch, Vec<CachedCustomer>>(gen, "client_cached_customers", true),
    ]
}
