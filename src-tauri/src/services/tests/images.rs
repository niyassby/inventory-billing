//! Product pictures: validation, scaling, replacing, removing, serving and cleaning up.

use std::fs;
use std::io::Cursor;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::{ImageFormat, Rgba, RgbaImage};

use crate::error::ErrorCode;
use crate::services::catalog::{self, ProductListQuery, SearchQuery};
use crate::services::images::{
    self, ImageGetInput, ImageRemoveInput, ImageSetInput, MAX_UPLOAD_BYTES,
};
use crate::services::testkit::{code, Kit};
use crate::services::users::IdInput;

fn png(width: u32, height: u32, color: [u8; 4]) -> String {
    let img = RgbaImage::from_pixel(width, height, Rgba(color));
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    STANDARD.encode(bytes)
}

fn set(
    kit: &Kit,
    product: &str,
    data: String,
) -> Result<images::ImageSaved, crate::error::AppError> {
    let owner = kit.owner();
    let mut conn = kit.conn();
    images::set(
        &kit.state,
        &mut kit.ctx(&mut conn, &owner),
        &ImageSetInput {
            product_id: product.into(),
            data,
        },
    )
}

fn product_of(kit: &Kit, variant: &str) -> String {
    kit.conn()
        .query_row(
            "SELECT product_id FROM product_variants WHERE id = ?1",
            [variant],
            |r| r.get(0),
        )
        .unwrap()
}

fn files(kit: &Kit) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(images::images_dir(&kit.state))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn get(kit: &Kit, product: &str, size: Option<&str>) -> Option<images::ImageData> {
    let owner = kit.owner();
    let mut conn = kit.conn();
    images::get(
        &kit.state,
        &mut kit.ctx(&mut conn, &owner),
        &ImageGetInput {
            product_id: product.into(),
            size: size.map(String::from),
            version: None,
        },
    )
    .unwrap()
}

fn jpeg_size(data: &images::ImageData) -> (u32, u32) {
    let bytes = STANDARD.decode(&data.data).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "a JPEG file");
    let img = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).unwrap();
    (img.width(), img.height())
}

#[test]
fn a_picture_is_scaled_stored_and_served() {
    let kit = Kit::new();
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 5, None);
    let product = product_of(&kit, &variant);

    let saved = set(&kit, &product, png(2_000, 1_000, [200, 30, 30, 255])).unwrap();
    let key = saved.image_key.clone().unwrap();
    assert!(key.starts_with(&product));
    assert_eq!(
        files(&kit),
        [format!("{key}.jpg"), format!("{key}.thumb.jpg")]
    );

    // The full picture is at most 800 px and keeps its shape; the thumbnail at most 160 px.
    assert_eq!(
        jpeg_size(&get(&kit, &product, Some("full")).unwrap()),
        (800, 400)
    );
    assert_eq!(jpeg_size(&get(&kit, &product, None).unwrap()), (160, 80));

    // The key shows up on the product, the list and the till's search.
    let owner = kit.owner();
    let mut conn = kit.conn();
    let detail = catalog::get_product(
        &kit.ctx(&mut conn, &owner),
        &IdInput {
            id: product.clone(),
        },
    )
    .unwrap();
    assert_eq!(detail.image_key.as_deref(), Some(key.as_str()));
    let list =
        catalog::list_products(&kit.ctx(&mut conn, &owner), &ProductListQuery::default()).unwrap();
    assert_eq!(list.items[0].image_key.as_deref(), Some(key.as_str()));
    let found = catalog::search(
        &kit.ctx(&mut conn, &owner),
        &SearchQuery {
            term: "rice".into(),
            limit: None,
            warehouse_id: None,
        },
    )
    .unwrap();
    assert_eq!(found[0].image_key.as_deref(), Some(key.as_str()));
}

#[test]
fn a_transparent_picture_goes_on_white() {
    let kit = Kit::new();
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 5, None);
    let product = product_of(&kit, &variant);
    set(&kit, &product, png(10, 10, [0, 0, 0, 0])).unwrap();
    let data = get(&kit, &product, Some("full")).unwrap();
    let bytes = STANDARD.decode(&data.data).unwrap();
    let img = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let pixel = img.get_pixel(5, 5);
    assert!(
        pixel.0.iter().all(|c| *c > 240),
        "white, not black: {pixel:?}"
    );
}

#[test]
fn replacing_a_picture_removes_the_old_files() {
    let kit = Kit::new();
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 5, None);
    let product = product_of(&kit, &variant);
    let first = set(&kit, &product, png(300, 300, [10, 200, 10, 255])).unwrap();
    let again = set(&kit, &product, png(300, 300, [10, 200, 10, 255])).unwrap();
    assert_eq!(
        first.image_key, again.image_key,
        "the same picture changes nothing"
    );
    assert_eq!(files(&kit).len(), 2);

    let second = set(&kit, &product, png(300, 300, [10, 10, 200, 255])).unwrap();
    assert_ne!(first.image_key, second.image_key);
    let key = second.image_key.unwrap();
    assert_eq!(
        files(&kit),
        [format!("{key}.jpg"), format!("{key}.thumb.jpg")]
    );
}

#[test]
fn removing_and_deleting_clean_up_the_files() {
    let kit = Kit::new();
    let a = kit.product("Rice", "RICE-1", 10_000, 4_000, 0, None);
    let b = kit.product("Oil", "OIL-1", 10_000, 4_000, 0, None);
    let (pa, pb) = (product_of(&kit, &a), product_of(&kit, &b));
    set(&kit, &pa, png(50, 50, [1, 2, 3, 255])).unwrap();
    set(&kit, &pb, png(50, 50, [9, 8, 7, 255])).unwrap();
    assert_eq!(files(&kit).len(), 4);

    let owner = kit.owner();
    let mut conn = kit.conn();
    let removed = images::remove(
        &kit.state,
        &mut kit.ctx(&mut conn, &owner),
        &ImageRemoveInput {
            product_id: pa.clone(),
        },
    )
    .unwrap();
    assert_eq!(removed.image_key, None);
    assert_eq!(files(&kit).len(), 2);
    assert!(get(&kit, &pa, None).is_none());

    // Deleting a product (no stock left) takes its picture with it.
    images::delete_product(
        &kit.state,
        &mut kit.ctx(&mut conn, &owner),
        &IdInput { id: pb.clone() },
    )
    .unwrap();
    assert!(files(&kit).is_empty());
}

#[test]
fn bad_uploads_are_refused_before_anything_is_written() {
    let kit = Kit::new();
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 0, None);
    let product = product_of(&kit, &variant);

    for (name, data) in [
        ("text", STANDARD.encode("not a picture at all")),
        ("empty", String::new()),
        ("not base64", "%%%".to_string()),
        (
            "a bitmap",
            STANDARD.encode(b"BM\x36\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00"),
        ),
        ("too wide", png(9_000, 2, [0, 0, 0, 255])),
        ("too big", STANDARD.encode(vec![0u8; MAX_UPLOAD_BYTES + 1])),
    ] {
        let result = set(&kit, &product, data);
        assert_eq!(code(result), ErrorCode::Validation, "{name}");
    }
    assert!(files(&kit).is_empty());
    assert!(get(&kit, &product, None).is_none());
}

#[test]
fn only_those_who_manage_products_can_change_pictures() {
    let kit = Kit::new();
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 0, None);
    let product = product_of(&kit, &variant);
    set(&kit, &product, png(50, 50, [1, 2, 3, 255])).unwrap();

    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let denied = images::set(
        &kit.state,
        &mut kit.ctx(&mut conn, &cashier),
        &ImageSetInput {
            product_id: product.clone(),
            data: png(10, 10, [0, 0, 0, 255]),
        },
    );
    assert_eq!(code(denied), ErrorCode::Forbidden);
    // A cashier can see it (the till shows pictures).
    let seen = images::get(
        &kit.state,
        &mut kit.ctx(&mut conn, &cashier),
        &ImageGetInput {
            product_id: product,
            size: None,
            version: None,
        },
    )
    .unwrap();
    assert!(seen.is_some());
}

#[test]
fn the_sweep_removes_only_files_no_product_points_at() {
    let kit = Kit::new();
    let variant = kit.product("Rice", "RICE-1", 10_000, 4_000, 0, None);
    let product = product_of(&kit, &variant);
    let kept = set(&kit, &product, png(40, 40, [5, 5, 5, 255]))
        .unwrap()
        .image_key
        .unwrap();
    let dir = images::images_dir(&kit.state);
    fs::write(dir.join("gone-12345678.jpg"), b"x").unwrap();
    fs::write(dir.join("gone-12345678.thumb.jpg"), b"x").unwrap();

    // A fresh stray file is left alone (an upload may be in flight) ...
    assert_eq!(images::sweep_orphans(&kit.state).unwrap(), 0);
    // ... an old one goes, and the picture in use stays.
    assert_eq!(
        images::sweep_older_than(&kit.state, Duration::ZERO).unwrap(),
        2
    );
    assert_eq!(
        files(&kit),
        [format!("{kept}.jpg"), format!("{kept}.thumb.jpg")]
    );
}
