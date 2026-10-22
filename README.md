# Inventory & Billing

An offline-first inventory and billing desktop app for small and medium businesses: POS billing,
products and stock, purchases, customers and suppliers, accounting-lite and reports. It runs fully
offline. Multi-counter shops connect their devices to one **hub** PC over the local network
(see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).

> **Status:** under active rebuild. Phase 0 (audit), Phase 1 (tooling, TypeScript, shared
> package), Phase 2 (v2 database schema and the v1 data migration), Phase 3 (v2 backend
> services, permissions and tests), Phase 4 (LAN hub, offline counters, backups) and Phase 5
> (point of sale, sales, products and stock, purchases, customers and suppliers on the new
> backend) and Phase 6 (reports, accounting-lite, settings) and Phase 7 (polish, accessibility,
> performance) are done. Phase 8 (tests, packaging, documentation) is done:
> see [docs/RELEASE.md](docs/RELEASE.md) for what to check before a release.
> Phase 9 (printing and labels: thermal receipts, cash drawer, barcode labels, the hub's print
> queue) is done: see [docs/PHASE9_REPORT.md](docs/PHASE9_REPORT.md).

## Stack

- **Desktop shell / backend:** Tauri 2, Rust, SQLite (rusqlite)
- **Frontend:** React 19, TypeScript (strict), Vite, Tailwind CSS 4, shadcn/ui (Base UI),
  TanStack Query, Zustand
- **Shared logic:** `packages/shared`: money, quantity, tax engine, numbering, Zod schemas

## Quick start

Prerequisites: Node.js 22+, Rust (stable), and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (WebView2 on Windows).

```bash
npm install
```

```bash
npm run tauri dev
```

The first run creates the database in `%LOCALAPPDATA%\com.niyas.tauri-app\inventory_v2.db`.

## Common commands

| Command | What it does |
|---|---|
| `npm run tauri dev` | Run the desktop app with hot reload |
| `npm run check` | Typecheck, lint, format check and tests with coverage (what CI runs) |
| `npm test` | Unit tests (Vitest) |
| `npm run rust:lint` | `cargo clippy` with warnings as errors |
| `npm run test:e2e` | End-to-end test in Chrome against the real backend |
| `npm run tauri build` | Build the Windows installers (NSIS and MSI) |

## Documentation

- [docs/USER_GUIDE.md](docs/USER_GUIDE.md): day-to-day use, for cashiers and owners
- [docs/ADMIN_GUIDE.md](docs/ADMIN_GUIDE.md): network setup, offline counters, backups and restore
- [docs/AUDIT.md](docs/AUDIT.md): Phase 0 review of the legacy code
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): offline and multi-user design (LAN hub)
- [docs/DATABASE.md](docs/DATABASE.md): schema, ER diagrams, invariants, v1 migration
- [docs/ASSUMPTIONS.md](docs/ASSUMPTIONS.md): decisions and defaults
- [docs/DEVELOPER.md](docs/DEVELOPER.md): project layout, conventions and workflow
- [CHANGELOG.md](CHANGELOG.md)
- [docs/FRONTEND_REDESIGN.md](docs/FRONTEND_REDESIGN.md): the design system and the redesign report
- [docs/RELEASE.md](docs/RELEASE.md): build, release checklist, code signing, auto-update
- [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md): security review and dependency audit
