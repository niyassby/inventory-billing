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
}

impl AppState {
    /// Opens (creating and migrating if needed) the v2 database and builds the pool.
    pub fn open(db_path: &Path, clock: Arc<dyn Clock>) -> AppResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
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
        })
    }

    pub fn conn(&self) -> AppResult<Conn> {
        Ok(self.pool.get()?)
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }
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
}
