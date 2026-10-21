//! Hub side of client mode (docs/ARCHITECTURE.md §6):
//!
//! * `catalogue`: what a counter caches to keep billing while the hub is unreachable;
//! * `lease`: blocks of invoice numbers from the counter's own series, taken while online;
//! * `push`: the counter's outbox, applied in order. Every item is idempotent (same key → same
//!   result, never a second invoice), logged in `sync_log`, and either applied or rejected with a
//!   code. Rejected items stay on the counter for a manager to retry (with approval) or discard.
//!
//! Offline sales are real sales: they are posted even when they take stock below zero, and such
//! cases (and prices that changed meanwhile) are listed in `sync_exceptions` for review.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::audit::AuditEntry;
use super::auth::{self, ApprovalInput};
use super::catalog::{self, SearchItem};
use super::common::{Page, PageQuery};
use super::context::Ctx;
use super::devices::DeviceRef;
use super::numbering;
use super::sales::{self, InvoiceDetail, OfflineOrigin, SaleInput};
use super::settings::{self, OfflineSalesPolicy, Settings};
use super::write;
use crate::app::AppState;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};

// ------------------------------------------------------------------ exceptions

pub(crate) struct Exception<'a> {
    pub kind: &'a str,
    pub device_id: &'a str,
    pub source: (&'a str, &'a str),
    pub variant_id: Option<&'a str>,
    pub warehouse_id: Option<&'a str>,
    pub amount: i64,
    pub details: Value,
}

pub(crate) fn record_exception(conn: &Connection, e: &Exception<'_>, now: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO sync_exceptions (id, kind, device_id, source_type, source_id, variant_id, warehouse_id, amount,
            details_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            new_id(),
            e.kind,
            e.device_id,
            e.source.0,
            e.source.1,
            e.variant_id,
            e.warehouse_id,
            e.amount,
            e.details.to_string(),
            now
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncException {
    pub id: String,
    pub kind: String,
    pub device_name: Option<String>,
    pub source_type: String,
    pub source_id: String,
    pub doc_no: Option<String>,
    pub item_name: Option<String>,
    pub amount: i64,
    pub details: Option<Value>,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolution_note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    #[serde(default)]
    pub include_resolved: bool,
}

pub fn list_exceptions(ctx: &Ctx<'_>, query: &ExceptionQuery) -> AppResult<Page<SyncException>> {
    ctx.require("sync.review")?;
    let order = query.page.order_by(
        &[
            ("when", "e.created_at"),
            ("kind", "e.kind"),
            ("device", "lower(coalesce(d.name, ''))"),
            ("amount", "e.amount"),
        ],
        "e.created_at DESC, e.id",
        "e.id",
    )?;
    let (limit, offset) = query.page.limit_offset();
    let filter = if query.include_resolved {
        "1 = 1"
    } else {
        "e.resolved_at IS NULL"
    };
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) FROM sync_exceptions e WHERE {filter}"),
        [],
        |r| r.get(0),
    )?;
    let items = ctx
        .conn
        .prepare(&format!(
            "SELECT e.id, e.kind, d.name, e.source_type, e.source_id, i.doc_no,
                p.name || coalesce(' — ' || v.variant_name, ''), e.amount, e.details_json, e.created_at, e.resolved_at,
                e.resolution_note
             FROM sync_exceptions e
             LEFT JOIN devices d ON d.id = e.device_id
             LEFT JOIN sales_invoices i ON e.source_type = 'sales_invoice' AND i.id = e.source_id
             LEFT JOIN product_variants v ON v.id = e.variant_id
             LEFT JOIN products p ON p.id = v.product_id
             WHERE {filter} ORDER BY {order} LIMIT ?1 OFFSET ?2"
        ))?
        .query_map(params![limit, offset], |r| {
            Ok(SyncException {
                id: r.get(0)?,
                kind: r.get(1)?,
                device_name: r.get(2)?,
                source_type: r.get(3)?,
                source_id: r.get(4)?,
                doc_no: r.get(5)?,
                item_name: r.get(6)?,
                amount: r.get(7)?,
                details: r
                    .get::<_, Option<String>>(8)?
                    .and_then(|d| serde_json::from_str(&d).ok()),
                created_at: r.get(9)?,
                resolved_at: r.get(10)?,
                resolution_note: r.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveExceptionInput {
    pub id: String,
    /// What was done about it (a stock count, a price correction, nothing needed…).
    pub note: String,
}

pub fn resolve_exception(ctx: &mut Ctx<'_>, input: &ResolveExceptionInput) -> AppResult<()> {
    ctx.require("sync.review")?;
    let note = super::common::required("note", &input.note, 500)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let changed = tx.execute(
            "UPDATE sync_exceptions SET resolved_at = ?1, resolved_by = ?2, resolution_note = ?3
             WHERE id = ?4 AND resolved_at IS NULL",
            params![now, actor.user_id, note, input.id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("Open sync exception"));
        }
        AuditEntry::new(
            "SYNC_EXCEPTION_RESOLVED",
            "sync_exceptions",
            Some(&input.id),
        )
        .by(actor)
        .summary(note.clone())
        .write(tx, &now)
    })
}

// ------------------------------------------------------------------ catalogue

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CachedCustomer {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub gstin: Option<String>,
    pub state_code: Option<String>,
}

/// What a counter keeps locally for offline billing. Refreshed every
/// `network.catalogRefreshMinutes` and whenever the hub comes back.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueSnapshot {
    pub generated_at: String,
    pub settings: Settings,
    pub branch_id: String,
    pub branch_state: Option<String>,
    pub warehouse_id: String,
    pub items: Vec<SearchItem>,
    pub customers: Vec<CachedCustomer>,
}

pub fn catalogue(conn: &Connection, now: DateTime<Utc>) -> AppResult<CatalogueSnapshot> {
    let mut settings = settings::load(conn)?;
    // Counters don't need the hub's backup configuration.
    settings.backup = settings::BackupSettings::default();
    let (branch_id, warehouse_id) = super::setup::default_location(conn)?;
    let branch_state: Option<String> = conn.query_row(
        "SELECT state_code FROM branches WHERE id = ?1",
        [&branch_id],
        |r| r.get(0),
    )?;
    let items = catalog::offline_catalogue(conn, &warehouse_id)?;
    let customers = conn
        .prepare(
            "SELECT id, name, phone, gstin, state_code FROM parties
             WHERE party_type IN ('customer', 'both') AND is_active = 1 AND deleted_at IS NULL ORDER BY lower(name)",
        )?
        .query_map([], |r| {
            Ok(CachedCustomer {
                id: r.get(0)?,
                name: r.get(1)?,
                phone: r.get(2)?,
                gstin: r.get(3)?,
                state_code: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CatalogueSnapshot {
        generated_at: now.to_rfc3339(),
        settings,
        branch_id,
        branch_state,
        warehouse_id,
        items,
        customers,
    })
}

// ------------------------------------------------------------------ number leases

/// A block of invoice numbers a counter may use offline, with everything needed to format them
/// exactly as the hub will.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub lease_id: String,
    pub series_id: String,
    pub period: String,
    pub fy: String,
    pub prefix: String,
    pub template: String,
    pub padding: usize,
    pub branch_code: String,
    pub from_seq: i64,
    pub to_seq: i64,
}

/// Leases a new block for the device's offline invoice series (current financial year).
pub fn lease(conn: &mut Connection, device: &DeviceRef, now: DateTime<Utc>) -> AppResult<Lease> {
    let settings = settings::load(conn)?;
    if settings.network.offline_sales_policy == OfflineSalesPolicy::Block {
        return Err(AppError::new(
            ErrorCode::Forbidden,
            "Offline sales are turned off (Settings → Network).",
        ));
    }
    let now_str = now.to_rfc3339();
    let today = settings.business_date(now);
    let fy = settings.fy_label(&today);
    let size = settings.network.lease_size;
    write(conn, |tx| {
        let series_id = numbering::ensure_device_series(
            tx,
            &device.branch_id,
            &device.id,
            &device.short_code,
            "sales_invoice",
            &now_str,
        )?;
        let info = numbering::series_info(tx, &series_id)?;
        let period = if info.reset_policy == "yearly" {
            fy.clone()
        } else {
            "all".to_string()
        };
        let (lease_id, from_seq, to_seq) =
            numbering::lease_block(tx, &series_id, &device.id, &period, size, &now_str)?;
        Ok(Lease {
            lease_id,
            series_id,
            period,
            fy: fy.clone(),
            prefix: info.prefix,
            template: info.template,
            padding: info.padding,
            branch_code: info.branch_code,
            from_seq,
            to_seq,
        })
    })
}

// ------------------------------------------------------------------ push

/// An offline invoice as the counter queued it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OfflineInvoice {
    pub invoice_id: String,
    pub series_id: String,
    pub period: String,
    pub seq: i64,
    pub sale: SaleInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxItem {
    pub idempotency_key: String,
    pub device_seq: i64,
    /// Only "sales_create_invoice" can be queued offline.
    pub command: String,
    pub payload: Value,
    /// The cashier's session at the time of the sale (checked against the device and the time).
    pub session_token: String,
    /// Counter clock at the time of the sale.
    pub client_created_at: String,
    /// A manager's approval added when retrying a rejected item.
    pub approval: Option<ApprovalInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    /// Counter clock now; the difference to the hub clock corrects offline dates.
    pub client_now: String,
    pub items: Vec<OutboxItem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Applied,
    /// Already applied earlier (a retry); the stored result is returned again.
    Duplicate,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub idempotency_key: String,
    pub status: ItemStatus,
    pub error: Option<AppError>,
    pub invoice: Option<InvoiceDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushResponse {
    pub results: Vec<ItemResult>,
    /// Hub clock minus counter clock, in seconds.
    pub clock_skew_seconds: i64,
}

fn parse_time(field: &str, value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| AppError::field(field, "Expected an RFC 3339 timestamp."))
}

/// Errors that a retry can't fix are final; the counter keeps the others (connection trouble,
/// a busy database) pending and sends them again.
fn is_final(error: &AppError) -> bool {
    !matches!(error.code, ErrorCode::Internal | ErrorCode::RateLimited)
}

fn apply_item(
    state: &AppState,
    device: &DeviceRef,
    item: &OutboxItem,
    skew: chrono::Duration,
    max_skew: chrono::Duration,
) -> AppResult<InvoiceDetail> {
    if item.command != "sales_create_invoice" {
        return Err(AppError::validation(format!(
            "'{}' can't be queued offline.",
            item.command
        )));
    }
    let offline: OfflineInvoice = serde_json::from_value(item.payload.clone())
        .map_err(|e| AppError::validation(format!("Invalid offline invoice: {e}")))?;
    if uuid::Uuid::parse_str(&offline.invoice_id).is_err() {
        return Err(AppError::field("invoiceId", "Expected a UUID."));
    }
    if offline.sale.idempotency_key != item.idempotency_key {
        return Err(AppError::validation(
            "The item and the invoice carry different request ids.",
        ));
    }
    let client_created = parse_time("clientCreatedAt", &item.client_created_at)?;
    let corrected_at = client_created + skew;
    let mut conn = state.conn()?;
    let now = state.now();
    let actor = auth::offline_actor(&conn, &item.session_token, &device.id, corrected_at)?;
    let approver = item
        .approval
        .as_ref()
        .map(|a| auth::verify_approver(&mut conn, &state.limiter, now, a))
        .transpose()?;
    let origin = OfflineOrigin {
        invoice_id: offline.invoice_id.clone(),
        device_id: device.id.clone(),
        client_created_at: item.client_created_at.clone(),
        corrected_at,
        series_id: offline.series_id,
        period: offline.period,
        seq: offline.seq,
    };
    let mut ctx = Ctx {
        conn: &mut conn,
        actor: &actor,
        approver: approver.as_ref(),
        now,
    };
    let invoice = sales::create_invoice_inner(&mut ctx, &offline.sale, false, Some(&origin))?;
    if skew.abs() > max_skew {
        let already: i64 = conn.query_row(
            "SELECT count(*) FROM sync_exceptions WHERE kind = 'clock_skew' AND source_id = ?1",
            [&invoice.id],
            |r| r.get(0),
        )?;
        if already == 0 {
            record_exception(
                &conn,
                &Exception {
                    kind: "clock_skew",
                    device_id: &device.id,
                    source: ("sales_invoice", &invoice.id),
                    variant_id: None,
                    warehouse_id: None,
                    amount: skew.num_seconds(),
                    details: json!({ "docNo": invoice.doc_no, "clientCreatedAt": item.client_created_at }),
                },
                &now.to_rfc3339(),
            )?;
        }
    }
    Ok(invoice)
}

/// Applies a counter's outbox in device order. Stops at the first item that failed for a
/// temporary reason, so later sales never overtake earlier ones.
pub fn push(
    state: &AppState,
    device: &DeviceRef,
    request: &PushRequest,
) -> AppResult<PushResponse> {
    if request.items.len() > 200 {
        return Err(AppError::validation("Send at most 200 items at a time."));
    }
    let now = state.now();
    let client_now = parse_time("clientNow", &request.client_now)?;
    let skew = now - client_now;
    let max_skew = {
        let conn = state.conn()?;
        chrono::Duration::hours(settings::load(&conn)?.network.max_clock_skew_hours)
    };
    let mut items: Vec<&OutboxItem> = request.items.iter().collect();
    items.sort_by_key(|i| i.device_seq);
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        let seen: Option<String> = state
            .conn()?
            .query_row(
                "SELECT status FROM sync_log WHERE device_id = ?1 AND idempotency_key = ?2",
                params![device.id, item.idempotency_key],
                |r| r.get(0),
            )
            .optional()?;
        let outcome = apply_item(state, device, item, skew, max_skew);
        let (status, error, invoice) = match outcome {
            Ok(invoice) => {
                let status = if seen.as_deref() == Some("applied") {
                    ItemStatus::Duplicate
                } else {
                    ItemStatus::Applied
                };
                (status, None, Some(invoice))
            }
            Err(e) if !is_final(&e) => {
                results.push(ItemResult {
                    idempotency_key: item.idempotency_key.clone(),
                    status: ItemStatus::Rejected,
                    error: Some(e),
                    invoice: None,
                });
                break;
            }
            Err(e) => (ItemStatus::Rejected, Some(e), None),
        };
        let log_status = match status {
            ItemStatus::Applied | ItemStatus::Duplicate => "applied",
            ItemStatus::Rejected => "rejected",
        };
        state.conn()?.execute(
            "INSERT INTO sync_log (id, device_id, idempotency_key, device_seq, command, status, error_code, client_time, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (device_id, idempotency_key) DO UPDATE SET status = excluded.status,
               error_code = excluded.error_code, received_at = excluded.received_at",
            params![
                new_id(),
                device.id,
                item.idempotency_key,
                item.device_seq,
                item.command,
                log_status,
                error.as_ref().map(|e| e.code.as_str()),
                item.client_created_at,
                now.to_rfc3339()
            ],
        )?;
        results.push(ItemResult {
            idempotency_key: item.idempotency_key.clone(),
            status,
            error,
            invoice,
        });
    }
    Ok(PushResponse {
        results,
        clock_skew_seconds: skew.num_seconds(),
    })
}
