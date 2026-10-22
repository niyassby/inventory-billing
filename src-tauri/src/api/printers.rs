//! Printing commands about this PC's own printers. Like the device commands (local.rs) they always
//! run here, even on a counter, because the printer is plugged into this PC. They take the usual
//! `{ token, input, approval? }` envelope and check the session through the normal v2 path
//! (`auth_me`, which a counter answers from its cache while the hub is offline).
//!
//! A document routed to a printer the hub shares goes to the hub's print queue
//! (`print_jobs_submit`); everything else is rendered and sent from here.

use std::path::PathBuf;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use super::local::Node;
use super::Caller;
use crate::app::AppState;
use crate::client::ClientRuntime;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::printing::config::{PrinterConfig, PrinterKind, PrinterProfile};
use crate::printing::transport::{self, Discovered, FailureKind};
use crate::printing::{self, escpos, PrintDocument, PrintOutcome, PrintStatus};
use crate::services::auth::{ApprovalInput, SessionUser};
use crate::services::common::NoInput;
use crate::services::printing::{print_permissions, PrintJob, SharedPrinter};
use crate::services::sales::InvoiceDetail;
use crate::services::settings::PublicSettings;

/// What the printing commands need: the app state, where printers.json lives, and (on a
/// counter) the connection to the hub.
#[derive(Clone)]
pub struct PrintEnv {
    pub state: AppState,
    pub data_dir: PathBuf,
    pub client: Option<Arc<ClientRuntime>>,
}

impl PrintEnv {
    fn from_node(node: &Node, state: &AppState) -> Self {
        Self {
            state: state.clone(),
            data_dir: node.data_dir.clone(),
            client: node.client.clone(),
        }
    }

    /// A v2 command, exactly as the window would send it (to the hub on a counter).
    async fn v2(
        &self,
        name: &str,
        token: &str,
        input: Value,
        approval: Option<&ApprovalInput>,
    ) -> AppResult<Value> {
        match &self.client {
            Some(runtime) => {
                runtime
                    .call(
                        name,
                        json!({ "token": token, "input": input, "approval": approval }),
                    )
                    .await
            }
            None => {
                super::dispatch_as(
                    &self.state,
                    Caller::Local,
                    name,
                    token.to_string(),
                    input,
                    approval.cloned(),
                )
                .await
            }
        }
    }

    async fn user(&self, token: &str) -> AppResult<SessionUser> {
        let value = self.v2("auth_me", token, json!({}), None).await?;
        Ok(serde_json::from_value(value)?)
    }

    async fn settings(&self, token: &str) -> AppResult<PublicSettings> {
        let value = self.v2("settings_public", token, json!({}), None).await?;
        Ok(serde_json::from_value(value)?)
    }

    fn config(&self) -> PrinterConfig {
        PrinterConfig::load(&self.data_dir)
    }
}

fn can(user: &SessionUser, permission: &str) -> bool {
    user.permissions.iter().any(|p| p == permission)
}

fn require(user: &SessionUser, permission: &str) -> AppResult<()> {
    if can(user, permission) {
        Ok(())
    } else {
        Err(AppError::forbidden(permission))
    }
}

/// Sends bytes on a blocking thread (network and serial writes wait for the printer).
async fn send(profile: &PrinterProfile, bytes: Vec<u8>) -> AppResult<()> {
    let name = profile.name.clone();
    let connection = profile.connection.clone();
    tauri::async_runtime::spawn_blocking(move || transport::send(&name, &connection, &bytes))
        .await
        .unwrap_or_else(|e| Err(AppError::internal(format!("print task failed: {e}"))))
}

// ------------------------------------------------------------------ configuration

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrinterSetup {
    pub config: PrinterConfig,
    /// A font for pictures of text (₹, Hindi, Malayalam) was found on this PC.
    pub picture_font: bool,
    /// This PC is a counter: it can also print to the hub's shared printers.
    pub is_counter: bool,
    /// The hub's shared printers (counters only; empty when the hub can't be reached).
    pub hub_printers: Vec<SharedPrinter>,
}

async fn setup(env: &PrintEnv, token: &str) -> PrinterSetup {
    let config = env.config();
    let picture_font = printing::font(&config).is_some();
    let hub_printers = if env.client.is_some() {
        match env.v2("printers_shared", token, json!({}), None).await {
            Ok(value) => serde_json::from_value(value).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };
    PrinterSetup {
        config,
        picture_font,
        is_counter: env.client.is_some(),
        hub_printers,
    }
}

/// This PC's printers and routes (every signed-in user: the till needs the routes).
pub async fn config_get(env: &PrintEnv, token: &str) -> AppResult<PrinterSetup> {
    env.user(token).await?;
    Ok(setup(env, token).await)
}

pub async fn config_save(
    env: &PrintEnv,
    token: &str,
    input: PrinterConfig,
) -> AppResult<PrinterSetup> {
    let user = env.user(token).await?;
    require(&user, "printing.manage")?;
    let config = input.validated()?;
    config.save(&env.data_dir)?;
    tracing::info!(
        user = user.id,
        "printer settings saved: {}",
        config.summary()
    );
    // The shared audit log lives on the hub; a counter that can't reach it still saves (printers
    // must be fixable during an outage) and the change stays in this PC's log.
    match env
        .v2(
            "printers_record_change",
            token,
            json!({ "summary": config.summary() }),
            None,
        )
        .await
    {
        Ok(_) => {}
        Err(e) if e.code == ErrorCode::HubOffline => {
            tracing::warn!("printer change not in the hub's audit log (hub offline)");
        }
        Err(e) => return Err(e),
    }
    Ok(setup(env, token).await)
}

pub async fn discover(env: &PrintEnv, token: &str) -> AppResult<Discovered> {
    require(&env.user(token).await?, "printing.manage")?;
    tauri::async_runtime::spawn_blocking(transport::discover)
        .await
        .map_err(|e| AppError::internal(format!("discovery failed: {e}")))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrinterTestInput {
    /// The printer as typed in the form (it need not be saved yet).
    pub printer: PrinterProfile,
}

/// Prints the test page (or a test label) on a printer, saved or not.
pub async fn test(env: &PrintEnv, token: &str, input: PrinterTestInput) -> AppResult<PrintOutcome> {
    require(&env.user(token).await?, "printing.manage")?;
    let profile = input.printer;
    if let Err((field, message)) = transport::validate(&profile.connection) {
        return Err(AppError::field(&field, message));
    }
    let settings = env.settings(token).await?;
    let config = env.config();
    let bytes = printing::render(
        &PrintDocument::Test,
        &profile,
        &settings,
        env.state.now(),
        printing::font(&config),
        false,
    )?;
    send(&profile, bytes).await?;
    Ok(PrintOutcome {
        status: PrintStatus::Printed,
        printer_name: Some(profile.name),
        job_id: None,
        message: None,
    })
}

// ------------------------------------------------------------------ printing

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintInput {
    pub document: PrintDocument,
    /// A printer of this PC, instead of the document's route.
    #[serde(default)]
    pub printer_id: Option<String>,
}

/// Prints a document where its route says (or on the printer named). Without a route it answers
/// `no_printer` and the screen prints through the system dialog instead.
pub async fn print(env: &PrintEnv, token: &str, input: PrintInput) -> AppResult<PrintOutcome> {
    let user = env.user(token).await?;
    let wanted = print_permissions(&input.document);
    if !wanted.iter().any(|p| can(&user, p)) {
        return Err(AppError::forbidden(
            wanted.first().copied().unwrap_or("sales.view"),
        ));
    }
    let config = env.config();
    let route = match (&input.printer_id, input.document.kind()) {
        (Some(id), _) => Some(printing::config::PrintRoute {
            printer_id: id.clone(),
            on_hub: false,
        }),
        (None, Some(kind)) => config.routes.get(kind).cloned(),
        (None, None) => None,
    };
    let Some(route) = route else {
        return Ok(PrintOutcome {
            status: PrintStatus::NoPrinter,
            printer_name: None,
            job_id: None,
            message: None,
        });
    };
    let open_drawer = input.document.takes_cash() && config.drawer_on_cash;
    let outcome = if route.on_hub {
        let value = env
            .v2(
                "print_jobs_submit",
                token,
                json!({ "idempotencyKey": new_id(), "printerId": route.printer_id, "document": input.document }),
                None,
            )
            .await?;
        let job: PrintJob = serde_json::from_value(value)?;
        PrintOutcome {
            status: if job.status == "printed" {
                PrintStatus::Printed
            } else {
                PrintStatus::Queued
            },
            printer_name: Some(job.printer_name),
            message: job.last_error,
            job_id: Some(job.id),
        }
    } else {
        let profile = config.printer(&route.printer_id).ok_or_else(|| {
            transport::printer_error(
                FailureKind::NotConfigured,
                &route.printer_id,
                "no such printer on this PC",
            )
        })?;
        let settings = env.settings(token).await?;
        let kick = open_drawer && config.drawer_printer_id.as_deref() == Some(profile.id.as_str());
        let bytes = printing::render(
            &input.document,
            profile,
            &settings,
            env.state.now(),
            printing::font(&config),
            kick,
        )?;
        send(profile, bytes).await?;
        PrintOutcome {
            status: PrintStatus::Printed,
            printer_name: Some(profile.name.clone()),
            job_id: None,
            message: None,
        }
    };
    // The drawer is on another printer than the one that printed: pulse it separately.
    if open_drawer
        && (route.on_hub || config.drawer_printer_id.as_deref() != Some(route.printer_id.as_str()))
    {
        if let Some(drawer) = config
            .drawer_printer_id
            .as_deref()
            .and_then(|id| config.printer(id))
        {
            if let Err(e) = send(drawer, escpos::drawer_only(drawer)).await {
                tracing::warn!("cash drawer did not open: {}", e.message);
            }
        }
    }
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrawerOpenInput {
    /// The cash sale just saved: the drawer opens for it without approval (auto-print off).
    #[serde(default)]
    pub invoice: Option<Box<InvoiceDetail>>,
    /// Why the drawer is opened without a sale.
    #[serde(default)]
    pub reason: Option<String>,
}

/// A sale saved within this many minutes may open the drawer without approval.
const CASH_SALE_WINDOW_MINUTES: i64 = 10;

fn recent_cash_sale(invoice: &InvoiceDetail, now: chrono::DateTime<chrono::Utc>) -> bool {
    let doc = PrintDocument::Receipt {
        invoice: Box::new(invoice.clone()),
        reprint: false,
    };
    let recent = invoice
        .finalized_at
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t| {
            (now - t.with_timezone(&chrono::Utc)).num_minutes().abs() <= CASH_SALE_WINDOW_MINUTES
        });
    doc.takes_cash() && recent && invoice.status == "finalized"
}

/// Opens the cash drawer: for a cash sale just made, or by hand (`sales.open_drawer` or a
/// manager's approval, recorded in the audit log).
pub async fn open_drawer(
    env: &PrintEnv,
    token: &str,
    input: DrawerOpenInput,
    approval: Option<&ApprovalInput>,
) -> AppResult<()> {
    let user = env.user(token).await?;
    let config = env.config();
    let drawer = config
        .drawer_printer_id
        .as_deref()
        .and_then(|id| config.printer(id))
        .filter(|p| p.kind == PrinterKind::Receipt && p.has_drawer)
        .ok_or_else(|| {
            transport::printer_error(
                FailureKind::NotConfigured,
                "cash drawer",
                "no drawer printer set",
            )
        })?;
    let for_sale = input
        .invoice
        .as_deref()
        .is_some_and(|i| recent_cash_sale(i, env.state.now()));
    if for_sale {
        if !(can(&user, "sales.create")) {
            return Err(AppError::forbidden("sales.create"));
        }
    } else {
        match env
            .v2(
                "drawer_authorize",
                token,
                json!({ "reason": input.reason }),
                approval,
            )
            .await
        {
            Ok(_) => {}
            // Offline, only someone who may open it themselves can (no approvals without the hub).
            Err(e) if e.code == ErrorCode::HubOffline && can(&user, "sales.open_drawer") => {
                tracing::warn!(user = user.id, "drawer opened offline: {:?}", input.reason);
            }
            Err(e) => return Err(e),
        }
    }
    send(drawer, escpos::drawer_only(drawer)).await
}

// ------------------------------------------------------------------ the browser bridge

/// Runs a printing command from JSON (the development bridge, which is a standalone PC).
pub async fn dispatch(
    env: &PrintEnv,
    name: &str,
    token: &str,
    input: Value,
    approval: Option<&ApprovalInput>,
) -> AppResult<Value> {
    fn decode<T: serde::de::DeserializeOwned>(input: Value) -> AppResult<T> {
        super::decode(input)
    }
    Ok(match name {
        "printers_config_get" => serde_json::to_value(config_get(env, token).await?)?,
        "printers_config_save" => {
            serde_json::to_value(config_save(env, token, decode(input)?).await?)?
        }
        "printers_discover" => serde_json::to_value(discover(env, token).await?)?,
        "printers_test" => serde_json::to_value(test(env, token, decode(input)?).await?)?,
        "print_document" => serde_json::to_value(print(env, token, decode(input)?).await?)?,
        "drawer_open" => {
            serde_json::to_value(open_drawer(env, token, decode(input)?, approval).await?)?
        }
        _ => return Err(AppError::not_found("That command")),
    })
}

// ------------------------------------------------------------------ Tauri commands

#[tauri::command]
pub async fn printers_config_get(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
) -> AppResult<PrinterSetup> {
    config_get(
        &PrintEnv::from_node(&node, &state),
        &token.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
pub async fn printers_config_save(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
    input: PrinterConfig,
) -> AppResult<PrinterSetup> {
    config_save(
        &PrintEnv::from_node(&node, &state),
        &token.unwrap_or_default(),
        input,
    )
    .await
}

#[tauri::command]
pub async fn printers_discover(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
) -> AppResult<Discovered> {
    discover(
        &PrintEnv::from_node(&node, &state),
        &token.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
pub async fn printers_test(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
    input: PrinterTestInput,
) -> AppResult<PrintOutcome> {
    test(
        &PrintEnv::from_node(&node, &state),
        &token.unwrap_or_default(),
        input,
    )
    .await
}

#[tauri::command]
pub async fn print_document(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
    input: PrintInput,
) -> AppResult<PrintOutcome> {
    print(
        &PrintEnv::from_node(&node, &state),
        &token.unwrap_or_default(),
        input,
    )
    .await
}

#[tauri::command]
pub async fn drawer_open(
    node: State<'_, Arc<Node>>,
    state: State<'_, AppState>,
    token: Option<String>,
    input: DrawerOpenInput,
    approval: Option<ApprovalInput>,
) -> AppResult<()> {
    open_drawer(
        &PrintEnv::from_node(&node, &state),
        &token.unwrap_or_default(),
        input,
        approval.as_ref(),
    )
    .await
}

pub const NAMES: &[&str] = &[
    "printers_config_get",
    "printers_config_save",
    "printers_discover",
    "printers_test",
    "print_document",
    "drawer_open",
];

pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
{
    tauri::generate_handler![
        printers_config_get,
        printers_config_save,
        printers_discover,
        printers_test,
        print_document,
        drawer_open,
    ]
}

/// Contract entries (all need a session).
pub(crate) fn specs(gen: &mut schemars::gen::SchemaGenerator) -> Vec<super::contract::Spec> {
    use super::contract::Spec;
    vec![
        Spec::new::<NoInput, PrinterSetup>(gen, "printers_config_get", true),
        Spec::new::<PrinterConfig, PrinterSetup>(gen, "printers_config_save", true),
        Spec::new::<NoInput, Discovered>(gen, "printers_discover", true),
        Spec::new::<PrinterTestInput, PrintOutcome>(gen, "printers_test", true),
        Spec::new::<PrintInput, PrintOutcome>(gen, "print_document", true),
        Spec::new::<DrawerOpenInput, ()>(gen, "drawer_open", true),
    ]
}
