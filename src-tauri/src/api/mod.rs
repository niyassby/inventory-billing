//! v2 commands: the only door into the services. Each command
//! * runs on a blocking thread (never on the UI thread),
//! * authenticates the session token and loads the actor's permissions,
//! * verifies an on-screen manager approval when one is attached,
//! * calls exactly one service function and returns `{ code, message, details }` on failure.
//!
//! Every command takes the same arguments: `{ token, input, approval? }` (public ones ignore the
//! token). The list below is also the API contract: `contract::schema()` turns it into the JSON
//! schema that generates the TypeScript types (packages/shared/src/generated/api.ts).

pub mod contract;

use std::time::Instant;

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

/// Commands that run as the signed-in user. `allow_pending_password` lets a user whose password
/// must be changed reach only the commands needed to change it.
async fn authed<T: Send + 'static>(
    state: &AppState,
    name: &'static str,
    token: String,
    approval: Option<ApprovalInput>,
    allow_pending_password: bool,
    f: impl FnOnce(&mut Ctx<'_>) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let state = state.clone();
    run(name, move || {
        let mut conn = state.conn()?;
        let now = state.now();
        let session = auth::authenticate(&conn, &token, now)?;
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
        f(&mut ctx)
    })
    .await
}

/// Declares commands once: generates the `#[tauri::command]` functions, the list of names, the
/// invoke handler and the contract entries.
macro_rules! commands {
    (
        $( public $pname:ident ( $pin:ty ) -> $pout:ty = $pcall:expr ; )*
        $( authed $aname:ident ( $ain:ty ) -> $aout:ty = $acall:expr ; )*
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
                authed(&state, stringify!($aname), token, approval, false, move |ctx| ($acall)(ctx, &input)).await
            }
        )*

        /// Every v2 command name (used to route invokes between v1 and v2 handlers).
        pub const NAMES: &[&str] = &[ $( stringify!($pname), )* $( stringify!($aname), )* "auth_logout", "auth_me", "auth_change_password" ];

        pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![ $( $pname, )* $( $aname, )* auth_logout, auth_me, auth_change_password ]
        }

        /// (name, needs a session, input schema, output schema) for the contract.
        pub(crate) fn specs(gen: &mut schemars::gen::SchemaGenerator) -> Vec<contract::Spec> {
            let mut specs = vec![
                $( contract::Spec::new::<$pin, $pout>(gen, stringify!($pname), false), )*
                $( contract::Spec::new::<$ain, $aout>(gen, stringify!($aname), true), )*
            ];
            specs.push(contract::Spec::new::<NoInput, ()>(gen, "auth_logout", true));
            specs.push(contract::Spec::new::<NoInput, auth::SessionUser>(gen, "auth_me", true));
            specs.push(contract::Spec::new::<auth::ChangePasswordInput, ()>(gen, "auth_change_password", true));
            specs
        }
    };
}

use crate::services::{
    catalog, common::Page, expenses, parties, payments, purchases, sales, settings, setup, stock,
    users,
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
    public auth_login(auth::LoginInput) -> AuthSession = |state: &AppState, conn: &mut Conn, input: &auth::LoginInput| auth::login(conn, &state.limiter, state.now(), input);
    public auth_pin_login(auth::PinLoginInput) -> AuthSession = |state: &AppState, conn: &mut Conn, input: &auth::PinLoginInput| auth::pin_login(conn, &state.limiter, state.now(), input);

    authed auth_set_pin(auth::SetPinInput) -> () = |ctx: &mut Ctx<'_>, input: &auth::SetPinInput| auth::set_pin(ctx.conn, ctx.actor, ctx.now, input);

    authed settings_get(NoInput) -> settings::Settings = |ctx: &mut Ctx<'_>, _: &NoInput| settings::get(ctx);
    authed settings_public(NoInput) -> serde_json::Value = |ctx: &mut Ctx<'_>, _: &NoInput| settings::public(ctx.conn);
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
    authed products_delete(users::IdInput) -> () = catalog::delete_product;
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
}

// Commands with special session handling.

/// Ends the session. Always succeeds, even for an expired token.
#[tauri::command]
pub async fn auth_logout(state: State<'_, AppState>, token: String) -> AppResult<()> {
    public(&state, "auth_logout", move |state, conn| {
        auth::logout(conn, &token, state.now())
    })
    .await
}

/// The signed-in user (reachable while a password change is pending).
#[tauri::command]
pub async fn auth_me(state: State<'_, AppState>, token: String) -> AppResult<auth::SessionUser> {
    authed(&state, "auth_me", token, None, true, |ctx| {
        auth::session_user(ctx.conn, &ctx.actor.user_id)
    })
    .await
}

#[tauri::command]
pub async fn auth_change_password(
    state: State<'_, AppState>,
    token: String,
    input: auth::ChangePasswordInput,
) -> AppResult<()> {
    authed(
        &state,
        "auth_change_password",
        token,
        None,
        true,
        move |ctx| auth::change_password(ctx.conn, ctx.actor, ctx.now, &input),
    )
    .await
}
