# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Phase 9: printing and labels (2026-10-22)

Migration 0010 (print queue, label templates, three permissions). Decisions: ASSUMPTIONS A-96 to
A-107. Step zero: [docs/GAP_ANALYSIS_PHASE9.md](docs/GAP_ANALYSIS_PHASE9.md).

#### Added
- Direct printing to receipt printers (ESC/POS, 58 and 80 mm) over the network (raw TCP), a
  serial port or a Windows print queue (USB printers), from this PC or through the hub. Receipt,
  return slip, till-close summary, held-bill slip and test page; code page, ₹ and Hindi or
  Malayalam text printed as pictures, cut, copies. The print dialog stays the fallback.
- Settings → Printers and drawer, per PC: printers (find Windows printers and COM ports, test
  page before saving), what prints where, automatic receipt after a sale, the cash drawer.
- Cash drawer: opens for cash sales, and by hand with `sales.open_drawer` or a manager's approval
  (audited).
- The hub's print queue: counters print to printers the hub shares; failed jobs are retried
  after 1, 2 and 5 minutes, then wait for Retry or Cancel. The queue is shown on hub and counters.
- Barcode labels (Stock → Barcode labels): from products or a goods receipt (one per unit, with
  batch and expiry), templates (size, labels across, EAN-13, EAN-8, Code 128 or QR, fields),
  preview, print to a ZPL or TSPL label printer or on A4 sheets. "Print labels" on goods receipts.
- In-store EAN-13 barcodes (prefix 29) for products without one, unique across products and pack
  sizes.
- First start: optional receipt printer setup with a test print.
- Commands: `printers_config_get/save`, `printers_discover`, `printers_test`, `print_document`,
  `drawer_open` (this PC); `printers_shared`, `printers_record_change`, `drawer_authorize`,
  `print_jobs_submit/list/retry/cancel`, `labels_templates_list/save/delete`, `labels_items`,
  `labels_preview`, `products_generate_barcodes` (hub). Error code `PRINTER_ERROR`.
- Tests: byte-level snapshots of receipts, slips and ZPL/TSPL jobs, a unit test per barcode type,
  a fake network printer (Rust and end-to-end), print-queue retries and replays, permissions,
  label templates, in-store barcodes, the 0010 migration on a database in use.

#### Changed
- Saving a product refuses a barcode that another product's variant or pack size already has.
- The till, invoice and return pages print through the configured printer when there is one.

#### Not done
- Printer status read-back (paper out, cover open): failures are connection errors only (A-100).
- Hindi and Malayalam receipts and UI text come with Phase 15 localisation; the picture fallback
  is in place.
- Not tried on real printers here (no hardware): see the manual checks in the phase report.

### Lists, pictures, shortcuts, controls and an Accounts audit (2026-10-21)

No schema change and no migration: product pictures use the existing `products.image_path`.
Accounts findings and tests: [docs/ACCOUNTS_AUDIT.md](docs/ACCOUNTS_AUDIT.md).

#### Added
- Server-side sorting, searching, filtering and paging on every list command (whitelisted sort keys);
  tables show the page, page size (10/25/50/100) and total, and keep their state in the address.
- Product pictures: upload (type and size checked, scaled, re-encoded), replace, remove, served
  through the hub; thumbnails in the product list, search, till cart and dashboard; orphan clean-up.
- Customizable till shortcuts (per PC and user) in Settings and in the till, with conflict and
  reserved-key checks, reset, and live hints in the till.
- shadcn `Select` and `DatePicker` everywhere instead of native controls.
- 23 Accounts audit scenarios and list, picture and shortcut tests.

#### Fixed
- The till's expected cash ignored cash refunds, cash expenses, receipts on account and supplier
  cash payments.
- A customer-and-supplier ("both") party broke the receivables/payables reconciliation and showed a
  negative payable.

### Frontend redesign (2026-10-20)

Frontend only; behaviour unchanged. Details and before/after: [docs/FRONTEND_REDESIGN.md](docs/FRONTEND_REDESIGN.md).

#### Added
- Design tokens for light and dark (surface, raised, status colours, chart ramp, radius, shadow,
  motion, type scale), spacing option (comfortable / compact), `Alert` and `Avatar`.
- Shell: grouped sidebar, breadcrumbs, account menu, search button.
- Dashboard: six KPI cards, 14-day sales chart, top sellers, low stock, quick actions.
- Till: customer chip, offline notice, quick-tender buttons; invoice timeline; balance chips;
  stock badges; column choice on lists; sticky save bar; role permissions as an area-by-action grid;
  trend chart on one-row-per-day reports; branded sign-in.
- `e2e/a11y.spec.ts`: axe scan of the main screens in both themes.

#### Changed
- Status chips use colour by meaning (paid, due, void, low or out of stock).
- All UI primitives are TypeScript; icons are lucide only.

#### Fixed
- Dark theme contrast and low-contrast labels found by axe.

#### Removed
- 29 unused UI primitives and 9 unused dependencies (`hugeicons`, `vaul`, `date-fns`, …).

### Phase 8: testing, packaging and documentation (2026-10-20)

#### Added
- End-to-end test (Playwright, Chrome) for the main billing flow, served with the packaged app's
  Content-Security-Policy; `npm run test:e2e`; a CI job for it.
- Content-Security-Policy in the app (audit S-4), minimum window size, NSIS and MSI installers.
- Start-up database check: a damaged file is kept as a copy and not opened.
- Docs: `USER_GUIDE.md`, `RELEASE.md` (checklist, code signing, auto-update steps),
  `SECURITY_REVIEW.md` (audit S-1 to S-12 status, dependency audit, licenses), and the damaged-file
  recovery steps in the Admin guide.
- CI: dependency audit job and a tag-triggered release workflow (unsigned installers).

#### Fixed (found by the end-to-end test)
- Ctrl+K crashed the app to a blank screen (the palette was missing its `Command` wrapper; added
  in Phase 7 and not exercised in a browser until now).
- A barcode scanner that pressed Enter within ~120 ms of the last digit got "No item matches":
  the till now looks the exact code up directly when the search hasn't run yet.

#### Changed
- Excel export uses `write-excel-file`; `xlsx` (unfixed advisories) is removed: `npm audit` is clean.
- Product name "Inventory and Billing" (the data folder is unchanged).

#### Removed
- The unused v1 screens (`src/pages`) and the v1 client store.

#### Not done
- Code signing, auto-update, a packaged-window check of the CSP and a clean-PC install test need
  the owner's certificate and a PC (docs/RELEASE.md). Invoice-save and app-start benchmarks
  (A-76). A "restore on first start" screen (A-79).

### Phase 7: polish, accessibility and performance (2026-10-20)

#### Added
- Dark theme (it could not be switched on before): follows the system, with a header button to
  cycle system, light and dark. Compact spacing option.
- Ctrl+K palette: jump to any screen you may open, find a product by name, SKU or barcode,
  switch theme or spacing.
- Accessibility: skip link, visible focus outline, reduced-motion support, darker focus ring,
  window title and focus move to the page on every navigation.
- `services/tests/perf.rs`: ignored benchmark on 50,000 products and 150,000 stock movements.

#### Changed
- Stock movement report: 3.1 s to 0.35 s (sums per item first). Product list: 342 ms to 211 ms
  (stock looked up per row of the page, not for every product).
- First-load script 650 kB to 265 kB (vendor code in cached chunks).
- `command.jsx` is now typed TypeScript.

#### Not done
- Billing-screen shortcut review, saved table views and column toggles, an onboarding printer
  test, and benchmarks for invoice save, app start and a year of documents: see ASSUMPTIONS A-75
  and A-76; planned for Phase 8 or as suggestions.

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
