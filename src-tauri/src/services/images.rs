//! Product pictures. The picture of a product is two small JPEG files in an `images` folder next to
//! the database (a 800 px one and a 160 px thumbnail for lists); `products.image_path` holds their
//! shared name, `<product id>-<8 hex of the picture's hash>`.
//!
//! * A picture is validated (real PNG, JPEG, WebP or GIF; at most 5 MB; at most 8000 px), decoded,
//!   scaled down, flattened onto white and re-encoded, so what is stored is never the uploaded
//!   bytes.
//! * Files are served through the same command API as everything else, so a counter gets them from
//!   the hub; nothing needs a shared folder.
//! * The files are written first, then the database row is changed, then the old files are
//!   removed: a crash in between leaves a stray file (swept later), never a broken product.
//! * Backups hold the database, not this folder (docs/ASSUMPTIONS.md A-90).

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Limits, Rgb, RgbImage};
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::audit::AuditEntry;
use super::{write, Ctx};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

/// The largest upload accepted (decoded bytes).
pub const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;
const MAX_EDGE_PIXELS: u32 = 8_000;
const FULL_EDGE: u32 = 800;
const THUMB_EDGE: u32 = 160;
const JPEG_QUALITY: u8 = 85;
/// Stray files younger than this are left alone by the sweep (an upload may be in flight).
const SWEEP_GRACE: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageSetInput {
    pub product_id: String,
    /// The picture file, base64 encoded (no `data:` prefix).
    pub data: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageGetInput {
    pub product_id: String,
    /// "thumb" (default) or "full".
    pub size: Option<String>,
    /// The product's `imageKey`: only a cache key for the screen, the server ignores it.
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageRemoveInput {
    pub product_id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageData {
    pub content_type: String,
    /// The JPEG file, base64 encoded.
    pub data: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageSaved {
    pub product_id: String,
    /// The new `imageKey` (null after a removal).
    pub image_key: Option<String>,
}

/// Where the picture files live.
pub fn images_dir(state: &AppState) -> PathBuf {
    state
        .db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
        .join("images")
}

fn full_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.jpg"))
}

fn thumb_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.thumb.jpg"))
}

/// A key is a product id plus a hash: letters, digits and dashes only, so it can never leave the
/// images folder.
fn safe_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 80 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn remove_files(dir: &Path, key: &str) {
    if !safe_key(key) {
        return;
    }
    for path in [full_path(dir, key), thumb_path(dir, key)] {
        let _ = fs::remove_file(path);
    }
}

fn current_key(conn: &Connection, product_id: &str) -> AppResult<Option<String>> {
    let row: Option<Option<String>> = conn
        .query_row(
            "SELECT image_path FROM products WHERE id = ?1 AND deleted_at IS NULL",
            [product_id],
            |r| r.get(0),
        )
        .optional()?;
    match row {
        None => Err(AppError::not_found("That product")),
        Some(key) => Ok(key.filter(|k| !k.is_empty())),
    }
}

fn encode_jpeg(image: &DynamicImage, edge: u32) -> AppResult<Vec<u8>> {
    let scaled = if image.width() > edge || image.height() > edge {
        image.resize(edge, edge, FilterType::Lanczos3)
    } else {
        image.clone()
    };
    // Transparent pictures are put on white, so a PNG cut-out does not turn black in a JPEG.
    let rgba = scaled.to_rgba8();
    let mut flat = RgbImage::from_pixel(rgba.width(), rgba.height(), Rgb([255, 255, 255]));
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        let blend = |channel: u8| -> u8 {
            u8::try_from((u32::from(channel) * alpha + 255 * (255 - alpha)) / 255).unwrap_or(255)
        };
        flat.put_pixel(
            x,
            y,
            Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]),
        );
    }
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&flat)
        .map_err(AppError::internal)?;
    Ok(out)
}

/// Checks and decodes an uploaded picture.
fn decode_upload(data: &str) -> AppResult<DynamicImage> {
    let text = data.trim();
    // Tolerate a data URL, though the screen sends the bare base64.
    let text = text.split_once(',').map_or(text, |(_, rest)| rest);
    if text.len() > MAX_UPLOAD_BYTES * 4 / 3 + 8 {
        return Err(AppError::field(
            "data",
            "The picture is too big (5 MB at most).",
        ));
    }
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| AppError::field("data", "That is not a picture file."))?;
    if bytes.is_empty() {
        return Err(AppError::field("data", "Choose a picture file."));
    }
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::field(
            "data",
            "The picture is too big (5 MB at most).",
        ));
    }
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| AppError::field("data", "That is not a picture file."))?;
    match reader.format() {
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif) => {}
        _ => {
            return Err(AppError::field(
                "data",
                "Use a PNG, JPEG, WebP or GIF picture.",
            ))
        }
    }
    let mut reader = reader;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_EDGE_PIXELS);
    limits.max_image_height = Some(MAX_EDGE_PIXELS);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| {
        AppError::field(
            "data",
            "That picture can't be read, or is too large (8000 px at most).",
        )
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(AppError::internal)?;
    fs::rename(&tmp, path).map_err(AppError::internal)
}

/// Sets or replaces a product's picture.
pub fn set(state: &AppState, ctx: &mut Ctx<'_>, input: &ImageSetInput) -> AppResult<ImageSaved> {
    ctx.require("products.manage")?;
    let old = current_key(ctx.conn, &input.product_id)?;
    let image = decode_upload(&input.data)?;
    let full = encode_jpeg(&image, FULL_EDGE)?;
    let thumb = encode_jpeg(&image, THUMB_EDGE)?;
    let hash = hex::encode(Sha256::digest(&full));
    let key = format!("{}-{}", input.product_id, &hash[..8]);
    if !safe_key(&key) {
        return Err(AppError::validation("That product can't have a picture."));
    }
    let dir = images_dir(state);
    fs::create_dir_all(&dir).map_err(AppError::internal)?;
    write_atomic(&full_path(&dir, &key), &full)?;
    write_atomic(&thumb_path(&dir, &key), &thumb)?;
    if old.as_deref() != Some(key.as_str()) {
        let now = ctx.now_str();
        let actor = ctx.actor;
        write(ctx.conn, |tx| {
            // `updated_at` is left alone: a new picture must not make an open product form
            // think someone else changed the product.
            tx.execute(
                "UPDATE products SET image_path = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![key, input.product_id],
            )?;
            AuditEntry::new("PRODUCT_IMAGE_SET", "products", Some(&input.product_id))
                .by(actor)
                .write(tx, &now)
        })?;
        if let Some(old) = old {
            remove_files(&dir, &old);
        }
    }
    Ok(ImageSaved {
        product_id: input.product_id.clone(),
        image_key: Some(key),
    })
}

/// Removes a product's picture (the files too).
pub fn remove(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &ImageRemoveInput,
) -> AppResult<ImageSaved> {
    ctx.require("products.manage")?;
    let old = current_key(ctx.conn, &input.product_id)?;
    if let Some(old) = old {
        let now = ctx.now_str();
        let actor = ctx.actor;
        write(ctx.conn, |tx| {
            tx.execute(
                "UPDATE products SET image_path = NULL WHERE id = ?1",
                [&input.product_id],
            )?;
            AuditEntry::new("PRODUCT_IMAGE_REMOVED", "products", Some(&input.product_id))
                .by(actor)
                .write(tx, &now)
        })?;
        remove_files(&images_dir(state), &old);
    }
    Ok(ImageSaved {
        product_id: input.product_id.clone(),
        image_key: None,
    })
}

/// The picture of a product (thumbnail by default), or nothing when it has none or the file is
/// missing (for example after restoring a backup on another PC).
pub fn get(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &ImageGetInput,
) -> AppResult<Option<ImageData>> {
    ctx.require("products.view")?;
    let Some(key) = current_key(ctx.conn, &input.product_id)? else {
        return Ok(None);
    };
    if !safe_key(&key) {
        return Ok(None);
    }
    let dir = images_dir(state);
    let path = if input.size.as_deref() == Some("full") {
        full_path(&dir, &key)
    } else {
        thumb_path(&dir, &key)
    };
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(ImageData {
            content_type: "image/jpeg".into(),
            data: STANDARD.encode(bytes),
        })),
        Err(_) => Ok(None),
    }
}

/// Deletes picture files that no live product points at (left by a crash, a deleted product or a
/// replaced picture whose cleanup failed). Returns how many files were removed.
pub fn sweep_orphans(state: &AppState) -> AppResult<usize> {
    sweep_older_than(state, SWEEP_GRACE)
}

/// `sweep_orphans` with the age a stray file must have reached (tests pass zero).
pub fn sweep_older_than(state: &AppState, grace: Duration) -> AppResult<usize> {
    let dir = images_dir(state);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(0);
    };
    let conn = state.conn()?;
    let mut live = std::collections::HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT image_path FROM products WHERE image_path IS NOT NULL AND deleted_at IS NULL",
    )?;
    for key in stmt.query_map([], |r| r.get::<_, String>(0))? {
        live.insert(key?);
    }
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let key = name.split('.').next().unwrap_or_default();
        if live.contains(key) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() > grace)
            .unwrap_or(false);
        if old_enough && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Deleting a product also deletes its picture files.
pub fn delete_product(
    state: &AppState,
    ctx: &mut Ctx<'_>,
    input: &super::users::IdInput,
) -> AppResult<()> {
    ctx.require("products.manage")?;
    let key = current_key(ctx.conn, &input.id).ok().flatten();
    super::catalog::delete_product(ctx, input)?;
    if let Some(key) = key {
        // The row keeps no picture once the product is deleted.
        ctx.conn.execute(
            "UPDATE products SET image_path = NULL WHERE id = ?1",
            [&input.id],
        )?;
        remove_files(&images_dir(state), &key);
    }
    Ok(())
}
