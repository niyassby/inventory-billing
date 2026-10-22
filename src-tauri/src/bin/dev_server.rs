//! Development bridge: the v2 commands over plain HTTP on 127.0.0.1, so the screens can run in
//! an ordinary browser (`npm run dev:web`) and in Playwright against the real backend.
//!
//! Never shipped: Tauri bundles only the main binary. It listens on loopback only and uses its
//! own data folder (default `.dev-data/` in the repository, ignored by git).
//!
//! ```text
//! cargo run --bin dev-server -- [--data DIR] [--port 1430] [--demo]
//! ```
//!
//! `--demo` sets up a sample shop on an empty data folder (see `DEMO_*` below; test data only).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri_app_lib::api::{self, Caller};
use tauri_app_lib::app::{AppState, SystemClock};
use tauri_app_lib::error::AppError;
use tauri_app_lib::services::auth::ApprovalInput;

/// Sample shop for `--demo` (development data only, never a real account).
const DEMO_EMAIL: &str = "demo@example.test";
const DEMO_PASSWORD: &str = "Demo-shop-2026";

#[derive(Deserialize)]
struct CallBody {
    #[serde(default)]
    token: String,
    #[serde(default)]
    input: Value,
    approval: Option<ApprovalInput>,
}

async fn call(
    State(state): State<AppState>,
    Path(command): Path<String>,
    Json(body): Json<CallBody>,
) -> Response {
    // Commands about "this PC": the bridge is always a standalone PC.
    if command == "device_status" {
        return Json(json!({
            "mode": "standalone", "runningMode": "standalone", "deviceName": "Browser (dev)",
            "hubLink": null, "client": null, "hub": null, "restartRequired": false
        }))
        .into_response();
    }
    // Printing: the bridge is a standalone PC whose printers.json sits in its data folder.
    if api::printers::NAMES.contains(&command.as_str()) {
        let env = api::printers::PrintEnv {
            data_dir: state
                .db_path
                .parent()
                .map(std::path::PathBuf::from)
                .unwrap_or_default(),
            state: state.clone(),
            client: None,
        };
        return match api::printers::dispatch(
            &env,
            &command,
            &body.token,
            body.input,
            body.approval.as_ref(),
        )
        .await
        {
            Ok(value) => Json(value).into_response(),
            Err(e) => e.into_response(),
        };
    }
    if api::local::NAMES.contains(&command.as_str()) {
        return AppError::validation("Not available in the browser bridge.").into_response();
    }
    match api::dispatch_as(
        &state,
        Caller::Local,
        &command,
        body.token,
        body.input,
        body.approval,
    )
    .await
    {
        Ok(value) => Json(value).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Lets the Vite dev server (any localhost port) call the bridge.
async fn cors(request: Request, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        .filter(|o| o.starts_with("http://localhost:") || o.starts_with("http://127.0.0.1:"))
        .map(String::from);
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    if let Some(origin) = origin.and_then(|o| HeaderValue::from_str(&o).ok()) {
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("POST, OPTIONS"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("content-type"),
        );
    }
    response
}

fn demo(state: &AppState) -> Result<(), AppError> {
    use tauri_app_lib::services::setup::{self, OwnerInput, SetupInput};
    let mut conn = state.conn()?;
    if !setup::needs_setup(&conn)? {
        return Ok(());
    }
    setup::create_business(
        &mut conn,
        state.now(),
        &SetupInput {
            business_name: "Demo Stores".into(),
            address: Some("MG Road, Bengaluru".into()),
            phone: Some("9876543210".into()),
            email: None,
            gstin: Some("29AAGCB7383J1Z4".into()),
            state_code: None,
            currency: None,
            currency_minor_digits: None,
            timezone: None,
            fy_start_month: None,
            tax_regime: None,
            prices_include_tax: Some(true),
            owner: OwnerInput {
                name: "Demo Owner".into(),
                email: DEMO_EMAIL.into(),
                password: DEMO_PASSWORD.into(),
            },
        },
    )?;
    eprintln!("demo shop created: {DEMO_EMAIL} / {DEMO_PASSWORD}");
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut data = PathBuf::from(".dev-data");
    let mut port: u16 = 1430;
    let mut with_demo = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" => data = args.next().map(PathBuf::from).expect("--data DIR"),
            "--port" => port = args.next().and_then(|p| p.parse().ok()).expect("--port N"),
            "--demo" => with_demo = true,
            other => panic!("unknown argument {other}"),
        }
    }
    tracing_subscriber::fmt().with_env_filter("info").init();
    let state = AppState::open(&data.join("inventory_v2.db"), Arc::new(SystemClock))
        .expect("open database");
    if with_demo {
        demo(&state).expect("demo data");
    }
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/call/:command", post(call))
        .layer(middleware::from_fn(cors))
        .with_state(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    eprintln!("dev bridge on http://{addr} (data in {})", data.display());
    axum::serve(listener, app).await.expect("serve");
}
