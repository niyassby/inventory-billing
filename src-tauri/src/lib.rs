mod auth;

// The v1 database code is no longer used by the app (Phase 5 cutover). It stays, for tests only,
// to build real v1 databases for the legacy-migration tests.
#[cfg(test)]
#[allow(
    dead_code,
    clippy::too_many_arguments,
    reason = "v1 code, kept unchanged as a test fixture"
)]
mod database;
#[cfg(test)]
#[allow(dead_code, reason = "v1 code, kept unchanged as a test fixture")]
mod models;
#[cfg(test)]
#[allow(
    dead_code,
    clippy::too_many_arguments,
    reason = "v1 code, kept unchanged as a test fixture"
)]
mod repositories;

// v2 backend (Phases 2–3). Public so the maintenance binaries in src/bin can use them.
pub mod api;
pub mod app;
pub mod client;
pub mod db;
pub mod domain;
pub mod error;
pub mod hub;
pub mod jobs;
pub mod legacy_migration;
pub mod logging;
pub mod node;
pub mod printing;
pub mod secrets;
pub mod services;

use std::sync::{Arc, OnceLock};

use tauri::{Emitter, Manager};

/// Set in client mode: v2 commands then go to the hub instead of the local database.
static CLIENT: OnceLock<Arc<client::ClientRuntime>> = OnceLock::new();

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| data_dir.join("logs"));
            logging::init(&log_dir);

            // v2 database: the new backend (docs/DATABASE.md).
            let secrets: Arc<dyn secrets::SecretStore> =
                Arc::new(secrets::KeyringSecrets::new(&app.config().identifier));
            let state = app::AppState::open(
                &data_dir.join("inventory_v2.db"),
                Arc::new(app::SystemClock),
            )?
            .with_secrets(secrets.clone());
            tracing::info!("v2 database ready at {}", state.db_path.display());

            // This PC's role: standalone, hub or counter (docs/ARCHITECTURE.md §3).
            let config = node::DeviceConfig::load(&data_dir);
            let mut running_mode = config.mode;
            let mut client_runtime = None;
            if config.mode == node::DeviceMode::Client {
                match (
                    config.hub.clone(),
                    secrets.get(secrets::DEVICE_TOKEN).ok().flatten(),
                ) {
                    (Some(link), Some(token)) => {
                        let runtime = client::ClientRuntime::new(
                            &data_dir,
                            link,
                            token,
                            state.clock.clone(),
                        )?;
                        let handle = app.handle().clone();
                        runtime.on_status(Box::new(move |status| {
                            let _ = handle.emit("hub-status", status);
                        }));
                        runtime.spawn();
                        let _ = CLIENT.set(runtime.clone());
                        client_runtime = Some(runtime);
                    }
                    _ => {
                        tracing::warn!("client mode without a pairing; running standalone");
                        running_mode = node::DeviceMode::Standalone;
                    }
                }
            } else {
                jobs::spawn(state.clone());
            }
            let node = Arc::new(api::local::Node {
                data_dir: data_dir.clone(),
                config: std::sync::RwLock::new(config),
                running_mode,
                client: client_runtime,
                hub_server: std::sync::Mutex::new(None),
                advertisement: std::sync::Mutex::new(None),
            });
            if running_mode == node::DeviceMode::Hub {
                start_hub(state.clone(), node.clone());
            }
            app.manage(node);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(route())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Starts the LAN API and the mDNS announcement in the background; failures are shown in
/// Settings → Network (hub_status), not fatal.
fn start_hub(state: app::AppState, node: Arc<api::local::Node>) {
    tauri::async_runtime::spawn(async move {
        let (port, business) = match state.conn().and_then(|c| services::settings::load(&c)) {
            Ok(s) => (s.network.port, s.business.name),
            Err(e) => {
                tracing::error!("hub: settings unavailable: {}", e.message);
                return;
            }
        };
        let bind = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        match hub::start(state.clone(), bind).await {
            Ok(server) => {
                let short = node::short_fingerprint(&server.fingerprint);
                let urls = hub::local_urls(port);
                if let Ok(mut info) = state.hub.write() {
                    *info = node::HubInfo {
                        running: true,
                        port,
                        fingerprint: server.fingerprint.clone(),
                        fingerprint_short: short.clone(),
                        urls,
                        error: None,
                    };
                }
                match hub::discovery::advertise(&business, port, &short) {
                    Ok(ad) => {
                        if let Ok(mut slot) = node.advertisement.lock() {
                            *slot = Some(ad);
                        }
                    }
                    Err(e) => tracing::warn!("hub: mDNS announcement failed: {}", e.message),
                }
                if let Ok(mut slot) = node.hub_server.lock() {
                    *slot = Some(server);
                }
            }
            Err(e) => {
                tracing::error!("hub: could not start: {}", e.message);
                if let Ok(mut info) = state.hub.write() {
                    info.running = false;
                    info.port = port;
                    info.error = Some(e.message);
                }
            }
        }
    });
}

/// One invoke handler: local device and printer commands, and v2 commands (sent to the hub when this PC is
/// a counter).
fn route<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    let v2 = api::handler::<R>();
    let local = api::local::handler::<R>();
    let printers = api::printers::handler::<R>();
    move |invoke| {
        let command = invoke.message.command();
        if api::local::NAMES.contains(&command) {
            local(invoke)
        } else if api::printers::NAMES.contains(&command) {
            printers(invoke)
        } else {
            match CLIENT.get() {
                Some(runtime) if api::NAMES.contains(&command) => {
                    forward_to_hub(runtime.clone(), invoke);
                    true
                }
                _ => v2(invoke),
            }
        }
    }
}

/// Client mode: hands a v2 call (its raw `{ token, input, approval }`) to the hub, or to the
/// offline handlers when the hub can't be reached.
fn forward_to_hub<R: tauri::Runtime>(
    runtime: Arc<client::ClientRuntime>,
    invoke: tauri::ipc::Invoke<R>,
) {
    let name = invoke.message.command().to_string();
    let payload = match invoke.message.payload() {
        tauri::ipc::InvokeBody::Json(value) => value.clone(),
        tauri::ipc::InvokeBody::Raw(_) => serde_json::Value::Null,
    };
    invoke.resolver.respond_async(async move {
        runtime
            .call(&name, payload)
            .await
            .map_err(tauri::ipc::InvokeError::from)
    });
}
