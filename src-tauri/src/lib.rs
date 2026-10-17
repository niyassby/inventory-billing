#![allow(
    clippy::too_many_arguments,
    reason = "legacy commands take flat arguments; replaced by typed DTOs in Phase 3"
)]

mod auth;
mod commands;
mod database;
mod models;
mod repositories;

// v2 foundations (Phase 2). Public so the maintenance binaries in src/bin can use them.
pub mod db;
pub mod domain;
pub mod legacy_migration;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let conn = database::init_db(app.handle());
            app.manage(database::DbState(std::sync::Mutex::new(conn)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::logout,
            commands::get_profile,
            commands::get_audit_logs_cmd,
            commands::list_users_cmd,
            commands::create_user_cmd,
            commands::update_user_cmd,
            commands::delete_user_cmd,
            commands::create_category_cmd,
            commands::list_categories_cmd,
            commands::delete_category_cmd,
            commands::create_brand_cmd,
            commands::list_brands_cmd,
            commands::delete_brand_cmd,
            commands::create_product_cmd,
            commands::update_product_cmd,
            commands::delete_product_cmd,
            commands::list_products_cmd,
            commands::get_product_by_id_cmd,
            commands::list_products_filtered_cmd,
            commands::adjust_stock_cmd,
            commands::list_stock_adjustments_cmd,
            commands::create_customer_cmd,
            commands::update_customer_cmd,
            commands::list_customers_cmd,
            commands::delete_customer_cmd,
            commands::create_supplier_cmd,
            commands::update_supplier_cmd,
            commands::list_suppliers_cmd,
            commands::delete_supplier_cmd,
            commands::create_lead_cmd,
            commands::update_lead_cmd,
            commands::list_leads_cmd,
            commands::delete_lead_cmd,
            commands::create_invoice_cmd,
            commands::get_invoice_cmd,
            commands::list_invoices_cmd,
            commands::create_quotation_cmd,
            commands::get_quotation_cmd,
            commands::list_quotations_cmd,
            commands::create_purchase_cmd,
            commands::get_purchase_cmd,
            commands::list_purchases_cmd,
            commands::create_payment_cmd,
            commands::list_payments_cmd,
            commands::create_expense_cmd,
            commands::list_expenses_cmd,
            commands::delete_expense_cmd,
            commands::list_chart_of_accounts_cmd,
            commands::create_chart_of_account_cmd,
            commands::create_journal_entry_cmd,
            commands::list_journal_entries_cmd,
            commands::get_dashboard_stats_cmd,
            commands::get_sales_chart_points_cmd,
            commands::get_tax_summary_cmd,
            commands::list_tax_rates_cmd,
            commands::create_tax_rate_cmd,
            commands::backup_database_cmd,
            commands::restore_database_cmd,
            commands::hold_order_cmd,
            commands::list_held_orders_cmd,
            commands::delete_held_order_cmd,
            commands::bulk_import_products_cmd,
            commands::update_invoice_status_cmd,
            commands::receive_purchase_cmd,
            commands::update_invoice_cmd,
            commands::list_customers_filtered_cmd,
            commands::list_suppliers_filtered_cmd,
            commands::list_leads_filtered_cmd,
            commands::list_invoices_filtered_cmd,
            commands::list_quotations_filtered_cmd,
            commands::list_purchases_filtered_cmd,
            commands::list_expenses_filtered_cmd,
            commands::list_payments_filtered_cmd,
            commands::create_unit_cmd,
            commands::list_units_cmd,
            commands::delete_unit_cmd,
            commands::get_product_transactions_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
