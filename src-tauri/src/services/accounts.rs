//! System chart-of-accounts ids (seeded by migration 0006) and payment-account rules.

use rusqlite::{Connection, OptionalExtension};

use crate::error::{AppError, AppResult};

pub const CASH: &str = "01920000-0000-7000-8001-000000001000";
pub const BANK: &str = "01920000-0000-7000-8001-000000001010";
pub const RECEIVABLE: &str = "01920000-0000-7000-8001-000000001200";
pub const INVENTORY: &str = "01920000-0000-7000-8001-000000001400";
pub const INPUT_TAX: &str = "01920000-0000-7000-8001-000000001500";
pub const PAYABLE: &str = "01920000-0000-7000-8001-000000002000";
pub const GOODS_RECEIVED_NOT_INVOICED: &str = "01920000-0000-7000-8001-000000002050";
pub const OUTPUT_TAX: &str = "01920000-0000-7000-8001-000000002100";
pub const OPENING_BALANCES: &str = "01920000-0000-7000-8001-000000003900";
pub const RETAINED_EARNINGS: &str = "01920000-0000-7000-8001-000000003100";
pub const SALES: &str = "01920000-0000-7000-8001-000000004000";
pub const SALES_RETURNS: &str = "01920000-0000-7000-8001-000000004100";
pub const ROUND_OFF: &str = "01920000-0000-7000-8001-000000004900";
pub const COGS: &str = "01920000-0000-7000-8001-000000005000";
pub const STOCK_ADJUSTMENTS: &str = "01920000-0000-7000-8001-000000005400";
pub const GENERAL_EXPENSES: &str = "01920000-0000-7000-8001-000000005900";

/// Account money moves through for a payment: the given cash/bank account, or the default for
/// the mode (cash → Cash in Hand, everything else → Bank Account).
pub fn payment_account(
    conn: &Connection,
    account_id: Option<&str>,
    mode: &str,
) -> AppResult<String> {
    match account_id {
        Some(id) => {
            let subtype: Option<String> = conn
                .query_row(
                    "SELECT subtype FROM accounts WHERE id = ?1 AND deleted_at IS NULL AND is_active = 1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            match subtype.as_deref() {
                Some("cash" | "bank") => Ok(id.to_string()),
                Some(_) => Err(AppError::field(
                    "accountId",
                    "Payments must use a cash or bank account.",
                )),
                None => Err(AppError::not_found("The account")),
            }
        }
        None if mode == "cash" => Ok(CASH.to_string()),
        None => Ok(BANK.to_string()),
    }
}
