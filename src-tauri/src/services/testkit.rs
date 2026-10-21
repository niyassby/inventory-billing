//! Test fixtures: a real (temporary file) database set up through the setup service, a fixed
//! clock, and helpers to act as users of each role.

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;

use super::auth::{self, LoginInput};
use super::catalog::{self, ProductInput, VariantInput};
use super::context::{Actor, Approver, Ctx};
use super::parties::{self, PartyInput};
use super::setup::{self, OwnerInput, SetupInput};
use super::users::{self, CreateUserInput};
use crate::app::{AppState, Conn, FixedClock};
use crate::error::AppError;

pub const OWNER_EMAIL: &str = "owner@shop.in";
pub const PASSWORD: &str = "Strong-pass-2026";

pub struct Kit {
    _dir: tempfile::TempDir,
    pub state: AppState,
    pub clock: Arc<FixedClock>,
}

/// 17 Oct 2026, 11:00 in India.
pub fn kit_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 17, 5, 30, 0).unwrap()
}

impl Default for Kit {
    fn default() -> Self {
        Self::new()
    }
}

impl Kit {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(FixedClock::new(kit_now()));
        let state = AppState::open(&dir.path().join("v2.db"), clock.clone()).unwrap();
        let kit = Self {
            _dir: dir,
            state,
            clock,
        };
        let mut conn = kit.conn();
        setup::create_business(
            &mut conn,
            kit.now(),
            &SetupInput {
                business_name: "Ravi Stores".into(),
                address: Some("MG Road, Bengaluru".into()),
                phone: None,
                email: None,
                gstin: Some("29AAGCB7383J1Z4".into()),
                state_code: None,
                currency: None,
                currency_minor_digits: None,
                timezone: None,
                fy_start_month: None,
                tax_regime: None,
                prices_include_tax: Some(false),
                owner: OwnerInput {
                    name: "Owner".into(),
                    email: OWNER_EMAIL.into(),
                    password: PASSWORD.into(),
                },
            },
        )
        .unwrap();
        kit
    }

    pub fn conn(&self) -> Conn {
        self.state.conn().unwrap()
    }

    pub fn now(&self) -> DateTime<Utc> {
        use crate::app::Clock;
        self.clock.now()
    }

    pub fn login(&self, email: &str) -> Actor {
        let mut conn = self.conn();
        let session = auth::login(
            &mut conn,
            &self.state.limiter,
            self.now(),
            &LoginInput {
                email: email.into(),
                password: PASSWORD.into(),
            },
        )
        .unwrap();
        auth::authenticate(&conn, &session.token, self.now())
            .unwrap()
            .actor
    }

    pub fn owner(&self) -> Actor {
        self.login(OWNER_EMAIL)
    }

    /// Creates a user with a built-in role ("manager", "cashier", "stock_keeper", "accountant").
    pub fn user(&self, role_code: &str) -> Actor {
        let owner = self.owner();
        let mut conn = self.conn();
        let role_id: String = conn
            .query_row("SELECT id FROM roles WHERE code = ?1", [role_code], |r| {
                r.get(0)
            })
            .unwrap();
        let email = format!("{role_code}@shop.in");
        let exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM users WHERE email = ?1",
                [&email],
                |r| r.get(0),
            )
            .unwrap();
        if exists == 0 {
            users::create(
                &mut self.ctx(&mut conn, &owner),
                &CreateUserInput {
                    name: role_code.into(),
                    email: email.clone(),
                    phone: None,
                    role_id,
                    temporary_password: PASSWORD.into(),
                },
            )
            .unwrap();
        }
        drop(conn);
        self.login(&email)
    }

    pub fn approver(&self, actor: &Actor) -> Approver {
        Approver {
            user_id: actor.user_id.clone(),
            name: actor.name.clone(),
            permissions: actor.permissions.clone(),
            max_discount_rate: actor.max_discount_rate,
        }
    }

    pub fn ctx<'a>(&self, conn: &'a mut Connection, actor: &'a Actor) -> Ctx<'a> {
        Ctx {
            conn,
            actor,
            approver: None,
            now: self.now(),
        }
    }

    /// A simple product with one variant: price and cost in minor units, stock in whole units.
    pub fn product(
        &self,
        name: &str,
        sku: &str,
        price: i64,
        cost: i64,
        stock_units: i64,
        tax_rate_id: Option<&str>,
    ) -> String {
        let owner = self.owner();
        let mut conn = self.conn();
        let unit: String = conn.query_row("SELECT id FROM units WHERE code = 'pcs'", [], |r| r.get(0)).unwrap_or_else(|_| {
            let id = crate::domain::ids::new_id();
            conn.execute(
                "INSERT INTO units (id, code, name, decimals, created_at, updated_at) VALUES (?1, 'pcs', 'Pieces', 0, ?2, ?2)",
                rusqlite::params![id, self.now().to_rfc3339()],
            )
            .unwrap();
            id
        });
        let detail = catalog::save_product(
            &mut self.ctx(&mut conn, &owner),
            &ProductInput {
                id: None,
                expected_updated_at: None,
                name: name.into(),
                description: None,
                category_id: None,
                brand_id: None,
                base_unit_id: unit,
                tax_rate_id: tax_rate_id.map(String::from),
                hsn_sac: Some("1006".into()),
                track_batches: false,
                track_expiry: false,
                is_service: false,
                is_active: true,
                variants: vec![VariantInput {
                    id: None,
                    variant_name: None,
                    sku: sku.into(),
                    barcode: Some(format!("89{sku}").replace('-', "")),
                    sale_price: price,
                    mrp: None,
                    cost_price: Some(cost),
                    reorder_level: 0,
                    reorder_qty: 0,
                    is_active: true,
                    opening_stock: Some(stock_units * 1000),
                }],
                unit_conversions: Vec::new(),
            },
        )
        .unwrap();
        detail.variants[0].id.clone()
    }

    pub fn gst_rate(&self, percent: i64) -> String {
        let mut conn = self.conn();
        // Setup seeds the standard GST slabs; reuse them.
        if let Ok(id) = conn.query_row(
            "SELECT id FROM tax_rates WHERE name = ?1 AND deleted_at IS NULL",
            [format!("GST {percent}%")],
            |r| r.get::<_, String>(0),
        ) {
            return id;
        }
        let owner = self.owner();
        catalog::save_tax_rate(
            &mut self.ctx(&mut conn, &owner),
            &catalog::TaxRateInput {
                id: None,
                name: format!("GST {percent}%"),
                tax_type: "gst".into(),
                rate: percent * 10_000,
                cess_rate: 0,
                is_active: true,
            },
        )
        .unwrap()
        .id
    }

    pub fn party(&self, name: &str, party_type: &str, credit_limit: Option<i64>) -> String {
        let mut conn = self.conn();
        let owner = self.owner();
        parties::save(
            &mut self.ctx(&mut conn, &owner),
            &PartyInput {
                id: None,
                expected_updated_at: None,
                party_type: party_type.into(),
                code: None,
                name: name.into(),
                phone: None,
                email: None,
                gstin: None,
                state_code: Some("29".into()),
                billing_address: None,
                shipping_address: None,
                credit_limit,
                credit_days: None,
                notes: None,
                is_active: true,
                opening_balance: None,
            },
        )
        .unwrap()
        .id
    }

    pub fn stock(&self, variant_id: &str) -> i64 {
        self.conn()
            .query_row(
                "SELECT coalesce(sum(qty), 0) FROM stock_levels WHERE variant_id = ?1",
                [variant_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    pub fn balance(&self, party_id: &str) -> i64 {
        self.conn()
            .query_row(
                "SELECT balance FROM parties WHERE id = ?1",
                [party_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// Every ledger, cache and journal invariant must hold after any operation.
    pub fn assert_books_balance(&self) {
        let conn = self.conn();
        for view in [
            "v_stock_level_mismatches",
            "v_party_balance_mismatches",
            "v_unbalanced_journal_entries",
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {view}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{view} is not empty");
        }
        let (debit, credit): (i64, i64) = conn
            .query_row(
                "SELECT coalesce(sum(debit), 0), coalesce(sum(credit), 0) FROM journal_lines",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(debit, credit, "trial balance");
        // Receivables and payables control accounts against the parties' balances. A party that
        // is both customer and supplier has one balance (what they owe us less what we owe them),
        // so the exact rule is: receivables less payables equals every party's balance added up;
        // with no such party each control account matches its own side.
        let ar: i64 = conn
            .query_row(
                "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines WHERE account_id = ?1",
                [super::accounts::RECEIVABLE],
                |r| r.get(0),
            )
            .unwrap();
        let ap: i64 = conn
            .query_row(
                "SELECT coalesce(sum(credit - debit), 0) FROM journal_lines WHERE account_id = ?1",
                [super::accounts::PAYABLE],
                |r| r.get(0),
            )
            .unwrap();
        let all_parties: i64 = conn
            .query_row("SELECT coalesce(sum(balance), 0) FROM parties", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            ar - ap,
            all_parties,
            "receivables less payables vs party balances"
        );
        let both: i64 = conn
            .query_row(
                "SELECT count(*) FROM parties WHERE party_type = 'both'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if both == 0 {
            let customers: i64 = conn
                .query_row(
                    "SELECT coalesce(sum(balance), 0) FROM parties WHERE party_type = 'customer'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                ar, customers,
                "receivables control account vs customer balances"
            );
            let suppliers: i64 = conn
                .query_row(
                    "SELECT coalesce(sum(-balance), 0) FROM parties WHERE party_type = 'supplier'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                ap, suppliers,
                "payables control account vs supplier balances"
            );
        }
        // Inventory account equals stock value.
        let inventory: i64 = conn
            .query_row(
                "SELECT coalesce(sum(debit - credit), 0) FROM journal_lines WHERE account_id = ?1",
                [super::accounts::INVENTORY],
                |r| r.get(0),
            )
            .unwrap();
        let stock_value: i64 = conn
            .query_row(
                "SELECT coalesce(sum(value), 0) FROM stock_levels",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(inventory, stock_value, "inventory account vs stock value");
    }
}

pub fn code(result: Result<impl std::fmt::Debug, AppError>) -> crate::error::ErrorCode {
    result.expect_err("expected an error").code
}
