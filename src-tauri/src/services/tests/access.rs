//! Setup, sign-in, sessions, approvals, users and roles.

use chrono::Duration;

use crate::error::ErrorCode;
use crate::services::auth::{
    self, ApprovalInput, ChangePasswordInput, LoginInput, PinLoginInput, SetPinInput,
};
use crate::services::context::approve;
use crate::services::setup::{self, OwnerInput, SetupInput};
use crate::services::testkit::{code, Kit, OWNER_EMAIL, PASSWORD};
use crate::services::users::{self, CreateUserInput, IdInput, RoleInput, UpdateUserInput};
use crate::services::{settings, Ctx};

fn login(
    kit: &Kit,
    email: &str,
    password: &str,
) -> Result<auth::AuthSession, crate::error::AppError> {
    auth::login(
        &mut kit.conn(),
        &kit.state.limiter,
        kit.now(),
        &LoginInput {
            email: email.into(),
            password: password.into(),
        },
    )
}

#[test]
fn setup_creates_the_business_once_and_no_default_password_exists() {
    let kit = Kit::new();
    let mut conn = kit.conn();
    assert!(!setup::needs_setup(&conn).unwrap());
    let again = setup::create_business(
        &mut conn,
        kit.now(),
        &SetupInput {
            business_name: "Other".into(),
            address: None,
            phone: None,
            email: None,
            gstin: None,
            state_code: None,
            currency: None,
            currency_minor_digits: None,
            timezone: None,
            fy_start_month: None,
            tax_regime: None,
            prices_include_tax: None,
            owner: OwnerInput {
                name: "X".into(),
                email: "x@y.in".into(),
                password: PASSWORD.into(),
            },
        },
    );
    assert_eq!(code(again), ErrorCode::Conflict);
    assert_eq!(
        code(login(&kit, "admin@example.com", "password123")),
        ErrorCode::Unauthorized
    );

    let s = settings::load(&conn).unwrap();
    assert_eq!(s.business.name, "Ravi Stores");
    assert_eq!(
        s.business.state_code.as_deref(),
        Some("29"),
        "state code derived from the GSTIN"
    );
    let series: i64 = conn
        .query_row(
            "SELECT count(*) FROM number_series WHERE is_default = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(series, 16);
}

#[test]
fn weak_owner_passwords_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::app::AppState::open(
        &dir.path().join("v2.db"),
        std::sync::Arc::new(crate::app::SystemClock),
    )
    .unwrap();
    let mut conn = state.conn().unwrap();
    for weak in ["short1", "onlyletterslong", "password123"] {
        let result = setup::create_business(
            &mut conn,
            chrono::Utc::now(),
            &SetupInput {
                business_name: "Shop".into(),
                address: None,
                phone: None,
                email: None,
                gstin: None,
                state_code: None,
                currency: None,
                currency_minor_digits: None,
                timezone: None,
                fy_start_month: None,
                tax_regime: None,
                prices_include_tax: None,
                owner: OwnerInput {
                    name: "O".into(),
                    email: "o@shop.in".into(),
                    password: weak.into(),
                },
            },
        );
        assert_eq!(code(result), ErrorCode::Validation, "{weak}");
    }
    assert!(setup::needs_setup(&conn).unwrap());
}

#[test]
fn login_locks_out_after_repeated_failures_and_unlocks_later() {
    let kit = Kit::new();
    for _ in 0..4 {
        assert_eq!(
            code(login(&kit, OWNER_EMAIL, "wrong-password-1")),
            ErrorCode::Unauthorized
        );
    }
    assert_eq!(
        code(login(&kit, OWNER_EMAIL, "wrong-password-1")),
        ErrorCode::LockedOut
    );
    // Even the right password is refused while locked.
    assert_eq!(
        code(login(&kit, OWNER_EMAIL, PASSWORD)),
        ErrorCode::LockedOut
    );
    kit.clock.advance(Duration::minutes(16));
    kit.state.limiter.reset(&format!("login:{OWNER_EMAIL}"));
    let session = login(&kit, OWNER_EMAIL, PASSWORD).unwrap();
    assert_eq!(session.user.role_code, "owner");
    assert!(session
        .user
        .permissions
        .contains(&"users.manage".to_string()));

    let failures: i64 = kit
        .conn()
        .query_row(
            "SELECT count(*) FROM audit_log WHERE action = 'LOGIN_FAILED'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(failures, 5);
}

#[test]
fn unknown_emails_and_emails_in_other_case() {
    let kit = Kit::new();
    assert_eq!(
        code(login(&kit, "nobody@shop.in", PASSWORD)),
        ErrorCode::Unauthorized
    );
    login(&kit, "OWNER@Shop.IN", PASSWORD).unwrap();
}

#[test]
fn sessions_expire_when_idle_and_after_logout() {
    let kit = Kit::new();
    let session = login(&kit, OWNER_EMAIL, PASSWORD).unwrap();
    let conn = kit.conn();
    auth::authenticate(&conn, &session.token, kit.now()).unwrap();
    assert_eq!(
        code(auth::authenticate(&conn, "not-a-token", kit.now()).map(|_| ())),
        ErrorCode::Unauthorized
    );

    kit.clock.advance(Duration::minutes(29));
    auth::authenticate(&conn, &session.token, kit.now()).unwrap();
    kit.clock.advance(Duration::minutes(31));
    assert_eq!(
        code(auth::authenticate(&conn, &session.token, kit.now()).map(|_| ())),
        ErrorCode::Unauthorized
    );

    let second = login(&kit, OWNER_EMAIL, PASSWORD).unwrap();
    auth::logout(&conn, &second.token, kit.now()).unwrap();
    assert_eq!(
        code(auth::authenticate(&conn, &second.token, kit.now()).map(|_| ())),
        ErrorCode::Unauthorized
    );
}

#[test]
fn new_users_must_change_their_temporary_password() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let session = login(&kit, "cashier@shop.in", PASSWORD).unwrap();
    assert!(session.must_change_password);
    let mut conn = kit.conn();
    let same = auth::change_password(
        &mut conn,
        &cashier,
        kit.now(),
        &ChangePasswordInput {
            current_password: PASSWORD.into(),
            new_password: PASSWORD.into(),
        },
    );
    assert_eq!(code(same), ErrorCode::Validation);
    auth::change_password(
        &mut conn,
        &cashier,
        kit.now(),
        &ChangePasswordInput {
            current_password: PASSWORD.into(),
            new_password: "Another-pass-77".into(),
        },
    )
    .unwrap();
    let again = login(&kit, "cashier@shop.in", "Another-pass-77").unwrap();
    assert!(!again.must_change_password);
}

#[test]
fn pin_login_and_manager_approval() {
    let kit = Kit::new();
    let manager = kit.user("manager");
    let mut conn = kit.conn();
    assert_eq!(
        code(auth::set_pin(
            &mut conn,
            &manager,
            kit.now(),
            &SetPinInput {
                current_password: PASSWORD.into(),
                pin: "1234".into()
            }
        )),
        ErrorCode::Validation,
        "sequential PINs are refused"
    );
    auth::set_pin(
        &mut conn,
        &manager,
        kit.now(),
        &SetPinInput {
            current_password: PASSWORD.into(),
            pin: "4826".into(),
        },
    )
    .unwrap();
    let session = auth::pin_login(
        &mut conn,
        &kit.state.limiter,
        kit.now(),
        &PinLoginInput {
            email: "manager@shop.in".into(),
            pin: "4826".into(),
        },
    )
    .unwrap();
    assert_eq!(session.user.role_code, "manager");

    let cashier = kit.user("cashier");
    // A cashier can't void; with the manager's PIN the action is approved and attributed.
    assert_eq!(
        code(approve(&cashier, None, "sales.void", "void invoice")),
        ErrorCode::ApprovalRequired
    );
    let approver = auth::verify_approver(
        &mut conn,
        &kit.state.limiter,
        kit.now(),
        &ApprovalInput {
            email: "manager@shop.in".into(),
            pin: Some("4826".into()),
            password: None,
        },
    )
    .unwrap();
    assert_eq!(
        approve(&cashier, Some(&approver), "sales.void", "void invoice").unwrap(),
        Some(approver.user_id.clone())
    );
    // Another cashier can't approve.
    let other = kit.approver(&cashier);
    assert_eq!(
        code(approve(
            &cashier,
            Some(&other),
            "sales.void",
            "void invoice"
        )),
        ErrorCode::Forbidden
    );
    let wrong = auth::verify_approver(
        &mut conn,
        &kit.state.limiter,
        kit.now(),
        &ApprovalInput {
            email: "manager@shop.in".into(),
            pin: Some("0000".into()),
            password: None,
        },
    );
    assert_eq!(code(wrong.map(|_| ())), ErrorCode::Forbidden);
}

#[test]
fn permissions_are_enforced_by_the_services() {
    let kit = Kit::new();
    let cashier = kit.user("cashier");
    let mut conn = kit.conn();
    let mut ctx = Ctx {
        conn: &mut conn,
        actor: &cashier,
        approver: None,
        now: kit.now(),
    };
    assert_eq!(code(users::list(&ctx)), ErrorCode::Forbidden);
    assert_eq!(code(settings::get(&ctx)), ErrorCode::Forbidden);
    let update = settings::update(
        &mut ctx,
        &settings::UpdateSettingsInput {
            section: "tax".into(),
            value: serde_json::json!({ "roundTotalTo": 0 }),
        },
    );
    assert_eq!(code(update), ErrorCode::Forbidden);
}

#[test]
fn the_last_owner_cannot_be_removed() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let manager_role: String = conn
        .query_row("SELECT id FROM roles WHERE code = 'manager'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut ctx = kit.ctx(&mut conn, &owner);
    let demote = users::update(
        &mut ctx,
        &UpdateUserInput {
            id: owner.user_id.clone(),
            name: "Owner".into(),
            phone: None,
            role_id: manager_role,
            is_active: true,
        },
    );
    assert_eq!(code(demote), ErrorCode::Conflict);
    assert_eq!(
        code(users::delete(
            &mut ctx,
            &IdInput {
                id: owner.user_id.clone()
            }
        )),
        ErrorCode::Conflict
    );
}

#[test]
fn users_and_custom_roles() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut ctx = kit.ctx(&mut conn, &owner);
    let role = users::save_role(
        &mut ctx,
        &RoleInput {
            id: None,
            code: "store_helper".into(),
            name: "Store helper".into(),
            max_discount_rate: 0,
            permissions: vec!["products.view".into(), "stock.view".into()],
        },
    )
    .unwrap();
    assert!(!role.is_system);
    let bad = users::save_role(
        &mut ctx,
        &RoleInput {
            id: None,
            code: "x".into(),
            name: "X".into(),
            max_discount_rate: 0,
            permissions: vec!["nope.perm".into()],
        },
    );
    assert_eq!(code(bad), ErrorCode::Validation);
    let user = users::create(
        &mut ctx,
        &CreateUserInput {
            name: "Helper".into(),
            email: "helper@shop.in".into(),
            phone: None,
            role_id: role.id.clone(),
            temporary_password: PASSWORD.into(),
        },
    )
    .unwrap();
    let duplicate = users::create(
        &mut ctx,
        &CreateUserInput {
            name: "Dup".into(),
            email: "HELPER@shop.in".into(),
            phone: None,
            role_id: role.id.clone(),
            temporary_password: PASSWORD.into(),
        },
    );
    assert_eq!(code(duplicate), ErrorCode::Validation);
    assert_eq!(
        code(users::delete_role(
            &mut ctx,
            &IdInput {
                id: role.id.clone()
            }
        )),
        ErrorCode::Conflict
    );
    users::delete(&mut ctx, &IdInput { id: user.id }).unwrap();
    users::delete_role(&mut ctx, &IdInput { id: role.id }).unwrap();
    let owner_role: String = ctx
        .conn
        .query_row("SELECT id FROM roles WHERE code = 'owner'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let edit_owner = users::save_role(
        &mut ctx,
        &RoleInput {
            id: Some(owner_role),
            code: "owner".into(),
            name: "Owner".into(),
            max_discount_rate: 0,
            permissions: vec![],
        },
    );
    assert_eq!(code(edit_owner), ErrorCode::Conflict);
}

#[test]
fn settings_are_validated_versioned_and_audited() {
    let kit = Kit::new();
    let owner = kit.owner();
    let mut conn = kit.conn();
    let mut ctx = kit.ctx(&mut conn, &owner);
    let bad = settings::update(
        &mut ctx,
        &settings::UpdateSettingsInput {
            section: "business".into(),
            value: serde_json::json!({ "name": "Shop", "fyStartMonth": 13 }),
        },
    );
    assert_eq!(code(bad), ErrorCode::Validation);
    let updated = settings::update(
        &mut ctx,
        &settings::UpdateSettingsInput {
            section: "inventory".into(),
            value: serde_json::json!({ "negativeStockPolicy": "warn" }),
        },
    )
    .unwrap();
    assert_eq!(
        updated.inventory.negative_stock_policy,
        settings::NegativeStockPolicy::Warn
    );
    let (version, audits): (i64, i64) = ctx
        .conn
        .query_row(
            "SELECT (SELECT version FROM settings WHERE key = 'inventory'), (SELECT count(*) FROM audit_log WHERE action = 'SETTINGS_UPDATE')",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((version, audits), (1, 1));
    assert_eq!(
        settings::load(ctx.conn).unwrap().business_date(kit.now()),
        "2026-10-17"
    );
}
