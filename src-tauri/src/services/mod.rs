//! Business services: every rule, permission check and transaction lives here. Commands
//! (`crate::api`) only authenticate, decode and delegate; the UI never decides anything.
//!
//! Conventions:
//! * every write runs in one IMMEDIATE transaction (`write`), so it either fully happens or not;
//! * document-creating calls take an idempotency key (`idempotency`), so retries never duplicate;
//! * money/qty/rates are integers (docs/DATABASE.md §2) and totals are always computed here.

pub mod accounting;
pub mod accounts;
pub mod audit;
pub mod auth;
pub mod backup;
pub mod catalog;
pub mod common;
pub mod context;
pub mod devices;
pub mod documents;
pub mod expenses;
pub mod idempotency;
pub mod images;
pub mod inventory;
pub mod journal;
pub mod notifications;
pub mod numbering;
pub mod parties;
pub mod party_ledger;
pub mod payments;
pub mod price_lists;
pub mod printing;
pub mod purchases;
pub mod reports;
pub mod sales;
pub mod settings;
pub mod setup;
pub mod stock;
pub mod stock_posting;
pub mod sync;
pub mod upgrade;
pub mod users;

#[cfg(test)]
pub mod testkit;
#[cfg(test)]
mod tests;

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::AppResult;

pub use context::{Actor, Approver, Ctx};

/// Runs `f` in an IMMEDIATE transaction: the write lock is taken up front, so two counters
/// selling the last unit are serialised instead of both reading stale stock.
pub fn write<T>(
    conn: &mut Connection,
    f: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
) -> AppResult<T> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let value = f(&tx)?;
    tx.commit()?;
    Ok(value)
}
