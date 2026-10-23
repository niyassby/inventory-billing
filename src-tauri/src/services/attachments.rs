//! Files attached to records (manual journals today): a PDF or a picture of the supporting
//! document. The file is kept as uploaded in an `attachments` folder next to the database; the
//! table holds its name, type, size and hash. Files travel through the command API, so a counter
//! reaches them through the hub. Backups hold the database, not this folder (like pictures).

use std::fs;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::audit::AuditEntry;
use super::{write, Ctx};
use crate::app::AppState;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};

pub const MAX_BYTES: usize = 5 * 1024 * 1024;
const MAX_PER_RECORD: i64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub id: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub created_by_name: Option<String>,
}

pub fn list_of(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
) -> AppResult<Vec<AttachmentInfo>> {
    Ok(conn
        .prepare(
            "SELECT a.id, a.file_name, a.content_type, a.size_bytes, a.created_at, u.name
             FROM attachments a LEFT JOIN users u ON u.id = a.created_by
             WHERE a.entity_type = ?1 AND a.entity_id = ?2 AND a.deleted_at IS NULL ORDER BY a.created_at",
        )?
        .query_map(params![entity_type, entity_id], |r| {
            Ok(AttachmentInfo {
                id: r.get(0)?,
                file_name: r.get(1)?,
                content_type: r.get(2)?,
                size_bytes: r.get(3)?,
                created_at: r.get(4)?,
                created_by_name: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

pub fn attachments_dir(state: &AppState) -> PathBuf {
    state
        .db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
        .join("attachments")
}

/// The file's real type from its first bytes: (content type, extension).
pub fn sniff(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"%PDF-") {
        Some(("application/pdf", "pdf"))
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

/// A file name safe to show and to offer for saving: no folders, no control characters.
fn clean_name(name: &str, ext: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(120)
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        format!("attachment.{ext}")
    } else {
        cleaned
    }
}

/// Who may read or change the attachments of a record, and whether it can still change.
fn check_entity(
    ctx: &Ctx<'_>,
    entity_type: &str,
    entity_id: &str,
    changing: bool,
) -> AppResult<String> {
    match entity_type {
        "manual_journal" => {
            if changing {
                ctx.require("journal.manage")?;
            } else {
                ctx.require("journal.manage")
                    .or_else(|_| ctx.require("journal.approve"))
                    .or_else(|_| ctx.require("accounting.view"))?;
            }
            let (doc_no, status): (String, String) = ctx
                .conn
                .query_row(
                    "SELECT doc_no, status FROM manual_journals WHERE id = ?1 AND deleted_at IS NULL",
                    [entity_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::not_found("The journal"))?;
            if changing && matches!(status.as_str(), "posted" | "reversed") {
                return Err(AppError::conflict(
                    "A posted journal's attachments can't change.",
                ));
            }
            Ok(doc_no)
        }
        _ => Err(AppError::field("entityType", "Unknown record type.")),
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentAddInput {
    /// "manual_journal".
    pub entity_type: String,
    pub entity_id: String,
    pub file_name: String,
    /// The file, base64 encoded (no `data:` prefix).
    pub data: String,
}

pub fn add(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &AttachmentAddInput,
) -> AppResult<AttachmentInfo> {
    let doc = check_entity(ctx, &input.entity_type, &input.entity_id, true)?;
    if input.data.len() > MAX_BYTES / 3 * 4 + 8 {
        return Err(AppError::field("data", "Files can be at most 5 MB."));
    }
    let bytes = STANDARD
        .decode(input.data.trim())
        .map_err(|_| AppError::field("data", "That file couldn't be read."))?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(AppError::field("data", "Files can be at most 5 MB."));
    }
    let (content_type, ext) = sniff(&bytes)
        .ok_or_else(|| AppError::field("data", "Attach a PDF or a picture (PNG, JPEG or WebP)."))?;
    let count: i64 = ctx.conn.query_row(
        "SELECT count(*) FROM attachments WHERE entity_type = ?1 AND entity_id = ?2 AND deleted_at IS NULL",
        params![input.entity_type, input.entity_id],
        |r| r.get(0),
    )?;
    if count >= MAX_PER_RECORD {
        return Err(AppError::conflict(format!(
            "A record can have at most {MAX_PER_RECORD} attachments."
        )));
    }
    let id = new_id();
    let key = format!("{id}.{ext}");
    let dir = attachments_dir(state);
    fs::create_dir_all(&dir).map_err(AppError::internal)?;
    let path = dir.join(&key);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &bytes).map_err(AppError::internal)?;
    fs::rename(&tmp, &path).map_err(AppError::internal)?;
    let name = clean_name(&input.file_name, ext);
    let hash = hex::encode(Sha256::digest(&bytes));
    let now = ctx.now_str();
    let actor = ctx.actor;
    let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    let saved = write(ctx.conn, |tx| {
        tx.execute(
            "INSERT INTO attachments (id, entity_type, entity_id, file_name, content_type, size_bytes, sha256, storage_key, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, input.entity_type, input.entity_id, name, content_type, size, hash, key, now, actor.user_id],
        )?;
        AuditEntry::new(
            "ATTACHMENT_ADDED",
            &input.entity_type,
            Some(&input.entity_id),
        )
        .by(actor)
        .summary(format!("{doc}: {name}"))
        .write(tx, &now)
    });
    if saved.is_err() {
        let _ = fs::remove_file(&path);
    }
    saved?;
    Ok(AttachmentInfo {
        id,
        file_name: name,
        content_type: content_type.into(),
        size_bytes: size,
        created_at: now,
        created_by_name: Some(actor.name.clone()),
    })
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentFile {
    pub file_name: String,
    pub content_type: String,
    /// The file, base64 encoded.
    pub data: String,
}

fn load(conn: &Connection, id: &str) -> AppResult<(String, String, String, String, String)> {
    conn.query_row(
        "SELECT entity_type, entity_id, file_name, content_type, storage_key FROM attachments WHERE id = ?1 AND deleted_at IS NULL",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("The attachment"))
}

pub fn get(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &AttachmentRef,
) -> AppResult<AttachmentFile> {
    let (entity_type, entity_id, file_name, content_type, key) = load(ctx.conn, &input.id)?;
    check_entity(ctx, &entity_type, &entity_id, false)?;
    if key.contains(['/', '\\']) || key.contains("..") {
        return Err(AppError::not_found("The attachment"));
    }
    let bytes = fs::read(attachments_dir(state).join(&key)).map_err(|_| {
        AppError::not_found("The attachment's file (it may not have been restored with a backup)")
    })?;
    Ok(AttachmentFile {
        file_name,
        content_type,
        data: STANDARD.encode(bytes),
    })
}

/// Removes an attachment of a record that can still change (the row stays, the file goes).
pub fn remove(state: &AppState, ctx: &mut Ctx<'_>, input: &AttachmentRef) -> AppResult<()> {
    let (entity_type, entity_id, file_name, _, key) = load(ctx.conn, &input.id)?;
    let doc = check_entity(ctx, &entity_type, &entity_id, true)?;
    let now = ctx.now_str();
    let actor = ctx.actor;
    write(ctx.conn, |tx| {
        tx.execute(
            "UPDATE attachments SET deleted_at = ?1, deleted_by = ?2 WHERE id = ?3",
            params![now, actor.user_id, input.id],
        )?;
        AuditEntry::new("ATTACHMENT_REMOVED", &entity_type, Some(&entity_id))
            .by(actor)
            .summary(format!("{doc}: {file_name}"))
            .write(tx, &now)
    })?;
    if !key.contains(['/', '\\']) && !key.contains("..") {
        let _ = fs::remove_file(attachments_dir(state).join(key));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{clean_name, sniff};

    #[test]
    fn knows_files_by_their_bytes_and_cleans_names() {
        assert_eq!(sniff(b"%PDF-1.7 ...").map(|t| t.1), Some("pdf"));
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]).map(|t| t.1), Some("jpg"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 ").map(|t| t.1), Some("webp"));
        assert_eq!(sniff(b"MZ\x90\0 not a document"), None);
        assert_eq!(clean_name("C:\\bills\\rent<oct>.pdf", "pdf"), "rentoct.pdf");
        assert_eq!(clean_name("../../..", "pdf"), "attachment.pdf");
    }
}
