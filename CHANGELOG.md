# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Phase 6: reports, accounting-lite and settings (2026-10-20)

#### Added
- **Reports (13):** sales summary, item profit, purchases, expenses, stock valuation, stock
  movement, receivables, payables, tax summary (with net payable), profit and loss, trial balance,
  day book, cash book and bank book. One screen for all: period presets (today to financial
  year), store and account filters, totals, print / PDF, CSV and Excel export.
- **Accounting-lite:** expenses (new, categories, void, paid now or on credit), chart of accounts
  with balances and cash/bank accounts you can add, day close with a daily summary and till
  counts, audit log viewer (filters, who approved what).
- **Settings:** business profile, tax and pricing, invoices and printing, inventory, security,
  network and offline sales, backup schedule; users (create, edit, reset password, remove) and
  roles with per-permission checkboxes; this PC's role, hub status, pairing codes, paired
  counters, offline sales to review; backups (now, passphrase, check a file, restore).
- Backend: `reports_run`, `accounts_list/save`, `day_summary/close`, `day_closes_list`,
  `audit_list/actions`; typed contract and TypeScript types regenerated.
- 5 more Rust tests (figures checked against the documents that made them, books in balance,
  access rules, day close, bank accounts, audit log); 6 more TypeScript tests (export, ranges).

#### Changed
- Navigation: Expenses, Accounts, Day close and Reports under "Accounts and reports"; Settings
  under "Admin". The "coming in the next update" page is gone.

#### Notes
- The v1 pages for accounting, reports and settings in `src/pages` are no longer needed; leads
  and barcode labels are not rebuilt (see ASSUMPTIONS A-14, A-71).

### Phase 5: core modules on the v2 backend (2026-10-19)

#### Added
- **First start:** import the previous version's data (dry run with a reconciliation report,
  then import into the new database; the old file is never written), or set up a new business
  (business, tax, owner). New businesses get default units and GST rates.
- **Sign-in** with password or PIN, forced password change, session in `sessionStorage`,
  sign-out; manager approval dialog that every screen gets for free (`APPROVAL_REQUIRED` →
  manager e-mail + PIN → the same request again).
- **App shell:** permission-aware sidebar, connection badge (standalone / hub / counter
  connected / offline / syncing), F2 opens the till from anywhere, dashboard with today's bills
  and notices.
- **Point of sale:** scan or search (scanner-friendly), keyboard control (F1, F3, F4, F7–F10,
  arrows, +/−, Delete), backend-priced totals, line and bill discounts, price changes with
  approval, customers and credit, split payments with change, hold and recall on any counter,
  till open/close with cash count, receipt printing (58/80 mm, A4).
- **Sales:** invoices (list, detail, print, void with reason), sales returns / credit notes from
  an invoice with restock and refund, quotations, sales orders and delivery challans (edit, status,
  print, convert to an invoice).
- **Products and stock:** product list (search, category, low stock), editor with variants,
  prices, MRP, cost, reorder levels, opening stock, pack sizes and barcodes; categories, brands,
  units, tax rates and stores; stock adjustments, transfers, stock counts with a variance report,
  stock card per item.
- **Purchases:** purchase orders, goods received (from an order, part deliveries), supplier bills
  (from a receipt or an order, landed charges, pay now), voids, returns to supplier.
- **Customers and suppliers:** lists with balances, editor (GSTIN check, credit limit, opening
  balance), statement, open bills, receive and pay with automatic allocation; payments list with
  void.
- Backend: stock-document, sales-return and goods-receipt lists and look-ups, store editing,
  `setup_import_legacy`, typed public settings; walk-in returns are refunded in full at the
  computed total; purchase lines carry their unit.
- Development bridge (`npm run dev:bridge`): the real backend over HTTP for the browser preview
  and end-to-end tests.
- Shared UI: data table (sorting, server paging, virtualisation, keyboard rows), money, quantity
  and rate inputs (exact), item and party pickers, line-items editor, confirm and approval
  dialogs, page frame with empty, error and loading states. 13 more shadcn primitives typed.

#### Changed
- The app no longer opens or creates the v1 database; the v1 command layer is removed and the
  v1 database code is kept only as a test fixture for the migration tests.
- The v1 screens for the rebuilt modules are removed; accounting, reports, settings, leads and
  barcode labels wait for Phase 6 (their data is imported).

#### Notes
- Tests: 103 Rust tests (new: first-run import into the open database, document lists and
  look-ups, walk-in refunds); 193 TypeScript tests (cart, settlement, held bills, printing,
  format, navigation, approval retries, transport).
- Walked through in a browser against the real backend (dev bridge): setup, sign-in, product
  create and edit, till sale with change, invoice view, walk-in return, supplier, purchase order
  → goods received → bill with part payment → supplier payment, quotation → invoice, stock
  adjustment, stock count, stock card. Afterwards the ledger checks were clean and the trial
  balance and stock value agreed with the accounts.
- Fixed during the walk-through: variant ids were hidden by the form library (editing a saved
  product), price field names for screen readers, "Tax (included)" on the till.
- Not yet run inside a packaged Tauri window; Playwright end-to-end tests come in Phase 8.

### Phase 4: offline and multi-user (LAN hub), outbox, backup (2026-10-19)

#### Added
- **Device roles:** standalone (default), hub, or counter (`device.json`; secrets in the OS
  keychain). Changing the role needs a manager's sign-in and a restart.
- **Hub:** HTTPS API on port 47900 (axum + rustls/ring) with a self-signed certificate,
  LAN-only address filter, mDNS announcement (`_invhub._tcp`), the same v2 commands as the
  window (`api::dispatch`), plus catalogue, number lease and outbox endpoints.
- **Pairing:** 6-digit one-time codes, certificate fingerprint check, a per-device credential,
  sessions bound to the device, device removal.
- **Counters:** every v2 command is forwarded to the hub. When the hub is unreachable they search
  the cached catalogue and make fully paid sales numbered from leased blocks (`C2/26-27/00001`),
  priced by the same code as the hub. Sales are queued and sent in order when the hub returns;
  retries never bill twice. Connection state (connected / slow / offline / syncing) is shown in
  the top bar. Refused offline sales wait for a manager to retry (optionally with approval) or
  discard.
- **Offline reconciliation:** offline sales are posted even when stock goes below zero, at the
  price the customer paid, dated by the hub's clock; oversells, changed prices and wrong counter
  clocks are listed in `sync_exceptions` for review.
- **Backups:** scheduled (daily 21:00 and every 4 hours), integrity-checked, zstd-compressed,
  AES-256-GCM encrypted once a passphrase is set, mirrored to a second folder, rotated (7 daily,
  4 weekly, 12 monthly). File check without restoring. Live restore after full verification and
  a safety backup. Failures raise a notice.
- **Background jobs:** backups, hourly low-stock notices, weekly integrity check, daily cleanup.
  Notifications commands for the dashboard.
- Migration 0009 (devices short codes, invoice origin, pairing codes, sync exceptions, backup
  runs, notifications, job runs; `devices.manage` and `sync.review` permissions). Network and
  backup settings sections.
- docs/ADMIN_GUIDE.md: network setup, offline behaviour, backups and the restore drill.
- 99 Rust tests, 16 new: pairing and TLS pinning, device-bound sessions, two counters selling the
  last unit while the hub is off, a lost response replayed, clock skew, refused sales retried,
  offline policy, the restore drill, tampered and wrong-passphrase backups, retention, jobs.
  165 TypeScript tests.

#### Notes
- Screens for Settings → Network / Backup, the pairing wizard and the sync issues list come with
  the Phase 5–6 screens; the commands and the top-bar status exist now.
- Not yet tested on two physical PCs over Wi-Fi; the tests run hub and counters over real HTTPS on
  127.0.0.1.

### Phase 3: backend services and permissions (2026-10-17)

#### Added
- v2 backend foundations: `AppError` with 15 stable codes (shared with TypeScript), r2d2
  connection pool (WAL, `BEGIN IMMEDIATE` writes), injectable clock, rate limiter, daily rolling
  logs (`APP_LOG` filter, 14 days kept).
- Rust pricing engine (`domain::pricing`) checked against 14 golden vectors generated from the
  TypeScript engine, so both sides price a bill identically. Financial year and number formats.
- Services on the v2 database, each checking permissions from `role_permissions`:
  - setup (business, owner, main branch, store, register, number series), sign-in with password
    or PIN, sessions with expiry and idle timeout, lockout, password change, users and roles;
  - settings (business, tax, inventory, security, invoice) with validation and audit;
  - catalogue masters, products with variants, pack sizes, barcodes, opening stock and full-text
    search; customers and suppliers with opening balances, statements and open documents;
  - stock adjustments, transfers and physical counts;
  - sales invoices (server-side pricing and totals check, credit limit, stock policy, payments,
    ledger and journal), returns, quotations/orders with conversion, held bills, voids;
  - payments in and out with allocation, cash registers;
  - purchase orders, goods receipts (GRNI), bills with landed costs, purchase returns, expenses.
- Manager approval inline per request (`APPROVAL_REQUIRED` → PIN → retry); every sensitive change
  is audited with who approved it. Idempotency keys on every create. Optimistic concurrency
  (`expected_updated_at`) on edits.
- Migrations 0007 (`sales.credit_override` permission) and 0008 (GRNI account 2050).
- Typed command layer (`src-tauri/src/api`): 87 commands with a `{ token, input, approval }`
  envelope, generated JSON schema contract and TypeScript types
  (`packages/shared/src/generated`), `createV2Client` in `src/api/client.ts`. CI fails when the
  contract or the generated types are stale.
- 83 Rust tests, 50 of them new. 37 end-to-end service tests check permissions, approvals,
  concurrency (two counters selling the last unit) and that the books balance after every flow
  (trial balance, receivables, payables and stock value).
- 162 TypeScript tests.

#### Changed
- The app opens `inventory_v2.db` at startup next to the v1 database. v1 and v2 commands share
  one invoke handler, routed by name. The screens still use v1 until Phase 5.
- `migrate-legacy` may replace the v2 file only while it is empty and unused.

#### Notes
- FIFO valuation is accepted as a setting but behaves as weighted average until Phase 6.
- The v2 commands are tested through the services and the contract. They have not been called
  from a running app window yet (no screen uses them before Phase 5).

### Phase 2: database schema and migration (2026-10-17)

#### Added
- v2 schema in 6 versioned, checksummed migrations (`src-tauri/src/db/migrations`): STRICT
  tables, integer money, quantities and rates, UUIDv7 ids, audit columns, soft delete.
- Stock ledger (`stock_movements`) and party ledger, each with a trigger-maintained cache and a
  mismatch view. Append-only journal, allocations and audit log.
- Immutable finalized documents (triggers), status transition rules, line snapshots.
- Payments with allocations to invoices, bills, returns and expenses. Paid amounts are maintained
  and validated by triggers; tendered and change are recorded separately.
- Product variants, unit conversions, price lists, batches and expiry, warehouses, transfers,
  adjustments and stock counts, purchase orders, goods receipts, bills with landed costs, returns,
  cash registers, register sessions, day close, number series with offline leases, devices, sync
  outbox and log, idempotency keys.
- FTS5 product search. Indexes designed from query patterns (docs/DATABASE.md §6).
- Migration runner: one transaction per migration, checksum verification, refuses newer schemas.
- `migrate-legacy` tool: backup → integrity check → new v2 database in one verified transaction
  → JSON report. Dry-run mode. Every v1 row archived.
- `domain::money` in Rust, tested against the same vectors as the TypeScript package.
- docs/DATABASE.md with ER diagrams. 33 Rust tests; 141 TypeScript tests in total.

#### Notes
- The app still runs on the v1 database. Phase 3 moves the services to v2.
- Dry run against the real v1 database: totals match exactly. One supplier balance difference
  (₹500) and one cancelled invoice with ₹17,700 recorded as paid are flagged for review.

### Phase 1: tooling, TypeScript, shared package (2026-10-17)

#### Added
- Strict TypeScript setup. Legacy `.jsx` pages are still allowed during the migration.
- ESLint (type-aware strict rules for TS), Prettier, EditorConfig, LF line endings.
- Vitest with an 80% coverage threshold on business logic; husky + lint-staged pre-commit hook.
- GitHub Actions CI: typecheck, lint, format, tests, build, `cargo fmt`, `clippy -D warnings`,
  `cargo test`.
- `@inventory/shared` package: integer money and quantities, centralised rounding, tax engine
  (inclusive/exclusive, CGST/SGST/IGST, line and bill discounts before tax, round-off), tender
  settlement (applied / change / due), financial year and document numbering, business date by
  timezone, GSTIN validation, Zod form schemas, roles and permissions, error codes. 104 tests.
- Typed API client with a pluggable transport and `ApiError` codes. Typed TanStack Query hooks.
- Route-level code splitting: the initial JS bundle went from 1.72 MB to 664 kB.

#### Changed
- App shell, store, hooks and 12 UI primitives converted to TypeScript.
- Branding: app name instead of "Acme" placeholders. Page title and favicon.
- `shadcn` CLI moved to devDependencies. `react-router-dom` upgraded to 7.18.4 (security fix).

#### Fixed
- The full-screen POS route could be opened without signing in.
- Clicking a column header on the Quotations page crashed (`toggleSort` was undefined).
- Report tab links and the "Reports" dashboard link pointed at routes that don't exist.
- The login page loaded a remote image (broken offline, third-party request).

#### Removed
- Unused carousel component and `embla-carousel-react`. Unused template assets.

### Phase 0: audit (2026-10-17)

- Baseline snapshot of the legacy code. `docs/AUDIT.md`, `docs/ARCHITECTURE.md`,
  `docs/ASSUMPTIONS.md`.
