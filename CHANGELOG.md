# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
