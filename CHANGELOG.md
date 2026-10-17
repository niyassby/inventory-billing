# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
