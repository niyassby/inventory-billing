/**
 * Domain constants shared by the UI and mirrored in the Rust `domain::enums` module (Phase 3).
 * String values are what goes over the wire and into the database, so treat them as stable IDs.
 */

export const APP_NAME = "Inventory & Billing";

export const ROLES = ["owner", "manager", "cashier", "stock_keeper", "accountant"] as const;
export type Role = (typeof ROLES)[number];

/** Legacy role strings (pre-v2 `users.role`) → new roles. See docs/ASSUMPTIONS.md A-16. */
export const LEGACY_ROLE_MAP: Readonly<Record<string, Role>> = {
  admin: "owner",
  manager: "manager",
  accountant: "accountant",
  staff: "cashier",
};

export const PERMISSIONS = [
  "dashboard.view",
  "products.view",
  "products.manage",
  "products.price_change",
  "stock.view",
  "stock.adjust",
  "stock.transfer",
  "stock.count",
  "sales.create",
  "sales.view",
  "sales.discount",
  "sales.discount_override",
  "sales.price_override",
  "sales.credit_override",
  "sales.void",
  "sales.return",
  "sales.backdate",
  "quotations.manage",
  "purchases.view",
  "purchases.manage",
  "purchases.receive",
  "parties.view",
  "parties.manage",
  "payments.receive",
  "payments.pay",
  "expenses.manage",
  "accounting.view",
  "accounting.manage",
  "day_close.perform",
  "reports.view",
  "reports.export",
  "settings.view",
  "settings.manage",
  "users.manage",
  "backup.manage",
  "audit.view",
  "approvals.grant",
] as const;
export type Permission = (typeof PERMISSIONS)[number];

const ALL: readonly Permission[] = PERMISSIONS;

/** Defaults for the built-in roles; owners can create custom roles in Settings → Users. */
export const DEFAULT_ROLE_PERMISSIONS: Readonly<Record<Role, readonly Permission[]>> = {
  owner: ALL,
  manager: ALL.filter((p) => p !== "users.manage" && p !== "backup.manage"),
  cashier: [
    "dashboard.view",
    "products.view",
    "stock.view",
    "sales.create",
    "sales.view",
    "sales.discount",
    "quotations.manage",
    "parties.view",
    "parties.manage",
    "payments.receive",
  ],
  stock_keeper: [
    "dashboard.view",
    "products.view",
    "products.manage",
    "stock.view",
    "stock.adjust",
    "stock.transfer",
    "stock.count",
    "purchases.view",
    "purchases.receive",
    "parties.view",
  ],
  accountant: [
    "dashboard.view",
    "products.view",
    "stock.view",
    "sales.view",
    "purchases.view",
    "parties.view",
    "payments.receive",
    "payments.pay",
    "expenses.manage",
    "accounting.view",
    "accounting.manage",
    "day_close.perform",
    "reports.view",
    "reports.export",
    "audit.view",
  ],
};

export function roleHasPermission(role: Role, permission: Permission): boolean {
  return DEFAULT_ROLE_PERMISSIONS[role].includes(permission);
}

export const PAYMENT_MODES = [
  "cash",
  "card",
  "upi",
  "bank_transfer",
  "cheque",
  "credit",
  "wallet",
] as const;
export type PaymentMode = (typeof PAYMENT_MODES)[number];

export const DOCUMENT_TYPES = [
  "sales_invoice",
  "sales_return",
  "quotation",
  "sales_order",
  "delivery_challan",
  "credit_note",
  "purchase_order",
  "goods_receipt",
  "purchase_bill",
  "purchase_return",
  "payment_in",
  "payment_out",
  "expense",
  "stock_transfer",
  "stock_adjustment",
] as const;
export type DocumentType = (typeof DOCUMENT_TYPES)[number];

export const DOCUMENT_STATUSES = ["draft", "finalized", "voided"] as const;
export type DocumentStatus = (typeof DOCUMENT_STATUSES)[number];

/** Every change to stock is one of these rows in `stock_movements` (the stock ledger). */
export const STOCK_MOVEMENT_TYPES = [
  "opening",
  "sale",
  "sale_return",
  "purchase",
  "purchase_return",
  "adjustment_in",
  "adjustment_out",
  "transfer_in",
  "transfer_out",
  "count_variance",
  "void_reversal",
] as const;
export type StockMovementType = (typeof STOCK_MOVEMENT_TYPES)[number];

export const NEGATIVE_STOCK_POLICIES = ["block", "warn", "allow"] as const;
export type NegativeStockPolicy = (typeof NEGATIVE_STOCK_POLICIES)[number];

export const OFFLINE_SALES_POLICIES = [
  "block",
  "allow_reserved_numbers",
  "allow_restricted",
] as const;
export type OfflineSalesPolicy = (typeof OFFLINE_SALES_POLICIES)[number];

export const DEVICE_MODES = ["standalone", "hub", "client"] as const;
export type DeviceMode = (typeof DEVICE_MODES)[number];

export const VALUATION_METHODS = ["weighted_average", "fifo"] as const;
export type ValuationMethod = (typeof VALUATION_METHODS)[number];

/**
 * Stable error codes returned by the backend. The UI maps them to friendly messages; never match
 * on message text.
 */
export const ERROR_CODES = [
  "UNAUTHORIZED",
  "FORBIDDEN",
  "APPROVAL_REQUIRED",
  "VALIDATION",
  "NOT_FOUND",
  "CONFLICT",
  "INSUFFICIENT_STOCK",
  "TOTALS_MISMATCH",
  "DOCUMENT_FINALIZED",
  "RATE_LIMITED",
  "LOCKED_OUT",
  "HUB_OFFLINE",
  "SETUP_REQUIRED",
  "PASSWORD_CHANGE_REQUIRED",
  "INTERNAL",
] as const;
export type ErrorCode = (typeof ERROR_CODES)[number];

export function isErrorCode(value: string): value is ErrorCode {
  return (ERROR_CODES as readonly string[]).includes(value);
}
