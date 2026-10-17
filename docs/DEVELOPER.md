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
  api/client.ts            typed clients over a Transport: v1 `api` and v2 `createV2Client`
  api/errors.ts            ApiError + normalisation of backend errors into stable codes
  api/hooks/*.ts           TanStack Query hooks per domain
  stores/app-store.ts      Zustand: session + UI state (legacy POS cart lives here for now)
  app/                     shell: layout, auth page, RequireAuth guard
  components/ui/           shadcn primitives (TSX; the rest are converted as they are touched)
  pages/**                 legacy pages (.jsx), rebuilt module by module in Phases 5–7
src-tauri/src/
  api/                     v2 commands (one macro: session, approval, logging) + contract.rs (JSON schema)
  services/                v2 business logic, one module per area; tests/ and testkit.rs
  app.rs, error.rs         AppState (pool, clock, rate limiter), AppError and its codes
  db/                      v2 database: connection settings, migration runner, migrations/*.sql, schema tests
  domain/                  pure logic (money, pricing, numbering, ids); mirrors packages/shared
  legacy_migration/        v1 → v2 data migration (reader, transform, report, end-to-end tests)
  bin/migrate_legacy.rs    command-line front end for the migration
  commands/, repositories/, database/   v1 backend, still serving the screens until Phase 5
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

## Database

The schema and its rules are in [DATABASE.md](DATABASE.md). To change it, add a new migration
file (never edit a released one) and extend `src-tauri/src/db/tests.rs`.

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin migrate-legacy -- --dry-run
```

## Previewing the UI without Tauri

`npm run dev` serves the frontend at http://localhost:1420. Backend calls fail in a plain browser
because there is no Tauri IPC. For quick UI checks you can stub IPC from the browser console
before signing in:

```js
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 0,
  invoke: async (cmd, args) => (cmd === "login" ? { user: {/* … */}, token: "dev" } : []),
};
```

## TypeScript migration status

| Area | Status | Converted in |
|---|---|---|
| `packages/shared`, `src/api`, `src/stores`, `src/lib`, `src/hooks` | ✅ TS | Phase 1 |
| App shell (`main`, `App`, `app/*`, login form) | ✅ TS | Phase 1 |
| shadcn primitives used by TS code (12 files) | ✅ TS | Phase 1 |
| Remaining shadcn primitives, sidebar components | JS | Phase 7 (design system) |
| Pages: inventory, sales/POS, purchases, contacts | JS | Phase 5 (rebuilt) |
| Pages: accounting, reports, settings | JS | Phase 6 (rebuilt) |
| Dashboard | JS | Phase 7 |
