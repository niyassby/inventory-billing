# Master Prompt: Review & Rebuild Inventory + Billing App

*Paste this whole document into your AI coding agent (Claude Code, Cursor, etc.) with your project folder open.*

---

## 1. Role and mission

You are a senior full-stack architect and engineer. You have access to my existing **inventory management app** (React, JavaScript). Your mission is to **review it thoroughly, then transform it into a professional, production-ready inventory and billing application for small and medium businesses (SMBs) that works fully offline and supports multiple users.**

Work in phases. Finish and report each phase before starting the next. Make reasonable assumptions instead of asking me questions, and list every assumption you make in `docs/ASSUMPTIONS.md`.

## 2. Goals

1. Review the current code and report what is wrong, slow, insecure or missing.
2. Migrate the whole codebase from JavaScript to **strict TypeScript**.
3. Redesign the **database schema** and **system settings**.
4. Improve the **backend**: restructure files and rewrite functions where needed for correctness, speed and maintainability.
5. Redesign the **frontend** so it looks modern, clean and attractive, and is fast for daily counter use.
6. Make the app **offline-first** and solve **multi-user operation without internet**.
7. Make it **production-ready**: secure, tested, backed up, packaged and documented.

## 3. Phase 0: Audit (do this first, change no code)

Read the entire codebase and produce `docs/AUDIT.md` covering:

- Current stack, folder structure, data flow and how data is stored today.
- List of all features and modules, and which are complete, partial or broken.
- Bugs, logic errors, race conditions and edge cases (negative stock, rounding, duplicate invoice numbers, deleted items on old invoices).
- Performance problems (unnecessary re-renders, N+1 queries, missing indexes, large lists without virtualization).
- Security problems (plain-text secrets, weak auth, missing validation, no role checks, injection risks).
- Code quality (duplicated code, dead code, giant components, missing types, no tests).
- UX problems (too many clicks, no keyboard shortcuts, poor mobile or small-screen layout, no empty or error states).
- A prioritized fix list: **Critical / High / Medium / Low**, with effort estimates.
- A short recommended architecture (see section 5) and a migration plan that avoids losing existing data.

Stop and summarize the audit before continuing.

## 4. Target product

A professional **inventory + billing** app for shops, wholesalers, distributors and small service businesses.

**Core modules**

- **Dashboard:** today's sales, profit, low-stock alerts, expiring items, top sellers, receivables and payables, cash and bank summary.
- **Products and inventory:** categories, brands, units and unit conversions, variants, SKU and barcode, batch and expiry (optional), multiple warehouses or locations, reorder levels, stock adjustments, stock transfers, stock-take (physical count) with variance report.
- **Billing and sales:** fast POS-style billing screen, barcode scan, quick item search, discounts (item and bill level), configurable tax engine (GST, VAT or none), multiple payment modes and split payments, hold and resume bills, quotations, sales orders, delivery challans, credit notes and sales returns, invoice PDF and thermal receipt (58mm and 80mm) printing, invoice numbering series per branch and financial year.
- **Purchases:** suppliers, purchase orders, goods received, purchase bills, purchase returns, landed cost.
- **Parties (CRM):** customers and suppliers, credit limits, outstanding balances, ledgers, payment receipts and payment-outs, statements.
- **Accounting-lite:** expenses, cash book, bank book, day close, profit and loss, tax summary.
- **Reports:** sales, purchases, stock valuation (FIFO or weighted average), stock movement, item profit, party ledger, tax reports, day book. Export to PDF, Excel and CSV.
- **Settings:** see section 8.
- **Users and roles:** see section 9.

## 5. Architecture decision: offline + multiple users

This is the most important design question. Evaluate both options below in `docs/ARCHITECTURE.md`, pick one, justify it, and implement it.

### Option A: LAN hub (recommended default for most SMBs)

- One computer in the shop is the **hub** and holds the single source-of-truth database (SQLite in WAL mode, or PostgreSQL if justified).
- Other devices (billing counters, manager's laptop, phones or tablets) connect to the hub **over the local network (Wi-Fi or LAN), with no internet needed.**
- The hub runs a local API server. Devices find it by a configured IP or hostname, with mDNS discovery as a bonus.
- All writes go through the hub inside transactions, so stock and invoice numbers can never conflict.
- If a client loses connection to the hub, it queues writes locally (outbox) and shows a clear "hub offline" state; sensitive actions that need live stock (for example selling the last unit) follow a configurable policy.


## 6. Tech stack and TypeScript migration

Detect my current stack in Phase 0 and keep what is good. Unless the audit says otherwise, target:

- **Frontend:** React + TypeScript (strict), Vite or Next.js (whichever matches the existing app), Tailwind CSS with a component library such as shadcn/ui, TanStack Query and TanStack Table (with virtualization), React Hook Form + Zod, Zustand for UI state.
- **Desktop shell :** Tauri (Rust).
- **Backend:** Rust commands, with a clean layered structure, No business logic in UI components or route handlers.
- **Validation:** one shared Zod schema package used by both frontend and backend.
- **Shared types:** a `packages/shared` module for types, schemas, constants and money and tax utilities.

**Migration rules**

- Convert file by file; keep the app running after every step.
- `strict: true`, `noUncheckedIndexedAccess`, no `any` (use `unknown` and narrow), no `@ts-ignore` without a written reason.
- Replace prop-drilling and ad-hoc fetches with typed hooks and a typed API client.
- ESLint + Prettier + a pre-commit hook; CI runs typecheck, lint and tests.

## 7. Database redesign

Produce `docs/DATABASE.md` with an ER diagram (Mermaid) .

**Rules**

- Money stored as **integers in the smallest currency unit** (paise or cents), never floats. Quantities use a fixed-precision decimal strategy. Tax and rounding rules are centralized.
- UUIDv7 primary keys; `created_at`, `updated_at`, `created_by`, `updated_by`, `deleted_at` on every business table.
- Foreign keys enforced, `CHECK` constraints, unique indexes (SKU, barcode, invoice number per series), and indexes designed from real query patterns.
- **Stock ledger** (`stock_movements`) is the source of truth; current stock is a maintained, verifiable cache that can be rebuilt from the ledger.
- **Finalized invoices are immutable.** They store a snapshot of item name, price, tax rate and customer details at the time of sale, so later edits never change history.
- Append-only **audit log** for sensitive actions (price changes, discounts, deletes, stock adjustments, logins, settings changes, voids).
- Suggested tables (refine as needed): `users, roles, permissions, devices, branches, warehouses, products, product_variants, categories, brands, units, unit_conversions, batches, stock_movements, stock_counts, parties, price_lists, tax_rates, sales_invoices, sales_invoice_items, sales_returns, quotations, purchase_orders, purchase_bills, purchase_bill_items, purchase_returns, payments, payment_allocations, expenses, cash_registers, day_closes, number_series, settings, audit_log, sync_outbox, sync_log, schema_migrations`.


## 8. System settings (design properly)

A typed, validated, versioned settings system with sections:

- **Business profile:** name, logo, address, tax IDs, bank details, financial year start, currency and number formats, timezone, language.
- **Tax and pricing:** tax rates and groups, inclusive vs exclusive pricing, rounding rules, discount limits per role.
- **Invoice and print:** templates, paper sizes, thermal printer setup, header and footer text, terms, QR code, number series and prefixes.
- **Inventory:** valuation method, negative-stock policy, low-stock thresholds, batch and expiry toggles, default warehouse.
- **Network and sync:** hub address, device name and role, sync interval, offline policy.
- **Security:** password policy, session timeout, auto-lock, PIN for quick switch of users, backup encryption.
- **Backup:** schedule, location, retention, restore.
- **Appearance:** light and dark theme, density, language.
- **Hardware:** barcode scanner, cash drawer, printers, label printing.

Settings changes must be audit-logged, validated with Zod and applied without restarting wherever possible.

## 9. Users, roles and security

- Multi-user login with **roles and fine-grained permissions** (Owner, Manager, Cashier, Stock Keeper, Accountant, custom roles). Enforce permissions in the **backend**, not only in the UI.
- Password hashing with **Argon2id**; account lockout and rate limiting; optional PIN for fast cashier switching.
- Local data encryption at rest where feasible (for example SQLCipher) and encrypted backups.
- Input validation and parameterized queries everywhere; no secrets in the repo; sanitized logs.
- Manager approval flow for sensitive actions (large discounts, voids, price overrides, back-dated entries).
- Licensing is out of scope unless I ask.

## 10. Backend improvements

- Restructure folders into clear modules.
- All multi-step operations (create invoice, receive stock, return, transfer) run inside **database transactions** with idempotency keys.
- Replace slow or repeated queries with indexed queries, pagination, and aggregation done in SQL.
- Centralized error handling with typed error codes, structured logging with rotation, and a diagnostics export for support.
- Background jobs for backups, sync, low-stock checks and report caching.
- Document every endpoint or command (OpenAPI or a typed contract).

## 11. Frontend redesign

Make it look like a polished commercial product:

- A cohesive design system: color tokens, typography scale, spacing, consistent icons, light and dark themes.
- **Billing screen built for speed:** keyboard-first (F-key shortcuts, barcode focus, arrow-key row navigation), large totals, clear payment dialog, instant search, recent and favorite items.
- Clean sidebar navigation, global search (Ctrl+K), breadcrumbs, and a dashboard with meaningful charts.
- Data tables with sorting, filtering, column toggles, saved views, virtualization and export.
- Skeleton loaders, empty states, inline validation, undo toasts and friendly error messages.
- Responsive layout (desktop first, usable on tablet), accessible (WCAG AA contrast, focus states, labels, keyboard support).
- Visible **online / hub-connected / offline / syncing** status at all times.
- Onboarding wizard for first run: business details, tax, first products, first user, printer test.

## 12. Performance targets

- App start under 3 seconds on a low-end office PC.
- Product search results in under 100 ms with 50,000 products.
- Invoice save in under 300 ms.
- Lists of 100,000 rows stay smooth (virtualization and pagination).
- Report queries on one year of data in under 3 seconds with proper indexes.

Add a seed-data script that generates large realistic datasets, and benchmark against these targets.

## 13. Testing and quality

- Unit tests for money, tax, rounding, stock valuation, numbering and sync-merge logic.
- Integration tests for the full invoice, return, purchase and stock-count flows against a real test database.
- **Offline and concurrency tests:** two devices selling the same item while disconnected, hub restart mid-transaction, duplicate retry, clock skew.
- End-to-end tests (Playwright) for the main billing flow.
- Minimum 80% coverage on business-logic modules. CI must pass before any phase counts as done.

## 14. Production readiness checklist

- Installer or packaged builds for Windows (and macOS or Linux if relevant), with auto-update support and code signing notes.
- First-run setup, crash reporting (local log bundle), safe shutdown and data-corruption recovery.
- Backup and restore tested end to end; restore drill documented.
- Environment-based config, no debug code or test data in production builds.
- Documentation: `README.md`, `docs/USER_GUIDE.md` (non-technical, with screenshots placeholders), `docs/ADMIN_GUIDE.md` (network setup, backups, troubleshooting), `docs/DEVELOPER.md`, `CHANGELOG.md`.
- Dependency audit, license check and a security review summary.

## 15. Delivery plan

Work in this order, committing after each step with clear messages:

1. **Phase 0:** Audit report and architecture proposal (no code changes).
2. **Phase 1:** Tooling, TypeScript setup, shared types, new project structure.
3. **Phase 2:** New database schema, migrations, data migration script.
4. **Phase 3:** Backend services and permissions, then tests.
5. **Phase 4:** Offline and multi-user layer (hub or sync, outbox, backup).
6. **Phase 5:** Core modules: products and inventory, billing, purchases, parties.
7. **Phase 6:** Reports, accounting-lite, settings.
8. **Phase 7:** UI redesign polish, accessibility, performance pass.
9. **Phase 8:** Testing, packaging, documentation, production checklist.

After each phase give me: what changed, what you tested, known issues, and what comes next.

## 16. Working rules

- Never delete or overwrite my existing data or code without a backup and a migration path.
- Prefer simple, boring, reliable solutions over clever ones; this software handles real money and stock.
- Do not add features outside this document without listing them as suggestions first.
- If something is ambiguous, choose the safest sensible default, record it in `docs/ASSUMPTIONS.md`, and continue.
- Keep every commit buildable and every phase demonstrable.

**Start now with Phase 0: read the whole project and write `docs/AUDIT.md` and `docs/ARCHITECTURE.md`.**
