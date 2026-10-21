//! LAN hub and client mode, end to end over real HTTPS on 127.0.0.1: pairing, pinned TLS,
//! device-bound sessions, offline sales from two counters, duplicate retries, clock skew.

use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::app::FixedClock;
use crate::client::hub_api::{HubApi, HubError};
use crate::client::{ClientRuntime, Link};
use crate::error::{AppError, ErrorCode};
use crate::hub::{self, HubServer};
use crate::node::HubLink;
use crate::services::devices::{self, PairInput};
use crate::services::sales::InvoiceDetail;
use crate::services::sync::{self, ItemStatus, PushRequest};
use crate::services::testkit::{Kit, OWNER_EMAIL, PASSWORD};
use crate::services::users::IdInput;

async fn start(kit: &Kit, addr: SocketAddr) -> HubServer {
    // A port is free again shortly after a server stops.
    for _ in 0..50 {
        if let Ok(server) = hub::start(kit.state.clone(), addr).await {
            return server;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("hub did not start on {addr}");
}

fn url(server: &HubServer) -> String {
    format!("https://{}", server.addr)
}

fn pairing_code(kit: &Kit) -> String {
    let owner = kit.owner();
    let mut conn = kit.conn();
    devices::create_pairing_code(&mut kit.ctx(&mut conn, &owner))
        .unwrap()
        .code
}

async fn pair(kit: &Kit, server: &HubServer, name: &str) -> (HubLink, String) {
    let api = HubApi::new(&url(server), &server.fingerprint, None).unwrap();
    let paired = api
        .pair(&PairInput {
            code: pairing_code(kit),
            device_name: name.into(),
            app_version: None,
        })
        .await
        .map_err(AppError::from)
        .unwrap();
    let link = HubLink {
        url: url(server),
        fingerprint: server.fingerprint.clone(),
        device_id: paired.device_id,
        short_code: paired.short_code,
        business_name: paired.business_name,
        paired_at: kit.now().to_rfc3339(),
    };
    (link, paired.device_token)
}

struct Counter {
    _dir: tempfile::TempDir,
    runtime: Arc<ClientRuntime>,
    clock: Arc<FixedClock>,
    token: String,
}

/// A paired counter with its own clock and local store, signed in as `email`.
async fn counter(kit: &Kit, server: &HubServer, name: &str, email: &str) -> Counter {
    let (link, device_token) = pair(kit, server, name).await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FixedClock::new(kit.now()));
    let runtime = ClientRuntime::new(dir.path(), link, device_token, clock.clone()).unwrap();
    let session = runtime
        .call(
            "auth_login",
            json!({ "input": { "email": email, "password": PASSWORD } }),
        )
        .await
        .unwrap();
    let token = session["token"].as_str().unwrap().to_string();
    let status = runtime.sync_once().await.unwrap();
    assert_eq!(status.link, Link::Connected);
    assert_eq!(
        status.offline_numbers_left, 50,
        "a full lease after the first sync"
    );
    Counter {
        _dir: dir,
        runtime,
        clock,
        token,
    }
}

/// A cashier who has already set their own password.
fn cashier(kit: &Kit) -> String {
    kit.user("cashier");
    kit.conn()
        .execute(
            "UPDATE users SET must_change_password = 0 WHERE email = 'cashier@shop.in'",
            [],
        )
        .unwrap();
    "cashier@shop.in".into()
}

async fn go_offline(counter: &Counter) {
    for _ in 0..3 {
        counter.runtime.sync_once().await.unwrap();
    }
    assert_eq!(counter.runtime.status().link, Link::HubOffline);
}

fn cash_sale(key: &str, variant: &str, units: i64, total: i64) -> Value {
    json!({
        "idempotencyKey": key,
        "lines": [{ "variantId": variant, "qty": units * 1000 }],
        "payments": [{ "mode": "cash", "amount": total }],
        "expectedTotal": total,
    })
}

async fn sell(counter: &Counter, sale: Value) -> Result<InvoiceDetail, AppError> {
    counter
        .runtime
        .call(
            "sales_create_invoice",
            json!({ "token": counter.token, "input": sale }),
        )
        .await
        .map(|v| serde_json::from_value(v).unwrap())
}

/// Kit with one 18% GST product: ₹100.00 + GST = ₹118.00, `stock` units on hand.
fn shop(stock: i64) -> (Kit, String) {
    let kit = Kit::new();
    let gst = kit.gst_rate(18);
    let rice = kit.product("Basmati Rice", "RICE-1", 10_000, 7_000, stock, Some(&gst));
    (kit, rice)
}

fn localhost() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

#[test]
fn lan_addresses_only() {
    let none: Vec<ipnet::IpNet> = Vec::new();
    for ip in [
        "127.0.0.1",
        "192.168.1.20",
        "10.4.0.7",
        "172.16.3.3",
        "169.254.1.1",
        "::1",
        "fd00::1",
        "fe80::1",
        "::ffff:192.168.1.9",
    ] {
        assert!(hub::is_lan_address(ip.parse().unwrap(), &none), "{ip}");
    }
    for ip in ["8.8.8.8", "100.64.0.1", "2001:db8::1", "::ffff:8.8.8.8"] {
        assert!(!hub::is_lan_address(ip.parse().unwrap(), &none), "{ip}");
    }
    let allowed = vec!["100.64.0.0/10".parse().unwrap()];
    assert!(hub::is_lan_address("100.64.0.1".parse().unwrap(), &allowed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pairing_pinning_and_device_bound_sessions() {
    let kit = Kit::new();
    let server = start(&kit, localhost()).await;

    // First contact shows the hub's fingerprint (to compare with the hub's screen).
    let (health, fingerprint) = HubApi::probe(&url(&server)).await.unwrap();
    assert_eq!(health.business_name, "Ravi Stores");
    assert_eq!(fingerprint, server.fingerprint);

    // Pairing needs a valid, unused code.
    let api = HubApi::new(&url(&server), &server.fingerprint, None).unwrap();
    let bad = api
        .pair(&PairInput {
            code: "000000".into(),
            device_name: "Counter".into(),
            app_version: None,
        })
        .await;
    assert!(matches!(bad, Err(HubError::App(ref e)) if e.code == ErrorCode::Unauthorized));
    let code = pairing_code(&kit);
    let input = PairInput {
        code: code.clone(),
        device_name: "Counter 1".into(),
        app_version: Some("0.1.0".into()),
    };
    let first = api.pair(&input).await.map_err(AppError::from).unwrap();
    assert_eq!(first.short_code, "C1");
    assert!(
        matches!(api.pair(&input).await, Err(HubError::App(ref e)) if e.code == ErrorCode::Unauthorized)
    );

    // A paired device signs in and works.
    let device = HubApi::new(
        &url(&server),
        &server.fingerprint,
        Some(first.device_token.clone()),
    )
    .unwrap();
    let session = device
        .call(
            "auth_login",
            "",
            &json!({ "email": OWNER_EMAIL, "password": PASSWORD }),
            None,
        )
        .await
        .map_err(AppError::from)
        .unwrap();
    let token = session["token"].as_str().unwrap().to_string();
    let me = device
        .call("auth_me", &token, &Value::Null, None)
        .await
        .map_err(AppError::from)
        .unwrap();
    assert_eq!(me["email"], OWNER_EMAIL);

    // The session belongs to that device: not usable from another device or on the hub itself.
    let (second_link, second_token) = pair(&kit, &server, "Counter 2").await;
    assert_eq!(second_link.short_code, "C2");
    let other = HubApi::new(&url(&server), &server.fingerprint, Some(second_token)).unwrap();
    let stolen = other.call("auth_me", &token, &Value::Null, None).await;
    assert!(matches!(stolen, Err(HubError::App(ref e)) if e.code == ErrorCode::Unauthorized));
    let local = crate::services::auth::authenticate(&kit.conn(), &token, kit.now()).unwrap();
    assert_eq!(
        local.actor.device_id.as_deref(),
        Some(first.device_id.as_str())
    );

    // No device credential, or a wrong certificate pin: refused.
    let anonymous = HubApi::new(&url(&server), &server.fingerprint, None).unwrap();
    let refused = anonymous.call("auth_me", &token, &Value::Null, None).await;
    assert!(matches!(refused, Err(HubError::App(ref e)) if e.code == ErrorCode::Unauthorized));
    let impostor = HubApi::new(
        &url(&server),
        &"0".repeat(64),
        Some(first.device_token.clone()),
    )
    .unwrap();
    assert!(matches!(
        impostor.health().await,
        Err(HubError::Unreachable(_))
    ));

    // Hub-only commands are refused over the LAN.
    let restore = device
        .call(
            "backup_restore",
            &token,
            &json!({ "path": "C:/x.invbak" }),
            None,
        )
        .await;
    assert!(matches!(restore, Err(HubError::App(ref e)) if e.code == ErrorCode::Forbidden));

    // Revoking the device ends its credential and its sessions.
    let owner = kit.owner();
    let mut conn = kit.conn();
    devices::revoke(
        &mut kit.ctx(&mut conn, &owner),
        &IdInput {
            id: first.device_id.clone(),
        },
    )
    .unwrap();
    drop(conn);
    let after = device.call("auth_me", &token, &Value::Null, None).await;
    assert!(matches!(after, Err(HubError::App(ref e)) if e.code == ErrorCode::Unauthorized));
    server.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_counters_sell_the_last_unit_while_the_hub_is_off() {
    let (kit, rice) = shop(1);
    let cashier = cashier(&kit);
    let server = start(&kit, localhost()).await;
    let addr = server.addr;
    let a = counter(&kit, &server, "Counter 1", OWNER_EMAIL).await;
    let b = counter(&kit, &server, "Counter 2", &cashier).await;

    // Online, the hub's stock rule applies: the counter can't sell 2 of 1.
    let refused = sell(&a, cash_sale("online-000001", &rice, 2, 23_600)).await;
    assert_eq!(refused.unwrap_err().code, ErrorCode::InsufficientStock);

    server.kill();
    go_offline(&a).await;
    go_offline(&b).await;

    // Both counters sell the last unit; each numbers it from its own leased series.
    let bill_a = sell(&a, cash_sale("offline-a-0001", &rice, 1, 11_800))
        .await
        .unwrap();
    let bill_b = sell(&b, cash_sale("offline-b-0001", &rice, 1, 11_800))
        .await
        .unwrap();
    assert_eq!(bill_a.doc_no, "C1/26-27/00001");
    assert_eq!(bill_b.doc_no, "C2/26-27/00001");
    assert_eq!(
        (bill_a.origin.as_str(), bill_a.total, bill_a.tax_total),
        ("offline", 11_800, 1_800)
    );
    assert_eq!(a.runtime.status().pending, 1);

    // Offline, only fully paid sales from the cache are possible.
    let mut credit = cash_sale("offline-a-0002", &rice, 1, 11_800);
    credit["payments"][0]["amount"] = json!(5_000);
    let credit = sell(&a, credit).await;
    assert_eq!(credit.unwrap_err().code, ErrorCode::Validation);
    let other = a
        .runtime
        .call("parties_list", json!({ "token": a.token, "input": {} }))
        .await;
    assert_eq!(other.unwrap_err().code, ErrorCode::HubOffline);

    // The hub comes back: both sales are posted, the oversell is recorded for review.
    let server = start(&kit, addr).await;
    let status_a = a.runtime.sync_once().await.unwrap();
    let status_b = b.runtime.sync_once().await.unwrap();
    assert_eq!((status_a.link, status_a.pending), (Link::Connected, 0));
    assert_eq!((status_b.link, status_b.pending), (Link::Connected, 0));
    let conn = kit.conn();
    let numbers: Vec<(String, String)> = conn
        .prepare("SELECT doc_no, origin FROM sales_invoices ORDER BY doc_no")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        numbers,
        vec![
            ("C1/26-27/00001".to_string(), "offline".to_string()),
            ("C2/26-27/00001".to_string(), "offline".to_string())
        ]
    );
    let hub_a = crate::services::sales::invoice_detail(&conn, &bill_a.id).unwrap();
    assert_eq!(
        (hub_a.total, hub_a.tax_total, hub_a.doc_no.as_str()),
        (bill_a.total, bill_a.tax_total, bill_a.doc_no.as_str())
    );
    assert_eq!(kit.stock(&rice), -1_000);
    let (oversells, short): (i64, i64) = conn
        .query_row(
            "SELECT count(*), coalesce(sum(amount), 0) FROM sync_exceptions WHERE kind = 'offline_oversell'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((oversells, short), (1, 1_000));
    drop(conn);
    kit.assert_books_balance();
    server.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lost_response_never_bills_twice() {
    let (kit, rice) = shop(10);
    let server = start(&kit, localhost()).await;
    let addr = server.addr;
    let a = counter(&kit, &server, "Counter 1", OWNER_EMAIL).await;
    server.kill();
    go_offline(&a).await;
    sell(&a, cash_sale("offline-a-0001", &rice, 1, 11_800))
        .await
        .unwrap();
    sell(&a, cash_sale("offline-a-0002", &rice, 2, 23_600))
        .await
        .unwrap();

    // The hub applies the outbox but the answer never reaches the counter (crash, cable).
    let server = start(&kit, addr).await;
    let device_id: String = kit
        .conn()
        .query_row("SELECT id FROM devices", [], |r| r.get(0))
        .unwrap();
    let device = devices::DeviceRef {
        id: device_id,
        name: "Counter 1".into(),
        short_code: "C1".into(),
        branch_id: kit
            .conn()
            .query_row("SELECT id FROM branches", [], |r| r.get(0))
            .unwrap(),
    };
    let request = PushRequest {
        client_now: kit.now().to_rfc3339(),
        items: a.runtime.pending_items().unwrap(),
    };
    let first = sync::push(&kit.state, &device, &request).unwrap();
    assert!(first
        .results
        .iter()
        .all(|r| r.status == ItemStatus::Applied));

    // The counter sends the same items again: the hub recognises them, nothing is posted twice.
    let status = a.runtime.sync_once().await.unwrap();
    assert_eq!(status.pending, 0);
    let again = sync::push(&kit.state, &device, &request).unwrap();
    assert!(again
        .results
        .iter()
        .all(|r| r.status == ItemStatus::Duplicate));
    let invoices: i64 = kit
        .conn()
        .query_row("SELECT count(*) FROM sales_invoices", [], |r| r.get(0))
        .unwrap();
    assert_eq!(invoices, 2);
    assert_eq!(kit.stock(&rice), 7_000);
    kit.assert_books_balance();
    server.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wrong_counter_clock_is_corrected_and_flagged() {
    let (kit, rice) = shop(10);
    let server = start(&kit, localhost()).await;
    let addr = server.addr;
    let a = counter(&kit, &server, "Counter 1", OWNER_EMAIL).await;
    server.kill();
    go_offline(&a).await;
    // The counter's clock runs 30 hours slow: it thinks the sale is on the 16th.
    a.clock.advance(chrono::Duration::hours(-30));
    let bill = sell(&a, cash_sale("offline-a-0001", &rice, 1, 11_800))
        .await
        .unwrap();
    assert_eq!(bill.business_date, "2026-10-16");

    let server = start(&kit, addr).await;
    a.runtime.sync_once().await.unwrap();
    let conn = kit.conn();
    let (date, client_time): (String, String) = conn
        .query_row(
            "SELECT business_date, client_created_at FROM sales_invoices WHERE id = ?1",
            [&bill.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(date, "2026-10-17", "dated by the hub's clock");
    assert!(client_time.starts_with("2026-10-15T23:30"));
    let skew: i64 = conn
        .query_row(
            "SELECT amount FROM sync_exceptions WHERE kind = 'clock_skew'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(skew, 30 * 3600);
    drop(conn);
    kit.assert_books_balance();
    server.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refused_offline_sales_wait_for_a_manager() {
    let (kit, rice) = shop(10);
    let cashier = cashier(&kit);
    let server = start(&kit, localhost()).await;
    let addr = server.addr;
    let b = counter(&kit, &server, "Counter 1", &cashier).await;
    server.kill();
    go_offline(&b).await;

    // Offline limits: no price change, no discount above the cashier's 5 %.
    let mut changed = cash_sale("offline-b-0001", &rice, 1, 11_800);
    changed["lines"][0]["unitPrice"] = json!(9_000);
    assert_eq!(
        sell(&b, changed).await.unwrap_err().code,
        ErrorCode::HubOffline
    );
    let mut big = cash_sale("offline-b-0002", &rice, 1, 10_620);
    big["billDiscount"] = json!({ "kind": "percent", "rate": 100_000 });
    assert_eq!(sell(&b, big).await.unwrap_err().code, ErrorCode::HubOffline);
    let bill = sell(&b, cash_sale("offline-b-0003", &rice, 1, 11_800))
        .await
        .unwrap();

    // Meanwhile the hub changed the price and disabled the cashier.
    kit.conn()
        .execute(
            "UPDATE product_variants SET sale_price = 12000 WHERE id = ?1",
            [&rice],
        )
        .unwrap();
    kit.conn()
        .execute(
            "UPDATE users SET is_active = 0 WHERE email = ?1",
            [&cashier],
        )
        .unwrap();
    let server = start(&kit, addr).await;
    let status = b.runtime.sync_once().await.unwrap();
    assert_eq!((status.pending, status.rejected), (0, 1));
    let outbox = b.runtime.outbox(false).unwrap();
    assert_eq!(
        outbox[0].last_error.as_ref().unwrap().code,
        ErrorCode::Unauthorized
    );

    // A manager re-enables the cashier and sends it again: posted at the price the customer paid.
    kit.conn()
        .execute(
            "UPDATE users SET is_active = 1 WHERE email = ?1",
            [&cashier],
        )
        .unwrap();
    b.runtime.retry(&outbox[0].id, None).unwrap();
    let status = b.runtime.sync_once().await.unwrap();
    assert_eq!((status.pending, status.rejected), (0, 0));
    let conn = kit.conn();
    let hub = crate::services::sales::invoice_detail(&conn, &bill.id).unwrap();
    assert_eq!((hub.total, hub.lines[0].unit_price), (11_800, 10_000));
    let changed: i64 = conn
        .query_row(
            "SELECT amount FROM sync_exceptions WHERE kind = 'offline_price_changed' AND source_id = ?1",
            [&bill.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(changed, -2_000);
    drop(conn);
    kit.assert_books_balance();
    server.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_sales_can_be_turned_off() {
    let (kit, rice) = shop(10);
    let server = start(&kit, localhost()).await;
    let addr = server.addr;
    let a = counter(&kit, &server, "Counter 1", OWNER_EMAIL).await;
    let owner = kit.owner();
    let mut conn = kit.conn();
    crate::services::settings::update(
        &mut kit.ctx(&mut conn, &owner),
        &crate::services::settings::UpdateSettingsInput {
            section: "network".into(),
            value: json!({ "offlineSalesPolicy": "block" }),
        },
    )
    .unwrap();
    drop(conn);
    a.runtime.refresh_catalogue(true).await.unwrap();
    server.kill();
    go_offline(&a).await;
    let refused = sell(&a, cash_sale("offline-a-0001", &rice, 1, 11_800)).await;
    assert_eq!(refused.unwrap_err().code, ErrorCode::HubOffline);
    // Search keeps working from the cache.
    let found = a
        .runtime
        .call(
            "products_search",
            json!({ "token": a.token, "input": { "term": "basmati" } }),
        )
        .await
        .unwrap();
    assert_eq!(found[0]["sku"], "RICE-1");
    drop(start(&kit, addr).await);
}

#[test]
fn only_managers_pair_devices_and_review_offline_sales() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let manager = kit.user("manager");
    let mut conn = kit.conn();
    let denied = devices::create_pairing_code(&mut kit.ctx(&mut conn, &cashier));
    assert_eq!(crate::services::testkit::code(denied), ErrorCode::Forbidden);
    let review = sync::list_exceptions(&kit.ctx(&mut conn, &cashier), &Default::default());
    assert_eq!(crate::services::testkit::code(review), ErrorCode::Forbidden);
    devices::create_pairing_code(&mut kit.ctx(&mut conn, &manager)).unwrap();
    assert!(devices::list(&kit.ctx(&mut conn, &manager))
        .unwrap()
        .is_empty());
    assert_eq!(
        sync::list_exceptions(&kit.ctx(&mut conn, &manager), &Default::default())
            .unwrap()
            .total,
        0
    );
}

/// A picture sent from a counter lands on the hub and any counter can fetch it from there; only
/// someone who manages products can change it; offline, it is simply not available.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pictures_travel_through_the_hub() {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use image::{ImageFormat, Rgba, RgbaImage};

    let (kit, rice) = shop(3);
    let product: String = kit
        .conn()
        .query_row(
            "SELECT product_id FROM product_variants WHERE id = ?1",
            [&rice],
            |r| r.get(0),
        )
        .unwrap();
    let cashier = cashier(&kit);
    let server = start(&kit, localhost()).await;
    let office = counter(&kit, &server, "Back office", OWNER_EMAIL).await;
    let till = counter(&kit, &server, "Till 2", &cashier).await;

    let mut bytes = Vec::new();
    RgbaImage::from_pixel(1_200, 900, Rgba([30, 120, 200, 255]))
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    let saved = office
        .runtime
        .call(
            "products_image_set",
            json!({ "token": office.token, "input": { "productId": product, "data": STANDARD.encode(&bytes) } }),
        )
        .await
        .unwrap();
    let key = saved["imageKey"].as_str().unwrap().to_string();
    // The file is on the hub, in the hub's images folder.
    assert!(crate::services::images::images_dir(&kit.state)
        .join(format!("{key}.jpg"))
        .is_file());

    // The cashier's till gets it through the hub, but may not change it.
    let fetched = till
        .runtime
        .call(
            "products_image_get",
            json!({ "token": till.token, "input": { "productId": product, "size": "thumb" } }),
        )
        .await
        .unwrap();
    assert_eq!(fetched["contentType"], "image/jpeg");
    assert!(!fetched["data"].as_str().unwrap().is_empty());
    let denied = till
        .runtime
        .call(
            "products_image_remove",
            json!({ "token": till.token, "input": { "productId": product } }),
        )
        .await;
    assert_eq!(denied.unwrap_err().code, ErrorCode::Forbidden);

    // With the hub off, a picture is just not available; nothing breaks.
    server.kill();
    go_offline(&till).await;
    let offline = till
        .runtime
        .call(
            "products_image_get",
            json!({ "token": till.token, "input": { "productId": product } }),
        )
        .await;
    assert!(offline.is_err());
}
