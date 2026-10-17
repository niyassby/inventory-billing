# Inventory & Billing

An offline-first inventory and billing desktop app for small and medium businesses: POS billing,
products and stock, purchases, customers and suppliers, accounting-lite and reports. It runs fully
offline. Multi-counter shops connect their devices to one **hub** PC over the local network
(see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).

> **Status:** under active rebuild. Phase 0 (audit), Phase 1 (tooling, TypeScript, shared
> package) and Phase 2 (v2 database schema and the v1 data migration) are done. The running app
> is still the legacy single-device version. Read [docs/AUDIT.md](docs/AUDIT.md) for its known
> issues before using it with real data.

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

The first run creates the database in `%LOCALAPPDATA%\com.niyas.tauri-app\inventory_app.db`.

## Common commands

| Command | What it does |
|---|---|
| `npm run tauri dev` | Run the desktop app with hot reload |
| `npm run check` | Typecheck, lint, format check and tests with coverage (what CI runs) |
| `npm test` | Unit tests (Vitest) |
| `npm run rust:lint` | `cargo clippy` with warnings as errors |
| `npm run tauri build` | Build the Windows installer |

## Documentation

- [docs/AUDIT.md](docs/AUDIT.md): Phase 0 review of the legacy code
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): offline and multi-user design (LAN hub)
- [docs/DATABASE.md](docs/DATABASE.md): schema, ER diagrams, invariants, v1 migration
- [docs/ASSUMPTIONS.md](docs/ASSUMPTIONS.md): decisions and defaults
- [docs/DEVELOPER.md](docs/DEVELOPER.md): project layout, conventions and workflow
- [CHANGELOG.md](CHANGELOG.md)
