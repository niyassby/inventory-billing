pub mod migrations;

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct DbState(pub Mutex<Connection>);

pub fn init_db(app_handle: &AppHandle) -> Connection {
    // Attempt to get the local app data directory for Tauri
    let mut db_path = app_handle
        .path()
        .app_local_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));

    // Ensure the directory exists
    if !db_path.exists() {
        std::fs::create_dir_all(&db_path).ok();
    }

    db_path.push("inventory_app.db");
    
    println!("Database path: {:?}", db_path);

    let conn = Connection::open(db_path).expect("Failed to open SQLite database");
    
    // Run migrations
    migrations::run_migrations(&conn).expect("Failed to run database migrations");

    conn
}
