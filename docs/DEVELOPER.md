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
src/
  api/client.ts            typed API client over a Transport (Tauri IPC today, HTTP to the hub later)
  api/errors.ts            ApiError + normalisation of backend errors into stable codes
  api/hooks/*.ts           TanStack Query hooks per domain
  stores/app-store.ts      Zustand: session + UI state (legacy POS cart lives here for now)
  app/                     shell: layout, auth page, RequireAuth guard
  components/ui/           shadcn primitives (TSX; the rest are converted as they are touched)
  pages/**                 legacy pages (.jsx), rebuilt module by module in Phases 5–7
src-tauri/                 Rust backend (restructured into domain/db/repo/services/api in Phases 2–3)
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
