//! Process-wide state for the v2 backend: the connection pool, the clock and the rate limiter.
//! Cloned into every command (cheap: everything is behind `Arc`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;

use crate::db;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::secrets::{MemorySecrets, SecretStore};

pub type DbPool = Pool<SqliteConnectionManager>;
pub type Conn = PooledConnection<SqliteConnectionManager>;

/// Time source. Services never call `Utc::now()` directly, so tests can pin the date.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Test clock that can be moved forward.
pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Mutex::new(at))
    }

    pub fn set(&self, at: DateTime<Utc>) {
        *self.0.lock().expect("clock lock") = at;
    }

    pub fn advance(&self, by: chrono::Duration) {
        let mut now = self.0.lock().expect("clock lock");
        *now += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

/// In-memory sliding-window limiter for login and PIN attempts (per e-mail / per device).
/// Persistent lockout lives in the users table; this only slows down rapid guessing.
#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn check(&self, key: &str, max: usize, window: Duration) -> AppResult<()> {
        let mut hits = self
            .hits
            .lock()
            .map_err(|_| AppError::internal("rate limiter poisoned"))?;
        let now = Instant::now();
        let entry = hits.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < window);
        if entry.len() >= max {
            return Err(AppError::new(
                ErrorCode::RateLimited,
                "Too many attempts. Please wait a minute and try again.",
            ));
        }
        entry.push(now);
        Ok(())
    }

    pub fn reset(&self, key: &str) {
        if let Ok(mut hits) = self.hits.lock() {
            hits.remove(key);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub clock: Arc<dyn Clock>,
    pub limiter: Arc<RateLimiter>,
    pub db_path: PathBuf,
    /// OS keychain in the app, memory in tests (device credential, TLS key, backup key).
    pub secrets: Arc<dyn SecretStore>,
    /// State of the LAN server when this PC is the hub.
    pub hub: Arc<std::sync::RwLock<crate::node::HubInfo>>,
}

impl AppState {
    /// Opens (creating and migrating if needed) the v2 database and builds the pool.
    pub fn open(db_path: &Path, clock: Arc<dyn Clock>) -> AppResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        check_existing_file(db_path)?;
        {
            let mut conn = db::open(db_path)?;
            db::migrate::run(&mut conn)?;
        }
        let manager = SqliteConnectionManager::file(db_path).with_init(|conn| {
            db::configure(conn, true).map_err(|e| match e {
                db::DbError::Sqlite(inner) => inner,
                other => rusqlite::Error::InvalidParameterName(other.to_string()),
            })
        });
        let pool = Pool::builder()
            .max_size(8)
            .connection_timeout(Duration::from_secs(10))
            .build(manager)
            .map_err(AppError::internal)?;
        Ok(Self {
            pool,
            clock,
            limiter: Arc::new(RateLimiter::default()),
            db_path: db_path.to_path_buf(),
            secrets: Arc::new(MemorySecrets::default()),
            hub: Arc::default(),
        })
    }

    pub fn with_secrets(mut self, secrets: Arc<dyn SecretStore>) -> Self {
        self.secrets = secrets;
        self
    }

    /// Folder next to the database (backups, hub certificate, client cache).
    pub fn data_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    pub fn conn(&self) -> AppResult<Conn> {
        Ok(self.pool.get()?)
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }
}

/// An existing database file is checked before the app uses it. A damaged file is never opened,
/// repaired or replaced: a copy is kept next to it (`.corrupt-<time>`) for support, and the error
/// says what to do (restore the latest backup).
fn check_existing_file(db_path: &Path) -> AppResult<()> {
    if !db_path.exists() {
        return Ok(());
    }
    let result =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .and_then(|c| c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0)));
    let problem = match result {
        Ok(text) if text == "ok" => return Ok(()),
        Ok(text) => text,
        Err(e) => e.to_string(),
    };
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let copy = db_path.with_extension(format!("db.corrupt-{stamp}"));
    let kept = std::fs::copy(db_path, &copy).is_ok();
    tracing::error!(
        "database check failed ({problem}); {}",
        if kept {
            format!("a copy was kept at {}", copy.display())
        } else {
            "a copy could not be kept".to_string()
        }
    );
    Err(AppError::new(
        ErrorCode::Internal,
        "The data file failed its safety check, so the app did not open it. Nothing was changed.          Restore the latest backup (see the Admin guide, \"If the data file is damaged\").",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_blocks_after_max_attempts() {
        let limiter = RateLimiter::default();
        for _ in 0..3 {
            limiter.check("a", 3, Duration::from_secs(60)).unwrap();
        }
        assert_eq!(
            limiter
                .check("a", 3, Duration::from_secs(60))
                .unwrap_err()
                .code,
            ErrorCode::RateLimited
        );
        limiter.check("b", 3, Duration::from_secs(60)).unwrap();
        limiter.reset("a");
        limiter.check("a", 3, Duration::from_secs(60)).unwrap();
    }

    #[test]
    fn pool_connections_have_foreign_keys_on() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(&dir.path().join("v2.db"), Arc::new(SystemClock)).unwrap();
        let a = state.conn().unwrap();
        let b = state.conn().unwrap();
        for conn in [&a, &b] {
            let fk: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk, 1);
        }
    }

    #[test]
    fn a_damaged_data_file_is_kept_and_not_opened() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory_v2.db");
        {
            let state = AppState::open(&path, Arc::new(SystemClock)).unwrap();
            drop(state);
        }
        // Damage the file: overwrite the middle of the first pages.
        let mut bytes = std::fs::read(&path).unwrap();
        for b in bytes.iter_mut().skip(4096).take(8192) {
            *b = 0xA5;
        }
        std::fs::write(&path, &bytes).unwrap();
        let before = std::fs::read(&path).unwrap();

        let err = AppState::open(&path, Arc::new(SystemClock)).err().unwrap();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("Restore the latest backup"));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "the file is untouched"
        );
        let kept = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(kept, "a copy is kept for support");
    }
}
