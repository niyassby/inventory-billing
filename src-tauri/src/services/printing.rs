//! Printing on the hub's side: the print queue counters send documents to, the printers the hub
//! shares, label templates and the items to label, internal barcodes, and the audit of printer
//! changes and manual drawer openings. The printers themselves and the bytes are crate::printing.
//!
//! A queued job is rendered once, when it arrives, and kept: a retry sends the same bytes. Sending
//! happens after the job is committed, never inside a transaction, so a slow printer can't hold
//! the database.

use std::path::PathBuf;

use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::audit::AuditEntry;
use super::common::{Page, PageQuery};
use super::users::IdInput;
use super::{idempotency, settings, write, Ctx};
use crate::app::AppState;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::printing::barcode::{self, Symbology};
use crate::printing::config::{PaperWidth, PrinterConfig, PrinterKind};
use crate::printing::labels::{self, LabelItem, LabelTemplate};
use crate::printing::{self, transport, PrintDocument};

pub fn data_dir(state: &AppState) -> PathBuf {
    state
        .db_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Who may print a document (any one of these).
pub fn print_permissions(doc: &PrintDocument) -> &'static [&'static str] {
    match doc {
        PrintDocument::Receipt { .. }
        | PrintDocument::ReturnSlip { .. }
        | PrintDocument::HeldSlip { .. } => &["sales.create", "sales.view"],
        PrintDocument::TillClose { .. } => &["sales.create", "day_close.perform"],
        PrintDocument::Labels { .. } => &["labels.print"],
        PrintDocument::Test => &["printing.manage"],
    }
}

pub fn require_any(
    permissions: &std::collections::HashSet<String>,
    wanted: &[&str],
) -> AppResult<()> {
    if wanted.iter().any(|p| permissions.contains(*p)) {
        Ok(())
    } else {
        Err(AppError::forbidden(
            wanted.first().copied().unwrap_or("printing.manage"),
        ))
    }
}

fn doc_type(doc: &PrintDocument) -> &'static str {
    match doc {
        PrintDocument::Receipt { .. } => "receipt",
        PrintDocument::ReturnSlip { .. } => "return_slip",
        PrintDocument::TillClose { .. } => "till_close",
        PrintDocument::HeldSlip { .. } => "held_slip",
        PrintDocument::Labels { .. } => "labels",
        PrintDocument::Test => "test",
    }
}

// ------------------------------------------------------------------ shared printers

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharedPrinter {
    pub id: String,
    pub name: String,
    pub kind: PrinterKind,
    pub paper: PaperWidth,
}

/// The printers this PC (the hub) lets counters print to.
pub fn shared_printers(state: &AppState) -> Vec<SharedPrinter> {
    PrinterConfig::load(&data_dir(state))
        .printers
        .into_iter()
        .filter(|p| p.shared)
        .map(|p| SharedPrinter {
            id: p.id,
            name: p.name,
            kind: p.kind,
            paper: p.paper,
        })
        .collect()
}

// ------------------------------------------------------------------ queue

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitPrintJobInput {
    pub idempotency_key: String,
    /// One of the hub's shared printers.
    pub printer_id: String,
    pub document: PrintDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    pub id: String,
    pub printer_id: String,
    pub printer_name: String,
    pub title: String,
    pub doc_type: String,
    /// "queued", "printed", "failed" or "cancelled".
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub error_kind: Option<String>,
    pub next_try_at: Option<String>,
    pub printed_at: Option<String>,
    pub device_name: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
    pub created_by_name: Option<String>,
}

const JOB_COLUMNS: &str =
    "j.id, j.printer_id, j.printer_name, j.title, j.doc_type, j.status, j.attempts, j.last_error,
    j.error_kind, j.next_try_at, j.printed_at, d.name, j.created_at, j.created_by, u.name";
const JOB_FROM: &str = "FROM print_jobs j LEFT JOIN devices d ON d.id = j.device_id LEFT JOIN users u ON u.id = j.created_by";

fn job_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PrintJob> {
    Ok(PrintJob {
        id: r.get(0)?,
        printer_id: r.get(1)?,
        printer_name: r.get(2)?,
        title: r.get(3)?,
        doc_type: r.get(4)?,
        status: r.get(5)?,
        attempts: r.get(6)?,
        last_error: r.get(7)?,
        error_kind: r.get(8)?,
        next_try_at: r.get(9)?,
        printed_at: r.get(10)?,
        device_name: r.get(11)?,
        created_at: r.get(12)?,
        created_by: r.get(13)?,
        created_by_name: r.get(14)?,
    })
}

pub fn get_job(conn: &Connection, id: &str) -> AppResult<PrintJob> {
    conn.query_row(
        &format!("SELECT {JOB_COLUMNS} {JOB_FROM} WHERE j.id = ?1 AND j.deleted_at IS NULL"),
        [id],
        job_row,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That print job"))
}

/// Retries after a failure: 1, 2 and 5 minutes, then a person has to press Retry.
const BACKOFF_MINUTES: [i64; 3] = [1, 2, 5];

/// Sends a job to its printer now and records the result.
pub fn attempt(state: &AppState, id: &str) -> AppResult<PrintJob> {
    let (printer_id, data, attempts): (String, Vec<u8>, i64) = state.conn()?.query_row(
        "SELECT printer_id, data, attempts FROM print_jobs WHERE id = ?1 AND status IN ('queued', 'failed') AND deleted_at IS NULL",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::conflict("That print job was already printed or cancelled."))?;
    let config = PrinterConfig::load(&data_dir(state));
    let result = match config.printer(&printer_id) {
        Some(p) if p.shared => transport::send(&p.name, &p.connection, &data),
        _ => Err(transport::printer_error(
            transport::FailureKind::NotConfigured,
            &printer_id,
            "the hub no longer shares this printer",
        )),
    };
    let now = state.now();
    let attempts = attempts + 1;
    let conn = state.conn()?;
    match &result {
        Ok(()) => {
            conn.execute(
                "UPDATE print_jobs SET status = 'printed', attempts = ?1, printed_at = ?2, last_error = NULL,
                   error_kind = NULL, next_try_at = NULL, updated_at = ?2 WHERE id = ?3",
                params![attempts, now.to_rfc3339(), id],
            )?;
        }
        Err(e) => {
            let kind = e
                .details
                .as_ref()
                .and_then(|d| d.get("kind"))
                .and_then(Value::as_str)
                .map(String::from);
            let next = usize::try_from(attempts - 1)
                .ok()
                .and_then(|i| BACKOFF_MINUTES.get(i))
                .map(|m| (now + Duration::minutes(*m)).to_rfc3339());
            conn.execute(
                "UPDATE print_jobs SET status = 'failed', attempts = ?1, last_error = ?2, error_kind = ?3,
                   next_try_at = ?4, updated_at = ?5 WHERE id = ?6",
                params![attempts, e.message, kind, next, now.to_rfc3339(), id],
            )?;
        }
    }
    get_job(&conn, id)
}

/// A document for one of the hub's shared printers. The job is saved first, then printed; if the
/// printer fails the job stays in the queue (retried automatically, and by hand).
pub fn submit(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &SubmitPrintJobInput,
) -> AppResult<PrintJob> {
    require_any(&ctx.actor.permissions, print_permissions(&input.document))?;
    let config = PrinterConfig::load(&data_dir(state));
    let printer = config
        .printer(&input.printer_id)
        .filter(|p| p.shared)
        .ok_or_else(|| {
            transport::printer_error(
                transport::FailureKind::NotConfigured,
                &input.printer_id,
                "not a shared printer of the hub",
            )
        })?;
    let public = settings::public(ctx.conn)?;
    // A counter's sale never opens the hub's drawer.
    let data = printing::render(
        &input.document,
        printer,
        &public,
        ctx.now,
        printing::font(&config),
        false,
    )?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    let title = input.document.title();
    let kind = doc_type(&input.document);
    let printer_name = printer.name.clone();
    let id = write(ctx.conn, |tx| {
        idempotency::once(
            tx,
            &input.idempotency_key,
            "print_jobs_submit",
            actor,
            input,
            &now,
            || {
                let id = new_id();
                tx.execute(
                "INSERT INTO print_jobs (id, printer_id, printer_name, title, doc_type, data, status, device_id,
                   created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7, ?8, ?8, ?9, ?9)",
                params![id, input.printer_id, printer_name, title, kind, data, actor.device_id, now, actor.user_id],
            )?;
                Ok(id)
            },
        )
    })?;
    let job = get_job(ctx.conn, &id)?;
    // A replayed request only prints if the first one never got to the printer.
    if job.status == "queued" && job.attempts == 0 {
        return attempt(state, &id);
    }
    Ok(job)
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    /// "queued", "printed", "failed" or "cancelled"; empty for all.
    #[serde(default)]
    pub status: Option<String>,
}

/// The queue: everything for `printing.manage`, otherwise the user's own jobs.
pub fn list_jobs(ctx: &Ctx<'_>, query: &PrintJobQuery) -> AppResult<Page<PrintJob>> {
    let mine_only = !ctx.actor.can("printing.manage");
    let status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(s) = status {
        if !["queued", "printed", "failed", "cancelled"].contains(&s) {
            return Err(AppError::field("status", "Unknown status."));
        }
    }
    let keys = [
        ("created", "j.created_at"),
        ("status", "j.status"),
        ("printer", "lower(j.printer_name)"),
        ("title", "lower(j.title)"),
    ];
    let order = query
        .page
        .order_by(&keys, "j.created_at DESC, j.id DESC", "j.id")?;
    let filter =
        "j.deleted_at IS NULL AND (?1 IS NULL OR j.status = ?1) AND (?2 = 0 OR j.created_by = ?3)";
    let args = params![status, mine_only, ctx.actor.user_id];
    let total: i64 = ctx.conn.query_row(
        &format!("SELECT count(*) {JOB_FROM} WHERE {filter}"),
        args,
        |r| r.get(0),
    )?;
    let (limit, offset) = query.page.limit_offset();
    let mut stmt = ctx.conn.prepare(&format!(
        "SELECT {JOB_COLUMNS} {JOB_FROM} WHERE {filter} ORDER BY {order} LIMIT ?4 OFFSET ?5"
    ))?;
    let items = stmt
        .query_map(
            params![status, mine_only, ctx.actor.user_id, limit, offset],
            job_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page::new(items, total, &query.page))
}

fn own_or_manager(ctx: &Ctx<'_>, job: &PrintJob) -> AppResult<()> {
    if ctx.actor.can("printing.manage")
        || job.created_by.as_deref() == Some(ctx.actor.user_id.as_str())
    {
        Ok(())
    } else {
        Err(AppError::forbidden("printing.manage"))
    }
}

/// Sends a failed (or waiting) job again.
pub fn retry_job(state: &AppState, ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<PrintJob> {
    let job = get_job(ctx.conn, &input.id)?;
    own_or_manager(ctx, &job)?;
    if !matches!(job.status.as_str(), "queued" | "failed") {
        return Err(AppError::conflict(
            "Only waiting or failed jobs can be sent again.",
        ));
    }
    let now = ctx.now_str();
    AuditEntry::new("PRINT_JOB_RETRIED", "print_jobs", Some(&job.id))
        .by(ctx.actor)
        .summary(job.title.clone())
        .write(ctx.conn, &now)?;
    attempt(state, &job.id)
}

pub fn cancel_job(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<PrintJob> {
    let job = get_job(ctx.conn, &input.id)?;
    own_or_manager(ctx, &job)?;
    if !matches!(job.status.as_str(), "queued" | "failed") {
        return Err(AppError::conflict(
            "Only waiting or failed jobs can be cancelled.",
        ));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "UPDATE print_jobs SET status = 'cancelled', next_try_at = NULL, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, job.id],
        )?;
        AuditEntry::new("PRINT_JOB_CANCELLED", "print_jobs", Some(&job.id))
            .by(actor)
            .summary(job.title.clone())
            .write(tx, &now)
    })?;
    get_job(ctx.conn, &input.id)
}

/// Background job (every minute on the hub): failed jobs whose next try is due.
pub fn retry_due(state: &AppState) -> AppResult<String> {
    let now = state.now().to_rfc3339();
    let due: Vec<String> = {
        let conn = state.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM print_jobs WHERE status = 'failed' AND next_try_at IS NOT NULL AND next_try_at <= ?1
               AND deleted_at IS NULL ORDER BY created_at LIMIT 20",
        )?;
        let ids = stmt
            .query_map([&now], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        ids
    };
    let mut printed = 0;
    for id in &due {
        if attempt(state, id).is_ok_and(|j| j.status == "printed") {
            printed += 1;
        }
    }
    Ok(format!(
        "{} print jobs retried, {printed} printed",
        due.len()
    ))
}

/// Daily clean-up: printed and cancelled jobs older than 7 days.
pub fn purge_old_jobs(conn: &Connection, now: chrono::DateTime<chrono::Utc>) -> AppResult<usize> {
    let cutoff = (now - Duration::days(7)).to_rfc3339();
    Ok(conn.execute(
        "DELETE FROM print_jobs WHERE status IN ('printed', 'cancelled') AND updated_at < ?1",
        [cutoff],
    )?)
}

// ------------------------------------------------------------------ audit of per-PC changes

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrinterChangeInput {
    /// The new configuration, summarised (names, kinds, connections, routes).
    pub summary: Value,
}

/// Printer settings live on each PC; this records a change in the shared audit log.
pub fn record_printer_change(ctx: &mut Ctx<'_>, input: &PrinterChangeInput) -> AppResult<()> {
    ctx.require("printing.manage")?;
    let now = ctx.now_str();
    AuditEntry::new(
        "PRINTERS_UPDATED",
        "devices",
        ctx.actor.device_id.as_deref(),
    )
    .by(ctx.actor)
    .summary("Printer settings changed")
    .after(input.summary.clone())
    .write(ctx.conn, &now)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrawerInput {
    /// Why the drawer is opened without a sale ("change for a customer", …).
    #[serde(default)]
    pub reason: Option<String>,
}

/// Opening the drawer without a cash sale: `sales.open_drawer`, or a manager's approval. Audited.
pub fn authorize_drawer(ctx: &mut Ctx<'_>, input: &DrawerInput) -> AppResult<()> {
    let approved_by = ctx.approve("sales.open_drawer", "open the cash drawer")?;
    let reason = super::common::optional("reason", input.reason.as_deref(), 200)?;
    let now = ctx.now_str();
    AuditEntry::new("DRAWER_OPENED", "devices", ctx.actor.device_id.as_deref())
        .by(ctx.actor)
        .approved_by(approved_by.as_deref())
        .summary(reason.unwrap_or_else(|| "No sale".into()))
        .write(ctx.conn, &now)
}

// ------------------------------------------------------------------ label templates

fn symbology_code(s: Symbology) -> &'static str {
    match s {
        Symbology::Ean13 => "ean13",
        Symbology::Ean8 => "ean8",
        Symbology::Code128 => "code128",
        Symbology::Qr => "qr",
    }
}

fn symbology_of(code: &str) -> Symbology {
    match code {
        "ean13" => Symbology::Ean13,
        "ean8" => Symbology::Ean8,
        "qr" => Symbology::Qr,
        _ => Symbology::Code128,
    }
}

fn template_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<LabelTemplate> {
    let symbology: String = r.get(6)?;
    Ok(LabelTemplate {
        id: r.get(0)?,
        name: r.get(1)?,
        width_mm: r.get(2)?,
        height_mm: r.get(3)?,
        gap_mm: r.get(4)?,
        columns: r.get(5)?,
        symbology: symbology_of(&symbology),
        show_business: r.get(7)?,
        show_name: r.get(8)?,
        show_price: r.get(9)?,
        show_mrp: r.get(10)?,
        show_sku: r.get(11)?,
        show_barcode: r.get(12)?,
        show_batch: r.get(13)?,
        show_expiry: r.get(14)?,
        updated_at: r.get(15)?,
    })
}

const TEMPLATE_COLUMNS: &str =
    "id, name, width_mm, height_mm, gap_mm, columns, symbology, show_business, show_name,
    show_price, show_mrp, show_sku, show_barcode, show_batch, show_expiry, updated_at";

fn can_use_labels(ctx: &Ctx<'_>) -> AppResult<()> {
    require_any(&ctx.actor.permissions, &["labels.print", "products.manage"])
}

pub fn list_templates(ctx: &Ctx<'_>) -> AppResult<Vec<LabelTemplate>> {
    can_use_labels(ctx)?;
    let mut stmt = ctx.conn.prepare(&format!(
        "SELECT {TEMPLATE_COLUMNS} FROM label_templates WHERE deleted_at IS NULL ORDER BY lower(name)"
    ))?;
    let rows = stmt
        .query_map([], template_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn get_template(conn: &Connection, id: &str) -> AppResult<LabelTemplate> {
    conn.query_row(
        &format!(
            "SELECT {TEMPLATE_COLUMNS} FROM label_templates WHERE id = ?1 AND deleted_at IS NULL"
        ),
        [id],
        template_row,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That label template"))
}

pub fn save_template(ctx: &mut Ctx<'_>, input: &LabelTemplate) -> AppResult<LabelTemplate> {
    ctx.require("products.manage")?;
    input.validate()?;
    let name = input.name.trim().to_string();
    let now = ctx.now_str();
    let actor = ctx.actor;
    let id = input.id.clone().unwrap_or_else(new_id);
    write(ctx.conn, |tx| {
        let current: Option<String> = tx
            .query_row(
                "SELECT updated_at FROM label_templates WHERE id = ?1 AND deleted_at IS NULL",
                [&id],
                |r| r.get(0),
            )
            .optional()?;
        let t = input;
        let values = params![
            id,
            name,
            t.width_mm,
            t.height_mm,
            t.gap_mm,
            t.columns,
            symbology_code(t.symbology),
            t.show_business,
            t.show_name,
            t.show_price,
            t.show_mrp,
            t.show_sku,
            t.show_barcode,
            t.show_batch,
            t.show_expiry,
            now,
            actor.user_id
        ];
        match current {
            Some(updated_at) => {
                if t.updated_at.as_ref().is_some_and(|e| *e != updated_at) {
                    return Err(AppError::conflict(
                        "Someone else changed this template. Reload it and try again.",
                    ));
                }
                tx.execute(
                    "UPDATE label_templates SET name = ?2, width_mm = ?3, height_mm = ?4, gap_mm = ?5, columns = ?6,
                       symbology = ?7, show_business = ?8, show_name = ?9, show_price = ?10, show_mrp = ?11,
                       show_sku = ?12, show_barcode = ?13, show_batch = ?14, show_expiry = ?15, updated_at = ?16,
                       updated_by = ?17 WHERE id = ?1",
                    values,
                )?;
            }
            None if input.id.is_some() => return Err(AppError::not_found("That label template")),
            None => {
                tx.execute(
                    "INSERT INTO label_templates (id, name, width_mm, height_mm, gap_mm, columns, symbology, show_business,
                       show_name, show_price, show_mrp, show_sku, show_barcode, show_batch, show_expiry, updated_at,
                       updated_by, created_at, created_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?16, ?17)",
                    values,
                )?;
            }
        }
        AuditEntry::new("LABEL_TEMPLATE_SAVED", "label_templates", Some(&id))
            .by(actor)
            .summary(name.clone())
            .write(tx, &now)
    })?;
    get_template(ctx.conn, &id)
}

pub fn delete_template(ctx: &mut Ctx<'_>, input: &IdInput) -> AppResult<()> {
    ctx.require("products.manage")?;
    let template = get_template(ctx.conn, &input.id)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "UPDATE label_templates SET deleted_at = ?1, updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("LABEL_TEMPLATE_DELETED", "label_templates", Some(&input.id))
            .by(actor)
            .summary(template.name)
            .write(tx, &now)
    })
}

// ------------------------------------------------------------------ items to label

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelSourceInput {
    /// Products (variants) to label, one label each to start with.
    #[serde(default)]
    pub variant_ids: Vec<String>,
    /// A goods receipt: one label per unit received (whole units, rounded up).
    #[serde(default)]
    pub receipt_id: Option<String>,
}

const ITEM_SELECT: &str = "SELECT v.id, p.name || CASE WHEN v.variant_name IS NULL OR v.variant_name = '' THEN '' ELSE ' ' || v.variant_name END,
    v.sku, v.barcode, v.sale_price, v.mrp";

pub fn label_items(ctx: &Ctx<'_>, input: &LabelSourceInput) -> AppResult<Vec<LabelItem>> {
    can_use_labels(ctx)?;
    if input.variant_ids.len() > 500 {
        return Err(AppError::validation("At most 500 products at a time."));
    }
    let mut items = Vec::new();
    let mut stmt = ctx.conn.prepare(&format!(
        "{ITEM_SELECT} FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = ?1 AND v.deleted_at IS NULL"
    ))?;
    for id in &input.variant_ids {
        let item = stmt
            .query_row([id], |r| {
                Ok(LabelItem {
                    variant_id: r.get(0)?,
                    name: r.get(1)?,
                    sku: r.get(2)?,
                    barcode: r.get(3)?,
                    price: r.get(4)?,
                    mrp: r.get(5)?,
                    batch_no: None,
                    expiry_date: None,
                    copies: 1,
                })
            })
            .optional()?
            .ok_or_else(|| AppError::not_found("A product"))?;
        items.push(item);
    }
    if let Some(receipt) = &input.receipt_id {
        let mut stmt = ctx.conn.prepare(&format!(
            "{ITEM_SELECT}, i.base_qty, b.batch_no, b.expiry_date
             FROM goods_receipt_items i JOIN product_variants v ON v.id = i.variant_id JOIN products p ON p.id = v.product_id
             LEFT JOIN batches b ON b.id = i.batch_id WHERE i.receipt_id = ?1 ORDER BY i.line_no"
        ))?;
        let rows = stmt
            .query_map([receipt], |r| {
                let base_qty: i64 = r.get(6)?;
                Ok(LabelItem {
                    variant_id: r.get(0)?,
                    name: r.get(1)?,
                    sku: r.get(2)?,
                    barcode: r.get(3)?,
                    price: r.get(4)?,
                    mrp: r.get(5)?,
                    batch_no: r.get(7)?,
                    expiry_date: r.get(8)?,
                    copies: (base_qty + 999) / 1000,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            return Err(AppError::not_found("That goods receipt"));
        }
        items.extend(rows);
    }
    Ok(items)
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelPreviewInput {
    pub template: LabelTemplate,
    pub items: Vec<LabelItem>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelPreview {
    /// One label as SVG (millimetre size).
    pub svg: String,
    /// The barcode actually used (EAN falls back to Code 128 for codes that aren't EANs).
    pub symbology: Option<Symbology>,
    pub warning: Option<String>,
}

/// One preview per item (copies are not repeated), drawn exactly as the label printer will.
pub fn preview(ctx: &Ctx<'_>, input: &LabelPreviewInput) -> AppResult<Vec<LabelPreview>> {
    can_use_labels(ctx)?;
    input.template.validate()?;
    if input.items.len() > 500 {
        return Err(AppError::validation("At most 500 products at a time."));
    }
    let public = settings::public(ctx.conn)?;
    Ok(input
        .items
        .iter()
        .map(|item| {
            let (svg, used) = labels::svg(&input.template, item, &public);
            let warning = if !input.template.show_barcode {
                None
            } else if used.is_none() {
                Some("No barcode or SKU to print.".to_string())
            } else if used != Some(input.template.symbology) {
                Some("Not a valid EAN: printed as Code 128.".to_string())
            } else {
                None
            };
            LabelPreview {
                svg,
                symbology: used,
                warning,
            }
        })
        .collect())
}

// ------------------------------------------------------------------ internal barcodes

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateBarcodesInput {
    pub variant_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedBarcode {
    pub variant_id: String,
    pub barcode: String,
}

/// In-store EAN-13 numbers start with this (GS1 keeps 20–29 for use inside one shop; 29 avoids
/// the 20–28 ranges scales use for weight barcodes).
pub const INTERNAL_PREFIX: &str = "29";

/// The next free in-store EAN-13, checked against product and pack barcodes.
pub fn next_internal_barcode(conn: &Connection) -> AppResult<String> {
    let max: Option<i64> = conn.query_row(
        "SELECT max(CAST(substr(code, 3, 10) AS INTEGER)) FROM (
           SELECT barcode AS code FROM product_variants WHERE barcode IS NOT NULL
           UNION ALL SELECT barcode FROM unit_conversions WHERE barcode IS NOT NULL)
         WHERE length(code) = 13 AND code GLOB '29[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]'",
        [],
        |r| r.get(0),
    )?;
    let next = max.unwrap_or(0) + 1;
    if next > 9_999_999_999 {
        return Err(AppError::conflict("No in-store barcode numbers are left."));
    }
    let body = format!("{INTERNAL_PREFIX}{next:010}");
    let check = barcode::ean_check_digit(&body).ok_or_else(|| AppError::internal("check digit"))?;
    Ok(format!("{body}{check}"))
}

/// Whether a barcode is taken by any product or pack size (deleted ones excluded), other than the
/// given product's own rows.
pub fn barcode_taken(
    conn: &Connection,
    code: &str,
    except_product: Option<&str>,
) -> AppResult<bool> {
    let except = except_product.unwrap_or("");
    let count: i64 = conn.query_row(
        "SELECT (SELECT count(*) FROM product_variants WHERE barcode = ?1 AND deleted_at IS NULL AND product_id <> ?2)
              + (SELECT count(*) FROM unit_conversions WHERE barcode = ?1 AND deleted_at IS NULL AND coalesce(product_id, '') <> ?2)",
        params![code, except],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Gives each product (variant) without a barcode an in-store EAN-13. Ones that already have a
/// barcode are left alone.
pub fn generate_barcodes(
    ctx: &mut Ctx<'_>,
    input: &GenerateBarcodesInput,
) -> AppResult<Vec<GeneratedBarcode>> {
    ctx.require("products.manage")?;
    if input.variant_ids.is_empty() || input.variant_ids.len() > 500 {
        return Err(AppError::validation("Choose 1 to 500 products."));
    }
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        let mut made = Vec::new();
        for id in &input.variant_ids {
            let row: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT product_id, barcode FROM product_variants WHERE id = ?1 AND deleted_at IS NULL",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (product_id, existing) = row.ok_or_else(|| AppError::not_found("A product"))?;
            if existing.is_some_and(|b| !b.is_empty()) {
                continue;
            }
            let code = next_internal_barcode(tx)?;
            if barcode_taken(tx, &code, None)? {
                return Err(AppError::conflict(
                    "That barcode is already in use. Try again.",
                ));
            }
            tx.execute(
                "UPDATE product_variants SET barcode = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
                params![code, now, actor.user_id, id],
            )?;
            // An editor open on this product must reload before saving over the new barcode.
            tx.execute(
                "UPDATE products SET updated_at = ?1, updated_by = ?2 WHERE id = ?3",
                params![now, actor.user_id, product_id],
            )?;
            AuditEntry::new("BARCODE_GENERATED", "product_variants", Some(id))
                .by(actor)
                .after(json!({ "barcode": code }))
                .write(tx, &now)?;
            made.push(GeneratedBarcode {
                variant_id: id.clone(),
                barcode: code,
            });
        }
        Ok(made)
    })
}

/// Map a printer failure from another PC (it arrives as an AppError) to its kind.
pub fn failure_kind(error: &AppError) -> Option<String> {
    if error.code != ErrorCode::PrinterError {
        return None;
    }
    error
        .details
        .as_ref()
        .and_then(|d| d.get("kind"))
        .and_then(Value::as_str)
        .map(String::from)
}
