# Developer Guide

## Project layout

```
packages/shared/           @inventory/shared: pure TS, no React and no Tauri
  src/money.ts             Money = integer minor units (paise); parse/format/arithmetic
  src/qty.ts               Qty = integer thousandths of a unit
  src/rounding.ts          the one place rounding happens (bigint), exact allocation
  src/tax.ts               document pricing: discounts → tax (inclusive/exclusive, GST split) → round-off
  src/numbering.ts         financial year, document number templates, business date
  src/gstin.ts             GSTIN validation
  src/schemas.ts           Zod form schemas (string input → domain values)
  src/constants.ts         roles, permissions, document/movement types, error codes
  src/types/legacy.ts      types of the current (v1) Tauri commands
  src/generated/           api-schema.json (from Rust) and api.ts (npm run gen:api): do not edit
src/
  App.tsx                  gate (setup → sign-in → password change) and all routes (lazy-loaded)
  app/                     shell (sidebar, top bar, F2), navigation config (permission-aware)
  api/v2.ts                `call(command, input)`: typed, asks for manager approval and retries
  api/transport.ts         Tauri IPC in the app, the HTTP dev bridge in a plain browser
  api/client.ts, errors.ts typed clients over a Transport; ApiError with stable codes
  api/connection.ts        what the top bar shows about the hub connection (hooks/use-connection.ts)
  lib/queries.ts           useCommand / useList (queries) and useAction (mutations + toasts)
  lib/format.ts            money, quantity, rate and date display/parsing (exact, no floats)
  lib/business.ts          business settings (currency, locale) for formatting
  stores/                  session (token in sessionStorage), approval prompt; legacy app-store (v1 pages)
  components/app/          page frame, DataTable, inputs (money/qty/rate), pickers, dialogs, line editor
  components/ui/           shadcn primitives (TSX for everything the v2 screens use)
  features/<module>/       screens per module: auth, dashboard, pos, sales, inventory, parties, purchases
  pages/**                 (removed in Phase 8: the v1 screens)
src-tauri/src/
  api/                     v2 commands (one macro: IPC, LAN dispatch, contract) + contract.rs (JSON schema)
  api/local.rs             commands about this PC (role, pairing, outbox); never forwarded to the hub
  hub/                     LAN HTTPS server (axum + rustls), TLS identity, mDNS discovery
  client/                  counter mode: pinned HTTPS client, offline handlers, client.db, sync loop
  jobs.rs                  background scheduler (backup, low stock, integrity, cleanup)
  node.rs, secrets.rs      this PC's role (device.json) and the OS keychain
  services/                v2 business logic, one module per area; tests/ and testkit.rs
  app.rs, error.rs         AppState (pool, clock, rate limiter), AppError and its codes
  db/                      v2 database: connection settings, migration runner, migrations/*.sql, schema tests
  domain/                  pure logic (money, pricing, numbering, ids); mirrors packages/shared
  legacy_migration/        v1 → v2 data migration (reader, transform, report, end-to-end tests)
  bin/migrate_legacy.rs    command-line front end for the migration
  bin/dev_server.rs        development only: the v2 commands over HTTP on 127.0.0.1:1430
  commands/, repositories/, database/   v1 backend (unused by the screens since Phase 5; removed in Phase 8)
packages/shared/test-vectors/  JSON vectors both the TS and Rust money code must pass
docs/                      audit, architecture, assumptions, this guide
```

## Rules

- **TypeScript is strict** (`strict`, `noUncheckedIndexedAccess`, no `any`). Use `unknown` and
  narrow it. `@ts-ignore` / `@ts-expect-error` need a written reason (lint enforces this).
- **All new files are `.ts` / `.tsx`.** Legacy `.jsx` pages are only converted when their module
  is rebuilt. Don't add new `.jsx` files.
- **Money is never a float.** Use `Money` and `Qty` from `@inventory/shared` and do arithmetic
  with its helpers. Display with `formatMoney`, and edit with `toMajorString` / `parseMoney`.
- **No business rules in components.** Pricing, tax and numbering live in `packages/shared`
  (mirrored in Rust `domain/` from Phase 3). The backend always recomputes and validates.
- **Talk to the backend only through `api.call(...)` or a hook in `src/api/hooks`,** never through
  `invoke` directly in new code. Handle errors by `ApiError.code`, never by message text.
- **Dates:** use `businessDate(instant, timeZone)` for "today" and document dates, never
  `toISOString().slice(0, 10)` (that is UTC).

## Workflow

```bash
npm run check
```

That runs typecheck, ESLint, the Prettier check and Vitest with coverage. Coverage below 80% on
`packages/shared` and `src/api` fails.

```bash
npm run rust:fmt && npm run rust:lint && npm run rust:test
```

- A pre-commit hook (husky + lint-staged) runs ESLint `--fix`, Prettier and rustfmt on staged files.
- CI (`.github/workflows/ci.yml`) runs the same checks on every push and PR. The frontend runs on
  Ubuntu and Rust runs on Windows, the primary target.
- Commits follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`).
  Every commit should build.

## Backend (v2)

A command is declared once in `src-tauri/src/api/mod.rs`:

```rust
authed products_save(catalog::ProductInput) -> catalog::ProductDetail = catalog::save_product;
```

That generates the Tauri command (session check, optional manager approval, blocking thread,
logging), registers it and adds it to the contract. Services take a `Ctx` (connection, actor,
approver, clock) and must:

- check a permission first (`ctx.require(...)`, or `ctx.approve(...)` when a manager may allow it);
- do every write inside `services::write` (one `BEGIN IMMEDIATE` transaction);
- post stock, party ledger and journal through `stock_posting`, `party_ledger` and `journal`
  (never update cached balances directly) and write an audit entry;
- return `AppError` with a code from `error.rs`; the UI branches on the code.

Service tests use `services::testkit::Kit` (temporary database, fixed clock, a set-up business)
and end with `kit.assert_books_balance()`.

After changing a command or a DTO, regenerate the contract and the TS types and commit both:

```bash
UPDATE_CONTRACT=1 cargo test --manifest-path src-tauri/Cargo.toml --lib contract
```

```bash
npm run gen:api
```

In the UI, call v2 through the generated types:

```ts
const v2 = createV2Client(tauriTransport, () => session.token);
const page = await v2.call("products_list", { search: "rice", page: 1 });
```

## LAN hub and counters (v2)

The same command list serves the window (IPC), counters (`POST /v1/call/{command}` on the hub,
`api::dispatch`) and, on a counter, the proxy in `lib.rs::route` that forwards IPC calls to the
hub. A command therefore needs no network code. If it should work on a counter while the hub is
offline, add it to `client::offline` (and keep it to read-only or fully self-checking work).

Tests run hubs and counters in-process over real HTTPS on 127.0.0.1
(`services/tests/network.rs`); `server.kill()` simulates switching the hub off. To try it by
hand, make one PC the hub (Settings → Network once the screens exist, or write
`{"mode":"hub","deviceName":"Hub","hub":null}` to `device.json` next to the database and
restart), then pair a second PC (or a second Windows user) through `device_probe_hub` and
`device_pair`.

## Database

The schema and its rules are in [DATABASE.md](DATABASE.md). To change it, add a new migration
file (never edit a released one) and extend `src-tauri/src/db/tests.rs`.

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin migrate-legacy -- --dry-run
```

## Running the screens in a browser

The screens talk to the backend through a transport: Tauri IPC inside the app, or the
development bridge in an ordinary browser (same commands, same checks, real database):

```bash
npm run dev:bridge
```

```bash
npm run dev
```

Then open http://localhost:1420. `dev:bridge` keeps its database in `.dev-data/` and, on an empty
folder, creates a demo shop: `demo@example.test` / `Demo-shop-2026` (test data only). Delete
`.dev-data/` to start over. The first build of the bridge takes a while (it compiles the whole
backend in debug mode).

## Writing a screen

- Read with `useCommand(name, input)` (or `useList` for paged lists); write with
  `useAction(name, { invalidates, success })` or `call(name, input)` for multi-step flows. Never
  call `invoke` directly.
- Every write that creates a document sends a `requestId()` as its `idempotencyKey`, created once
  per screen, so a double click or a retry never makes two documents.
- Money is minor units, quantities thousandths, rates ppm. Use `MoneyInput`, `QtyInput`,
  `RateInput` and the `show*` helpers; never `parseFloat`.
- `APPROVAL_REQUIRED` needs no code: `call` opens the manager-approval dialog and retries.
- Totals shown before saving come from the backend (`sales_preview`); the screen never computes
  tax.
- Lists are sorted, filtered and paged in the database. Backend: take `PageQuery` and build the
  `ORDER BY` with `page.order_by(&KEYS, default, tiebreak)`, where `KEYS` maps a public key to a SQL
  expression; never interpolate a request value. Frontend: `useTableState(key, defaults)` keeps page,
  size, sort and filters in the address; give columns `meta.sortKey` and pass `sortProps` as the
  table's `sort` prop.
- Use `SelectField` / `DatePicker` (`components/ui`), never a native `<select>` or
  `<input type="date">`. Dates are `YYYY-MM-DD` strings.
- Product pictures live in `services/images.rs` and are fetched with `products_image_get`
  (base64), so they work from a counter; show them with `ProductImage`.
- Business rules belong in the backend or in `packages/shared`, never in a component. Pure
  screen logic (like the POS cart in `features/pos/cart.ts`) lives in a `.ts` file with tests.

## TypeScript migration status

| Area | Status | Converted in |
|---|---|---|
| `packages/shared`, `src/api`, `src/stores`, `src/lib`, `src/hooks` | ✅ TS | Phase 1 |
| App shell, sign-in, setup | ✅ TS (rewritten on v2) | Phase 5 |
| shadcn primitives used by the v2 screens (25 files) | ✅ TS | Phases 1 and 5 |
| Inventory, sales/POS, purchases, customers and suppliers, payments, dashboard | ✅ TS (rebuilt on v2) | Phase 5 |
| Reports, expenses, accounts, day close, settings, users, audit | ✅ TS (new) | Phase 6 |
| Leads, barcode labels | JS (v1, not routed) | not rebuilt (A-14, A-71) |
| Remaining shadcn primitives | JS | Phase 7 (design system) |

## Performance benchmark

`src-tauri/src/services/tests/perf.rs` loads 50,000 products and 150,000 stock movements with SQL
and times search, barcode scan, the product list and the stock reports against the targets in the
Master Prompt (§12). It is ignored in normal runs; in a release build:

```bash
cargo test --release --lib perf -- --ignored --nocapture
```

## End-to-end tests

`npm run test:e2e` starts the dev bridge on port 1431 with a fresh demo shop (`.e2e-data/`), builds
the web app into `dist-e2e/` and serves it with the packaged app's Content-Security-Policy, then
drives installed Chrome through `e2e/*.spec.ts`. The first run compiles the bridge (about 15
minutes in a debug build); later runs take about a minute. `e2e/global-setup.ts` creates the
item the test sells through the same API the screens use.

## UI design system

Tokens live in `src/index.css`; components use semantic classes (`bg-card`, `text-muted-foreground`,
`bg-success-soft`, `shadow-sm`, `rounded-lg`, `num`, `text-page-title`) and never a raw palette
colour. Status colours mean something (success = paid, warning = due or low stock, danger = void or
out of stock, info = informational): do not use them for decoration. `/dev/ui` (dev builds only)
shows every primitive in both themes. Icons: lucide only. Add primitives with `npx shadcn add`
(the config is TSX + lucide) and convert nothing to JS. See [FRONTEND_REDESIGN.md](FRONTEND_REDESIGN.md).
