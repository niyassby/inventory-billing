//! The hub's LAN API (docs/ARCHITECTURE.md §3, §7): HTTPS on `network.port` (default 47900).
//!
//! * TLS with a self-signed certificate made on first start; counters pin its SHA-256 at pairing.
//! * Only private-network addresses (and `network.allowedSubnets`) may connect.
//! * Every call except `/v1/health` and `/v1/pair` needs the device credential
//!   (`x-device-token`); commands also need the user's session token.
//!
//! Routes:
//!
//! | Method | Path | Body | Result |
//! |---|---|---|---|
//! | GET | /v1/health | — | `Health` |
//! | POST | /v1/pair | `PairInput` | `Paired` |
//! | POST | /v1/call/{command} | `{ token, input, approval? }` | the command's output |
//! | GET | /v1/sync/catalogue | — | `CatalogueSnapshot` |
//! | POST | /v1/sync/lease | — | `Lease` |
//! | POST | /v1/sync/push | `PushRequest` | `PushResponse` |
//!
//! Errors are `{ code, message, details }` with a matching HTTP status.

pub mod discovery;
pub mod tls;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app::AppState;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::services::auth::ApprovalInput;
use crate::services::devices::{self, DeviceRef};
use crate::services::{settings, sync};

pub const DEVICE_HEADER: &str = "x-device-token";

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self.code {
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden | ErrorCode::ApprovalRequired => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict | ErrorCode::DocumentFinalized => StatusCode::CONFLICT,
            ErrorCode::RateLimited | ErrorCode::LockedOut => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        (status, Json(self)).into_response()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub ok: bool,
    pub business_name: String,
    pub hub_time: String,
    pub schema_version: i64,
    pub app_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallBody {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub input: Value,
    pub approval: Option<ApprovalInput>,
}

/// Private and local networks: what a shop LAN uses. Public addresses are refused unless the
/// owner lists their subnet in Settings → Network.
pub fn is_lan_address(ip: IpAddr, allowed: &[ipnet::IpNet]) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    };
    let private = match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link local fe80::/10
        }
    };
    private || allowed.iter().any(|net| net.contains(&ip))
}

async fn lan_only(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let allowed: Vec<ipnet::IpNet> = state
        .conn()
        .ok()
        .and_then(|c| settings::load(&c).ok())
        .map(|s| {
            s.network
                .allowed_subnets
                .iter()
                .filter_map(|n| n.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    if !is_lan_address(peer.ip(), &allowed) {
        tracing::warn!("refused LAN API call from {peer}");
        return AppError::new(
            ErrorCode::Forbidden,
            "Only devices on the shop network can connect.",
        )
        .into_response();
    }
    next.run(request).await
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(AppError::internal(format!("task failed: {e}"))))
}

async fn device(state: &AppState, headers: &HeaderMap) -> AppResult<DeviceRef> {
    let token = headers
        .get(DEVICE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Err(AppError::new(
            ErrorCode::Unauthorized,
            "This device is not paired with the hub.",
        ));
    }
    let state = state.clone();
    blocking(move || devices::authenticate_device(&*state.conn()?, &token, state.now())).await
}

async fn health(State(state): State<AppState>) -> AppResult<Json<Health>> {
    blocking(move || {
        let conn = state.conn()?;
        Ok(Json(Health {
            ok: true,
            business_name: settings::load(&conn)?.business.name,
            hub_time: state.now().to_rfc3339(),
            schema_version: conn.query_row(
                "SELECT coalesce(max(version), 0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )?,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    })
    .await
}

async fn pair(
    State(state): State<AppState>,
    Json(input): Json<devices::PairInput>,
) -> AppResult<Json<devices::Paired>> {
    blocking(move || {
        let mut conn = state.conn()?;
        devices::pair(&mut conn, &state.limiter, state.now(), &input).map(Json)
    })
    .await
}

async fn call(
    State(state): State<AppState>,
    Path(command): Path<String>,
    headers: HeaderMap,
    Json(body): Json<CallBody>,
) -> AppResult<Json<Value>> {
    let device = device(&state, &headers).await?;
    crate::api::dispatch(
        &state,
        &device.id,
        &command,
        body.token,
        body.input,
        body.approval,
    )
    .await
    .map(Json)
}

async fn catalogue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<sync::CatalogueSnapshot>> {
    device(&state, &headers).await?;
    blocking(move || sync::catalogue(&*state.conn()?, state.now()).map(Json)).await
}

async fn lease(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<sync::Lease>> {
    let device = device(&state, &headers).await?;
    blocking(move || {
        let mut conn = state.conn()?;
        sync::lease(&mut conn, &device, state.now()).map(Json)
    })
    .await
}

async fn push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<sync::PushRequest>,
) -> AppResult<Json<sync::PushResponse>> {
    let device = device(&state, &headers).await?;
    blocking(move || sync::push(&state, &device, &request).map(Json)).await
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/pair", post(pair))
        .route("/v1/call/:command", post(call))
        .route("/v1/sync/catalogue", get(catalogue))
        .route("/v1/sync/lease", post(lease))
        .route("/v1/sync/push", post(push))
        .layer(middleware::from_fn_with_state(state.clone(), lan_only))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(state)
}

/// A running hub server.
pub struct HubServer {
    pub addr: SocketAddr,
    pub fingerprint: String,
    handle: axum_server::Handle,
}

impl HubServer {
    pub fn stop(&self) {
        self.handle.graceful_shutdown(Some(Duration::from_secs(5)));
    }

    /// Stops at once, dropping open connections (tests: "the hub PC was switched off").
    pub fn kill(&self) {
        self.handle.shutdown();
    }
}

/// Starts the LAN API on `bind` (port 0 picks a free port, for tests).
pub async fn start(state: AppState, bind: SocketAddr) -> AppResult<HubServer> {
    let identity = tls::load_or_create(&state)?;
    let config =
        axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(identity.server_config()?));
    let handle = axum_server::Handle::new();
    let app = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server = axum_server::bind_rustls(bind, config).handle(handle.clone());
    tokio::spawn(async move {
        if let Err(e) = server.serve(app).await {
            tracing::error!("hub server stopped: {e}");
        }
    });
    let addr = handle
        .listening()
        .await
        .ok_or_else(|| AppError::internal(format!("the hub could not listen on {bind}")))?;
    tracing::info!("hub listening on https://{addr}");
    Ok(HubServer {
        addr,
        fingerprint: identity.fingerprint,
        handle,
    })
}

/// Addresses counters can use to reach this PC. The primary LAN address is found by asking the
/// OS which interface would route outwards (no packet is sent).
pub fn local_urls(port: u16) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("192.168.0.1:9").is_ok() || socket.connect("10.0.0.1:9").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if !addr.ip().is_unspecified() && !addr.ip().is_loopback() {
                    urls.push(format!("https://{}:{port}", addr.ip()));
                }
            }
        }
    }
    if let Ok(host) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        urls.push(format!("https://{}.local:{port}", host.to_lowercase()));
    }
    urls
}
