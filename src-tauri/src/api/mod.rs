//! v2 commands: the only door into the services. Each command
//! * runs on a blocking thread (never on the UI thread),
//! * authenticates the session token and loads the actor's permissions,
//! * verifies an on-screen manager approval when one is attached,
//! * calls exactly one service function and returns `{ code, message, details }` on failure.
//!
//! Every command takes the same arguments: `{ token, input, approval? }` (public ones ignore the
//! token). The list below is also the API contract: `contract::schema()` turns it into the JSON
//! schema that generates the TypeScript types (packages/shared/src/generated/api.ts).
//!
//! The same list serves three doors: Tauri IPC on this PC, `dispatch` for counters calling the
//! hub over the LAN (crate::hub), and — in client mode — the counter's proxy (crate::client),
//! which forwards IPC calls to the hub and answers a few of them offline.

pub mod contract;
pub mod local;
pub mod printers;

use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::State;

use crate::app::{AppState, Conn};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::services::auth::{self, ApprovalInput, AuthSession};
use crate::services::common::NoInput;
use crate::services::Ctx;

async fn run<T: Send + 'static>(
    name: &'static str,
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let started = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(AppError::internal(format!("task failed: {e}"))));
    let ms = started.elapsed().as_millis();
    match &result {
        Ok(_) => tracing::info!(command = name, ms, "ok"),
        Err(e) => tracing::info!(command = name, ms, code = e.code.as_str(), "failed"),
    }
    result
}

/// Commands that need no session (setup and sign-in).
async fn public<T: Send + 'static>(
    state: &AppState,
    name: &'static str,
    f: impl FnOnce(&AppState, &mut Conn) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let state = state.clone();
    run(name, move || {
        let mut conn = state.conn()?;
        f(&state, &mut conn)
    })
    .await
}

/// Who is calling: this PC's own window, or a paired device over the LAN.
#[derive(Debug, Clone)]
pub enum Caller {
    Local,
    Device(String),
}

/// Commands that run as the signed-in user. `allow_pending_password` lets a user whose password
/// must be changed reach only the commands needed to change it. A session works only where it was
/// created: sessions of LAN devices are refused locally and on every other device.
async fn authed<T: Send + 'static>(
    state: &AppState,
    name: &'static str,
    caller: Caller,
    token: String,
    approval: Option<ApprovalInput>,
    allow_pending_password: bool,
    f: impl FnOnce(&AppState, &mut Ctx<'_>) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let state = state.clone();
    run(name, move || {
        let mut conn = state.conn()?;
        let now = state.now();
        let session = match &caller {
            Caller::Local => {
                let session = auth::authenticate(&conn, &token, now)?;
                if session.actor.device_id.is_some() {
                    return Err(AppError::unauthorized());
                }
                session
            }
            Caller::Device(device_id) => {
                auth::authenticate_on_device(&conn, &token, now, device_id)?
            }
        };
        if session.must_change_password && !allow_pending_password {
            return Err(AppError::new(
                ErrorCode::PasswordChangeRequired,
                "Please set a new password before continuing.",
            ));
        }
        let approver = approval
            .map(|a| auth::verify_approver(&mut conn, &state.limiter, now, &a))
            .transpose()?;
        let mut ctx = Ctx {
            conn: &mut conn,
            actor: &session.actor,
            approver: approver.as_ref(),
            now,
        };
        f(&state, &mut ctx)
    })
    .await
}

/// Commands that only make sense on the hub PC itself (they touch its files or create the
/// business); a LAN device gets FORBIDDEN.
pub const LOCAL_ONLY: &[&str] = &[
    "setup_create_business",
    "setup_import_legacy",
    "backup_restore",
    "backup_set_passphrase",
    "backup_verify",
];

fn decode<T: DeserializeOwned>(input: Value) -> AppResult<T> {
    // A missing input is the same as `{}` (commands without input).
    let input = if input.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        input
    };
    serde_json::from_value(input).map_err(|e| AppError::validation(format!("Invalid input: {e}")))
}

/// Declares commands once: generates the `#[tauri::command]` functions, the list of names, the
/// invoke handler, the LAN dispatcher and the contract entries.
macro_rules! commands {
    (
        $( public $pname:ident ( $pin:ty ) -> $pout:ty = $pcall:expr ; )*
        $( authed $aname:ident ( $ain:ty ) -> $aout:ty = $acall:expr ; )*
        $( stateful $sname:ident ( $sin:ty ) -> $sout:ty = $scall:expr ; )*
    ) => {
        $(
            #[tauri::command]
            pub async fn $pname(state: State<'_, AppState>, input: $pin) -> AppResult<$pout> {
                public(&state, stringify!($pname), move |state, conn| ($pcall)(state, conn, &input)).await
            }
        )*
        $(
            #[tauri::command]
            pub async fn $aname(state: State<'_, AppState>, token: String, input: $ain, approval: Option<ApprovalInput>) -> AppResult<$aout> {
                authed(&state, stringify!($aname), Caller::Local, token, approval, false, move |_, ctx| ($acall)(ctx, &input)).await
            }
        )*
        $(
            #[tauri::command]
            pub async fn $sname(state: State<'_, AppState>, token: String, input: $sin, approval: Option<ApprovalInput>) -> AppResult<$sout> {
                authed(&state, stringify!($sname), Caller::Local, token, approval, false, move |state, ctx| ($scall)(state, ctx, &input)).await
            }
        )*

        /// Every v2 command name (used to route invokes between v1 and v2 handlers).
        pub const NAMES: &[&str] = &[ $( stringify!($pname), )* $( stringify!($aname), )* $( stringify!($sname), )* "auth_logout", "auth_me", "auth_change_password" ];

        pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![ $( $pname, )* $( $aname, )* $( $sname, )* auth_logout, auth_me, auth_change_password ]
        }

        /// Runs a command from JSON (the LAN API and the development bridge use this).
        pub async fn dispatch_as(
            state: &AppState,
            caller: Caller,
            name: &str,
            token: String,
            input: Value,
            approval: Option<ApprovalInput>,
        ) -> AppResult<Value> {
            let value = match name {
                $(
                    stringify!($pname) => {
                        let input: $pin = decode(input)?;
                        serde_json::to_value(public(state, stringify!($pname), move |state, conn| ($pcall)(state, conn, &input)).await?)?
                    }
                )*
                $(
                    stringify!($aname) => {
                        let input: $ain = decode(input)?;
                        serde_json::to_value(authed(state, stringify!($aname), caller, token, approval, false, move |_, ctx| ($acall)(ctx, &input)).await?)?
                    }
                )*
                $(
                    stringify!($sname) => {
                        let input: $sin = decode(input)?;
                        serde_json::to_value(authed(state, stringify!($sname), caller, token, approval, false, move |state, ctx| ($scall)(state, ctx, &input)).await?)?
                    }
                )*
                "auth_logout" => serde_json::to_value(logout(state, token).await?)?,
                "auth_me" => serde_json::to_value(me(state, caller, token).await?)?,
                "auth_change_password" => {
                    let input: auth::ChangePasswordInput = decode(input)?;
                    serde_json::to_value(change_password(state, caller, token, input).await?)?
                }
                _ => return Err(AppError::not_found("That command")),
            };
            Ok(value)
        }

        /// (name, needs a session, input schema, output schema) for the contract.
        pub(crate) fn specs(gen: &mut schemars::gen::SchemaGenerator) -> Vec<contract::Spec> {
            let mut specs = vec![
                $( contract::Spec::new::<$pin, $pout>(gen, stringify!($pname), false), )*
                $( contract::Spec::new::<$ain, $aout>(gen, stringify!($aname), true), )*
                $( contract::Spec::new::<$sin, $sout>(gen, stringify!($sname), true), )*
            ];
            specs.push(contract::Spec::new::<NoInput, ()>(gen, "auth_logout", true));
            specs.push(contract::Spec::new::<NoInput, auth::SessionUser>(gen, "auth_me", true));
            specs.push(contract::Spec::new::<auth::ChangePasswordInput, ()>(gen, "auth_change_password", true));
            specs
        }
    };
}

use crate::printing::labels::{LabelItem, LabelTemplate};
use crate::services::{
    accounting, attachments, backup, bank, catalog, cheques, common::Page, devices, documents,
    expenses, images, inventory, manual_journals as journals, notifications, parties, payments,
    period, price_lists, printing, purchases, report_views, reports, sales, settings, setup, stock,
    sync, upgrade, users,
};

fn legacy_database_found(state: &AppState) -> bool {
    state
        .db_path
        .parent()
        .is_some_and(|dir| dir.join("inventory_app.db").is_file())
}

commands! {
    public setup_status(NoInput) -> setup::SetupStatus = |state: &AppState, conn: &mut Conn, _: &NoInput| setup::status(conn, legacy_database_found(state));
    public setup_create_business(setup::SetupInput) -> AuthSession = |state: &AppState, conn: &mut Conn, input: &setup::SetupInput| {
        let now = state.now();
        let email = setup::create_business(conn, now, input)?;
        auth::login(conn, &state.limiter, now, &auth::LoginInput { email, password: input.owner.password.clone() })
    };
    public setup_import_legacy(upgrade::ImportLegacyInput) -> upgrade::ImportLegacyResult = |state: &AppState, _: &mut Conn, input: &upgrade::ImportLegacyInput| upgrade::import_legacy(state, input);
    public auth_login(auth::LoginInput) -> AuthSession = |state: &AppState, conn: &mut Conn, input: &auth::LoginInput| auth::login(conn, &state.limiter, state.now(), input);
    public auth_pin_login(auth::PinLoginInput) -> AuthSession = |state: &AppState, conn: &mut Conn, input: &auth::PinLoginInput| auth::pin_login(conn, &state.limiter, state.now(), input);

    authed auth_set_pin(auth::SetPinInput) -> () = |ctx: &mut Ctx<'_>, input: &auth::SetPinInput| auth::set_pin(ctx.conn, ctx.actor, ctx.now, input);

    authed settings_get(NoInput) -> settings::Settings = |ctx: &mut Ctx<'_>, _: &NoInput| settings::get(ctx);
    authed settings_public(NoInput) -> settings::PublicSettings = |ctx: &mut Ctx<'_>, _: &NoInput| settings::public(ctx.conn);
    authed settings_update(settings::UpdateSettingsInput) -> settings::Settings = settings::update;

    authed users_list(NoInput) -> Vec<users::UserSummary> = |ctx: &mut Ctx<'_>, _: &NoInput| users::list(ctx);
    authed users_create(users::CreateUserInput) -> users::UserSummary = users::create;
    authed users_update(users::UpdateUserInput) -> users::UserSummary = users::update;
    authed users_reset_password(users::ResetPasswordInput) -> () = users::reset_password;
    authed users_delete(users::IdInput) -> () = users::delete;
    authed roles_list(NoInput) -> Vec<users::RoleSummary> = |ctx: &mut Ctx<'_>, _: &NoInput| users::list_roles(ctx);
    authed roles_save(users::RoleInput) -> users::RoleSummary = users::save_role;
    authed roles_delete(users::IdInput) -> () = users::delete_role;
    authed permissions_list(NoInput) -> Vec<users::PermissionInfo> = |ctx: &mut Ctx<'_>, _: &NoInput| users::list_permissions(ctx);

    authed categories_list(NoInput) -> Vec<catalog::Category> = |ctx: &mut Ctx<'_>, _: &NoInput| catalog::list_categories(ctx);
    authed categories_save(catalog::CategoryInput) -> catalog::IdOut = catalog::save_category;
    authed brands_list(NoInput) -> Vec<catalog::Brand> = |ctx: &mut Ctx<'_>, _: &NoInput| catalog::list_brands(ctx);
    authed brands_save(catalog::BrandInput) -> catalog::IdOut = catalog::save_brand;
    authed units_list(NoInput) -> Vec<catalog::Unit> = |ctx: &mut Ctx<'_>, _: &NoInput| catalog::list_units(ctx);
    authed units_save(catalog::UnitInput) -> catalog::IdOut = catalog::save_unit;
    authed tax_rates_list(NoInput) -> Vec<catalog::TaxRate> = |ctx: &mut Ctx<'_>, _: &NoInput| catalog::list_tax_rates(ctx);
    authed tax_rates_save(catalog::TaxRateInput) -> catalog::IdOut = catalog::save_tax_rate;
    authed catalog_delete_master(catalog::DeleteMasterInput) -> () = catalog::delete_master;
    authed products_list(catalog::ProductListQuery) -> Page<catalog::ProductListItem> = |ctx: &mut Ctx<'_>, q: &catalog::ProductListQuery| catalog::list_products(ctx, q);
    authed products_get(users::IdInput) -> catalog::ProductDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| catalog::get_product(ctx, i);
    authed products_save(catalog::ProductInput) -> catalog::ProductDetail = catalog::save_product;
    authed products_search(catalog::SearchQuery) -> Vec<catalog::SearchItem> = |ctx: &mut Ctx<'_>, q: &catalog::SearchQuery| catalog::search(ctx, q);
    authed products_lookup(catalog::SearchQuery) -> Option<catalog::SearchItem> = |ctx: &mut Ctx<'_>, q: &catalog::SearchQuery| catalog::lookup_code(ctx, q);

    authed parties_list(parties::PartyListQuery) -> Page<parties::PartyListItem> = |ctx: &mut Ctx<'_>, q: &parties::PartyListQuery| parties::list(ctx, q);
    authed parties_get(users::IdInput) -> parties::PartyDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| parties::get(ctx, i);
    authed parties_save(parties::PartyInput) -> parties::PartyDetail = parties::save;
    authed parties_delete(users::IdInput) -> () = parties::delete;
    authed parties_statement(parties::StatementQuery) -> parties::PartyStatement = |ctx: &mut Ctx<'_>, q: &parties::StatementQuery| parties::statement(ctx, q);
    authed parties_open_documents(users::IdInput) -> Vec<parties::OpenDocument> = |ctx: &mut Ctx<'_>, i: &users::IdInput| parties::list_open_documents(ctx, i);

    authed stock_warehouses(NoInput) -> Vec<stock::Warehouse> = |ctx: &mut Ctx<'_>, _: &NoInput| stock::list_warehouses(ctx);
    authed stock_adjust(stock::AdjustInput) -> stock::StockDocResult = stock::adjust;
    authed stock_transfer(stock::TransferInput) -> stock::StockDocResult = stock::transfer;
    authed stock_card(stock::StockCardQuery) -> stock::StockCard = |ctx: &mut Ctx<'_>, q: &stock::StockCardQuery| stock::stock_card(ctx, q);
    authed stock_count_start(stock::CountStartInput) -> stock::CountDetail = stock::start_count;
    authed stock_count_get(users::IdInput) -> stock::CountDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| stock::get_count(ctx, i);
    authed stock_count_enter(stock::CountEntriesInput) -> stock::CountDetail = stock::enter_counts;
    authed stock_count_finalize(stock::CountFinalizeInput) -> stock::CountDetail = stock::finalize_count;

    authed sales_preview(sales::PreviewInput) -> sales::Preview = |ctx: &mut Ctx<'_>, i: &sales::PreviewInput| sales::preview(ctx, i);
    authed sales_create_invoice(sales::SaleInput) -> sales::InvoiceDetail = sales::create_invoice;
    authed sales_get_invoice(users::IdInput) -> sales::InvoiceDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| sales::get_invoice(ctx, i);
    authed sales_list_invoices(sales::InvoiceListQuery) -> Page<sales::InvoiceListItem> = |ctx: &mut Ctx<'_>, q: &sales::InvoiceListQuery| sales::list_invoices(ctx, q);
    authed sales_void_invoice(sales::VoidInput) -> sales::InvoiceDetail = sales::void_invoice;
    authed sales_create_return(sales::ReturnInput) -> sales::ReturnDetail = sales::create_return;
    authed sales_get_return(users::IdInput) -> sales::ReturnDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| sales::get_return(ctx, i);
    authed sales_docs_save(sales::SalesDocInput) -> sales::SalesDocDetail = sales::save_sales_doc;
    authed sales_docs_get(users::IdInput) -> sales::SalesDocDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| sales::get_sales_doc(ctx, i);
    authed sales_docs_list(sales::SalesDocListQuery) -> Page<sales::SalesDocListItem> = |ctx: &mut Ctx<'_>, q: &sales::SalesDocListQuery| sales::list_sales_docs(ctx, q);
    authed sales_docs_set_status(sales::SalesDocStatusInput) -> sales::SalesDocDetail = sales::set_sales_doc_status;
    authed sales_docs_convert(sales::ConvertInput) -> sales::InvoiceDetail = sales::convert_sales_doc;
    authed sales_hold(sales::HoldInput) -> sales::HeldBill = sales::hold;
    authed sales_held_list(NoInput) -> Vec<sales::HeldBill> = |ctx: &mut Ctx<'_>, _: &NoInput| sales::list_held(ctx);
    authed sales_recall(users::IdInput) -> sales::HeldBill = sales::recall;

    authed payments_record(payments::PaymentEntryInput) -> payments::PaymentDetail = payments::record;
    authed payments_allocate(payments::AllocateInput) -> payments::PaymentDetail = payments::allocate;
    authed payments_void(payments::VoidPaymentInput) -> payments::PaymentDetail = payments::void;
    authed payments_get(users::IdInput) -> payments::PaymentDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| payments::get(ctx, i);
    authed payments_list(payments::PaymentListQuery) -> Page<payments::PaymentListItem> = |ctx: &mut Ctx<'_>, q: &payments::PaymentListQuery| payments::list(ctx, q);
    authed payments_accounts(NoInput) -> Vec<payments::PaymentAccount> = |ctx: &mut Ctx<'_>, _: &NoInput| payments::list_accounts(ctx);
    authed register_open(payments::OpenRegisterInput) -> payments::RegisterSession = payments::open_register;
    authed register_close(payments::CloseRegisterInput) -> payments::RegisterSession = payments::close_register;
    authed register_current(NoInput) -> Option<payments::RegisterSession> = |ctx: &mut Ctx<'_>, _: &NoInput| payments::current_register(ctx);

    authed purchases_save_order(purchases::PurchaseOrderInput) -> purchases::PurchaseDocDetail = purchases::save_order;
    authed purchases_get_order(users::IdInput) -> purchases::PurchaseDocDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| purchases::get_order(ctx, i);
    authed purchases_cancel_order(users::IdInput) -> purchases::PurchaseDocDetail = purchases::cancel_order;
    authed purchases_receive_goods(purchases::GoodsReceiptInput) -> purchases::GoodsReceiptDetail = purchases::receive_goods;
    authed purchases_create_bill(purchases::PurchaseBillInput) -> purchases::PurchaseDocDetail = purchases::create_bill;
    authed purchases_get_bill(users::IdInput) -> purchases::PurchaseDocDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| purchases::get_bill(ctx, i);
    authed purchases_void_bill(purchases::VoidBillInput) -> purchases::PurchaseDocDetail = purchases::void_bill;
    authed purchases_create_return(purchases::PurchaseReturnInput) -> purchases::PurchaseDocDetail = purchases::create_return;
    authed purchases_list(purchases::PurchaseListQuery) -> Page<purchases::PurchaseListItem> = |ctx: &mut Ctx<'_>, q: &purchases::PurchaseListQuery| purchases::list(ctx, q);

    authed expenses_categories(NoInput) -> Vec<expenses::ExpenseCategory> = |ctx: &mut Ctx<'_>, _: &NoInput| expenses::list_categories(ctx);
    authed expenses_save_category(expenses::ExpenseCategoryInput) -> expenses::ExpenseCategory = expenses::save_category;
    authed expenses_create(expenses::ExpenseInput) -> expenses::ExpenseDetail = expenses::create;
    authed expenses_void(expenses::VoidExpenseInput) -> expenses::ExpenseDetail = expenses::void;
    authed expenses_list(expenses::ExpenseListQuery) -> Page<expenses::ExpenseDetail> = |ctx: &mut Ctx<'_>, q: &expenses::ExpenseListQuery| expenses::list(ctx, q);

    authed stock_documents(documents::StockDocQuery) -> Page<documents::StockDocListItem> = |ctx: &mut Ctx<'_>, q: &documents::StockDocQuery| documents::list_stock_documents(ctx, q);
    authed stock_document_get(documents::StockDocRef) -> stock::StockDocResult = |ctx: &mut Ctx<'_>, i: &documents::StockDocRef| documents::get_stock_document(ctx, i);
    authed warehouses_save(documents::WarehouseInput) -> stock::Warehouse = documents::save_warehouse;
    authed sales_list_returns(documents::ReturnListQuery) -> Page<documents::ReturnListItem> = |ctx: &mut Ctx<'_>, q: &documents::ReturnListQuery| documents::list_sales_returns(ctx, q);
    authed sales_invoice_returns(users::IdInput) -> Vec<sales::ReturnDetail> = |ctx: &mut Ctx<'_>, i: &users::IdInput| documents::returns_of_invoice(ctx, i);
    authed purchases_list_receipts(documents::ReceiptListQuery) -> Page<documents::ReceiptListItem> = |ctx: &mut Ctx<'_>, q: &documents::ReceiptListQuery| documents::list_goods_receipts(ctx, q);
    authed purchases_get_receipt(users::IdInput) -> purchases::GoodsReceiptDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| documents::get_goods_receipt(ctx, i);
    authed purchases_get_return(users::IdInput) -> purchases::PurchaseDocDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| documents::get_purchase_return(ctx, i);

    authed reports_run(reports::ReportQuery) -> reports::ReportTable = |ctx: &mut Ctx<'_>, q: &reports::ReportQuery| reports::run(ctx, q);
    authed accounts_list(NoInput) -> Vec<accounting::AccountRow> = |ctx: &mut Ctx<'_>, _: &NoInput| accounting::list_accounts(ctx);
    authed accounts_save(accounting::MoneyAccountInput) -> accounting::AccountRow = accounting::save_money_account;
    authed day_summary(accounting::DayInput) -> accounting::DaySummary = |ctx: &mut Ctx<'_>, i: &accounting::DayInput| accounting::day_summary(ctx, i);
    authed day_close(accounting::DayInput) -> accounting::DaySummary = accounting::close_day;
    authed day_closes_list(NoInput) -> Vec<accounting::DayCloseRow> = |ctx: &mut Ctx<'_>, _: &NoInput| accounting::list_day_closes(ctx);
    authed audit_list(accounting::AuditQuery) -> Page<accounting::AuditRow> = |ctx: &mut Ctx<'_>, q: &accounting::AuditQuery| accounting::list_audit(ctx, q);
    authed audit_actions(NoInput) -> Vec<String> = |ctx: &mut Ctx<'_>, _: &NoInput| accounting::audit_actions(ctx);
    authed audit_export(accounting::AuditQuery) -> accounting::AuditExport = accounting::export_audit;

    authed period_status(NoInput) -> period::PeriodStatus = |ctx: &mut Ctx<'_>, _: &NoInput| period::status(ctx);
    authed day_reopen(period::ReopenDayInput) -> () = period::reopen_day;
    authed years_list(NoInput) -> Vec<period::YearRow> = |ctx: &mut Ctx<'_>, _: &NoInput| period::years(ctx);
    authed year_close_preview(period::YearPreviewInput) -> period::YearClosePreview = |ctx: &mut Ctx<'_>, i: &period::YearPreviewInput| period::year_close_preview(ctx, i);
    authed year_close(period::YearCloseInput) -> period::YearRow = period::close_year;
    authed year_reopen(period::YearReopenInput) -> period::YearRow = period::reopen_year;

    authed journals_list(journals::JournalListQuery) -> Page<journals::JournalListItem> = |ctx: &mut Ctx<'_>, q: &journals::JournalListQuery| journals::list(ctx, q);
    authed journals_get(users::IdInput) -> journals::JournalDetail = |ctx: &mut Ctx<'_>, i: &users::IdInput| journals::get(ctx, &i.id);
    authed journals_save(journals::JournalInput) -> journals::JournalDetail = journals::save;
    authed journals_delete(journals::JournalActionInput) -> () = journals::delete;
    authed journals_submit(journals::JournalActionInput) -> journals::JournalDetail = journals::submit;
    authed journals_send_back(journals::JournalActionInput) -> journals::JournalDetail = journals::send_back;
    authed journals_approve(journals::JournalActionInput) -> journals::JournalDetail = journals::approve;
    authed journals_allow_restricted(journals::JournalActionInput) -> journals::JournalDetail = journals::allow_restricted;
    authed journals_post(journals::JournalActionInput) -> journals::JournalDetail = journals::post;
    authed journals_reverse(journals::ReverseJournalInput) -> journals::JournalDetail = journals::reverse;

    authed bank_import(bank::StatementImportInput) -> bank::StatementImportResult = bank::import;
    authed bank_view(bank::BankQuery) -> bank::BankView = |ctx: &mut Ctx<'_>, q: &bank::BankQuery| bank::view(ctx, q);
    authed bank_auto_match(bank::AccountInput) -> bank::AutoMatchResult = bank::auto_match;
    authed bank_match(bank::MatchInput) -> () = bank::match_lines;
    authed bank_unmatch(bank::UnmatchInput) -> () = bank::unmatch;
    authed bank_ignore(bank::IgnoreInput) -> () = bank::ignore;
    authed bank_tick(bank::TickInput) -> () = bank::tick;
    authed bank_reconciliation(bank::ReconcileQuery) -> bank::Reconciliation = |ctx: &mut Ctx<'_>, q: &bank::ReconcileQuery| bank::reconciliation(ctx, q);
    authed bank_finish(bank::FinishInput) -> bank::ReconciliationRecord = bank::finish;
    authed bank_history(bank::AccountInput) -> Vec<bank::ReconciliationRecord> = |ctx: &mut Ctx<'_>, i: &bank::AccountInput| bank::history(ctx, i);

    authed cheques_list(cheques::ChequeQuery) -> Page<cheques::Cheque> = |ctx: &mut Ctx<'_>, q: &cheques::ChequeQuery| cheques::list(ctx, q);
    authed cheques_save(cheques::ChequeInput) -> cheques::Cheque = cheques::save;
    authed cheques_clear(cheques::ChequeClearInput) -> cheques::Cheque = cheques::clear;
    authed cheques_bounce(cheques::ChequeStatusInput) -> cheques::Cheque = cheques::bounce;
    authed cheques_cancel(cheques::ChequeStatusInput) -> cheques::Cheque = cheques::cancel;

    authed report_views_list(NoInput) -> Vec<report_views::ReportView> = |ctx: &mut Ctx<'_>, _: &NoInput| report_views::list(ctx);
    authed report_views_save(report_views::ReportViewInput) -> report_views::ReportView = report_views::save;
    authed report_views_delete(users::IdInput) -> () = report_views::delete;
    authed report_views_schedule(report_views::ScheduleInput) -> report_views::ReportView = report_views::set_schedule;
    authed report_views_run_now(users::IdInput) -> String = report_views::run_now;

    authed devices_pairing_code(NoInput) -> devices::PairingCode = |ctx: &mut Ctx<'_>, _: &NoInput| devices::create_pairing_code(ctx);
    authed devices_list(NoInput) -> Vec<devices::DeviceSummary> = |ctx: &mut Ctx<'_>, _: &NoInput| devices::list(ctx);
    authed devices_revoke(users::IdInput) -> () = devices::revoke;
    authed sync_exceptions(sync::ExceptionQuery) -> Page<sync::SyncException> = |ctx: &mut Ctx<'_>, q: &sync::ExceptionQuery| sync::list_exceptions(ctx, q);
    authed sync_resolve_exception(sync::ResolveExceptionInput) -> () = sync::resolve_exception;
    authed notifications_list(notifications::NotificationQuery) -> Vec<notifications::Notification> = |ctx: &mut Ctx<'_>, q: &notifications::NotificationQuery| notifications::list(ctx, q);
    authed notifications_mark_read(users::IdInput) -> () = notifications::mark_read;

    authed printers_record_change(printing::PrinterChangeInput) -> () = printing::record_printer_change;
    authed drawer_authorize(printing::DrawerInput) -> () = printing::authorize_drawer;
    authed print_jobs_list(printing::PrintJobQuery) -> Page<printing::PrintJob> = |ctx: &mut Ctx<'_>, q: &printing::PrintJobQuery| printing::list_jobs(ctx, q);
    authed print_jobs_cancel(users::IdInput) -> printing::PrintJob = printing::cancel_job;
    authed labels_templates_list(NoInput) -> Vec<LabelTemplate> = |ctx: &mut Ctx<'_>, _: &NoInput| printing::list_templates(ctx);
    authed labels_templates_save(LabelTemplate) -> LabelTemplate = printing::save_template;
    authed labels_templates_delete(users::IdInput) -> () = printing::delete_template;
    authed labels_items(printing::LabelSourceInput) -> Vec<LabelItem> = |ctx: &mut Ctx<'_>, i: &printing::LabelSourceInput| printing::label_items(ctx, i);
    authed labels_preview(printing::LabelPreviewInput) -> Vec<printing::LabelPreview> = |ctx: &mut Ctx<'_>, i: &printing::LabelPreviewInput| printing::preview(ctx, i);
    authed products_generate_barcodes(printing::GenerateBarcodesInput) -> Vec<printing::GeneratedBarcode> = printing::generate_barcodes;

    authed price_lists_list(NoInput) -> Vec<price_lists::PriceListSummary> = |ctx: &mut Ctx<'_>, _: &NoInput| price_lists::list(ctx);
    authed price_lists_save(price_lists::PriceListInput) -> price_lists::PriceListSummary = price_lists::save;
    authed price_lists_delete(users::IdInput) -> () = price_lists::delete;
    authed price_lists_items(price_lists::PriceListItemQuery) -> Page<price_lists::PriceListItemRow> = |ctx: &mut Ctx<'_>, q: &price_lists::PriceListItemQuery| price_lists::items(ctx, q);
    authed price_lists_set_prices(price_lists::SetPricesInput) -> price_lists::SetPricesResult = price_lists::set_prices;
    authed price_lists_bulk_update(price_lists::BulkUpdateInput) -> price_lists::BulkUpdateResult = price_lists::bulk_update;
    authed price_lists_tills(NoInput) -> Vec<price_lists::TillPriceList> = |ctx: &mut Ctx<'_>, _: &NoInput| price_lists::tills(ctx);
    authed price_lists_set_till(price_lists::SetTillPriceListInput) -> Vec<price_lists::TillPriceList> = price_lists::set_till;

    authed stock_batches(inventory::BatchQuery) -> Vec<inventory::BatchStock> = |ctx: &mut Ctx<'_>, q: &inventory::BatchQuery| inventory::batches(ctx, q);
    authed stock_write_off(inventory::WriteOffInput) -> stock::StockDocResult = inventory::write_off_batch;
    authed inventory_expiry_summary(NoInput) -> inventory::ExpirySummary = |ctx: &mut Ctx<'_>, _: &NoInput| inventory::expiry_summary(ctx);
    authed inventory_expiry_list(inventory::ExpiryQuery) -> Vec<inventory::ExpiryRow> = |ctx: &mut Ctx<'_>, q: &inventory::ExpiryQuery| inventory::expiry_list(ctx, q);
    authed inventory_valuation_preview(NoInput) -> inventory::ValuationPreview = |ctx: &mut Ctx<'_>, _: &NoInput| inventory::valuation_preview(ctx);
    authed inventory_valuation_switch(inventory::ValuationSwitchInput) -> inventory::ValuationSwitchResult = inventory::valuation_switch;
    authed unit_conversions_list(NoInput) -> Vec<inventory::UnitConversion> = |ctx: &mut Ctx<'_>, _: &NoInput| inventory::conversions(ctx);
    authed unit_conversions_save(inventory::UnitConversionInput) -> Vec<inventory::UnitConversion> = inventory::save_conversion;
    authed unit_conversions_delete(users::IdInput) -> Vec<inventory::UnitConversion> = inventory::delete_conversion;

    stateful hub_status(NoInput) -> crate::node::HubInfo = |state: &AppState, ctx: &mut Ctx<'_>, _: &NoInput| {
        ctx.require("devices.manage")?;
        Ok(state.hub.read().map_err(AppError::internal_lock)?.clone())
    };
    stateful backup_status(NoInput) -> backup::BackupStatus = |state: &AppState, ctx: &mut Ctx<'_>, _: &NoInput| backup::status(state, ctx);
    stateful backup_now(NoInput) -> backup::BackupRun = |state: &AppState, ctx: &mut Ctx<'_>, _: &NoInput| {
        ctx.require("backup.manage")?;
        backup::create(state, "manual", Some(&ctx.actor.user_id))
    };
    stateful backup_verify(backup::RestoreInput) -> backup::VerifiedBackup = |state: &AppState, ctx: &mut Ctx<'_>, input: &backup::RestoreInput| backup::verify(state, ctx, input);
    stateful backup_restore(backup::RestoreInput) -> backup::RestoreResult = backup::restore;
    stateful backup_set_passphrase(backup::SetPassphraseInput) -> () = backup::set_passphrase;
    stateful products_delete(users::IdInput) -> () = images::delete_product;
    stateful products_image_set(images::ImageSetInput) -> images::ImageSaved = images::set;
    stateful products_image_remove(images::ImageRemoveInput) -> images::ImageSaved = images::remove;
    stateful products_image_get(images::ImageGetInput) -> Option<images::ImageData> = images::get;
    stateful printers_shared(NoInput) -> Vec<printing::SharedPrinter> = |state: &AppState, _: &mut Ctx<'_>, _: &NoInput| Ok(printing::shared_printers(state));
    stateful print_jobs_submit(printing::SubmitPrintJobInput) -> printing::PrintJob = printing::submit;
    stateful print_jobs_retry(users::IdInput) -> printing::PrintJob = printing::retry_job;
    stateful attachments_add(attachments::AttachmentAddInput) -> attachments::AttachmentInfo = attachments::add;
    stateful attachments_get(attachments::AttachmentRef) -> attachments::AttachmentFile = attachments::get;
    stateful attachments_remove(attachments::AttachmentRef) -> () = attachments::remove;
}

/// A command sent by a paired device over the LAN (crate::hub). Same commands, same checks; the
/// session must belong to that device, and hub-only commands are refused.
pub async fn dispatch(
    state: &AppState,
    device_id: &str,
    name: &str,
    token: String,
    input: Value,
    approval: Option<ApprovalInput>,
) -> AppResult<Value> {
    if LOCAL_ONLY.contains(&name) {
        return Err(AppError::new(
            ErrorCode::Forbidden,
            "This can only be done on the hub PC itself.",
        ));
    }
    let caller = Caller::Device(device_id.to_string());
    let value = dispatch_as(state, caller, name, token, input, approval).await?;
    // A session started over the LAN belongs to the device that started it.
    if matches!(name, "auth_login" | "auth_pin_login") {
        if let Some(session_token) = value.get("token").and_then(Value::as_str) {
            auth::bind_session_to_device(&*state.conn()?, session_token, device_id)?;
        }
    }
    Ok(value)
}

// Commands with special session handling.

async fn logout(state: &AppState, token: String) -> AppResult<()> {
    public(state, "auth_logout", move |state, conn| {
        auth::logout(conn, &token, state.now())
    })
    .await
}

async fn me(state: &AppState, caller: Caller, token: String) -> AppResult<auth::SessionUser> {
    authed(state, "auth_me", caller, token, None, true, |_, ctx| {
        auth::session_user(ctx.conn, &ctx.actor.user_id)
    })
    .await
}

async fn change_password(
    state: &AppState,
    caller: Caller,
    token: String,
    input: auth::ChangePasswordInput,
) -> AppResult<()> {
    authed(
        state,
        "auth_change_password",
        caller,
        token,
        None,
        true,
        move |_, ctx| auth::change_password(ctx.conn, ctx.actor, ctx.now, &input),
    )
    .await
}

/// Ends the session. Always succeeds, even for an expired token.
#[tauri::command]
pub async fn auth_logout(state: State<'_, AppState>, token: String) -> AppResult<()> {
    logout(&state, token).await
}

/// The signed-in user (reachable while a password change is pending).
#[tauri::command]
pub async fn auth_me(state: State<'_, AppState>, token: String) -> AppResult<auth::SessionUser> {
    me(&state, Caller::Local, token).await
}

#[tauri::command]
pub async fn auth_change_password(
    state: State<'_, AppState>,
    token: String,
    input: auth::ChangePasswordInput,
) -> AppResult<()> {
    change_password(&state, Caller::Local, token, input).await
}
