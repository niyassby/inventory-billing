# Gap analysis before Phase 9 (Step zero)

*Written 2026-10-22 against commit `4158b37` (plus the uncommitted title-bar work in the working
tree, which this analysis does not touch). Method: read README, CHANGELOG, Master Prompt and every
doc in `docs/`; walked `src/`, `src-tauri/src/`, `packages/shared/` and the nine migrations;
checked the API contract (`packages/shared/src/generated/api-schema.json`: 131 commands).*

**Status key.** **Missing**: nothing in code beyond, at most, an unused schema column.
**Partial**: some of it exists and works; the rest is listed. **Done**: exists and needs no work
beyond a check.

## Summary

| Phase | Items | Done | Partial | Missing | Notes |
|---|---|---|---|---|---|
| 9 Printing and labels | 6 | 0 | 0 | 6 | Only HTML printing through the system dialog exists. |
| 10 Inventory depth | 5 | 0 | 2 | 3 | Batch schema and capture on purchases exist; price lists and FIFO are schema-only or setting-only. Units are mostly done. |
| 11 Accounting depth | 6 | 0 | 1 | 5 | No manual journals, lock, bank reconciliation or audit export. |
| 12 Data in and out | 6 | 0 | 1 | 5 | Damaged-file detection exists; the recovery screen does not. No import at all. |
| 13 Multi-device | 7 | 0 | 1 | 6 | The hub's HTTP API and an HTTP transport exist; no static UI serving. |
| 14 Customer features | 7 | 0 | 3 | 4 | `upi` payment mode and receivables data exist; leads are data only. |
| 15 Release hardening | 10 | 0 | 4 | 6 | Benchmarks, axe scan, security review and Linux frontend CI are partial. |

Nothing in sections 5–11 of the brief is already fully done, so no phase is skipped. Two items
shrink (units, duplicate-barcode detector) and several reuse more than the brief assumes (see
"Decision" columns).

## Facts the brief should know

1. **Migrations:** the latest is `0009_hub_sync_backup.sql`; the next free number is **0010**.
   `docs/DATABASE.md` still says "Schema version 6" in its header: stale, to fix in Phase 9's doc
   update.
2. **Assumptions:** the last is A-95; the next free number is **A-96** (as the brief says).
3. **Permissions today** (migrations 0006, 0007, 0009): `accounting.manage/view`,
   `approvals.grant`, `audit.view`, `backup.manage`, `dashboard.view`, `day_close.perform`,
   `devices.manage`, `expenses.manage`, `parties.manage/view`, `payments.pay/receive`,
   `products.manage/view/price_change`, `purchases.manage/receive/view`, `quotations.manage`,
   `reports.export/view`, `sales.backdate/create/credit_override/discount/discount_override/
   price_override/return/view/void`, `settings.manage/view`, `stock.adjust/count/transfer/view`,
   `sync.review`, `users.manage`. None of the new ones the brief names exist yet
   (`journal.approve`, `period.override`, `audit.export`).
4. **A settings toggle that does nothing:** `invoice.show_qr` (`src-tauri/src/services/settings.rs:159`,
   shown in `src/features/settings/settings-forms.tsx:331`) is saved, but no print template draws a
   QR (`src/features/sales/print.ts`). Phase 14's UPI QR should make it real.
5. **Brief inconsistencies:** the title says "Phases 9–16" but the map stops at 15. Leads (CRM)
   appear in the Phase 14 map and tests but not in its scope list. Report grouping, drill-down and
   saved views appear in both Phase 11 item 5 and the "fold into Phase 12" note. See the open
   questions at the end.

## Phase 9: Printing and labels

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 9.1 | ESC/POS printing (USB, serial, TCP 9100; 58/80 mm), templates (receipt, return slip, till close, held-bill slip, test page), browser fallback | **Missing** | Printing is HTML through the system dialog only: `src/features/sales/print.ts` (`receiptHtml`, `invoiceHtml`, `documentHtml`, `salesDocHtml`, `printHtml`); A-60. No ESC/POS code or printer crate in `src-tauri/Cargo.toml`. There is no HTML return slip, till-close or held-bill print either. | Build in full. New Rust module `printing/` (byte builder + transports). "USB" on Windows = the printer's Windows queue in RAW mode (works with the vendor's USB driver), serial via a serial crate, network via raw TCP. Keep `print.ts` as the fallback. Add HTML versions of the new templates for the fallback. |
| 9.2 | Per-PC, per-printer configuration and document routing | **Missing** | Only a business-wide `invoice.paper` (`settings.rs:154-160`). No printer table or per-PC store. | Build. Printer profiles are per PC: keep them in the device-local config next to `device.json` (not the shared DB), since the hub must not dictate another PC's ports. Hub printers are also registered on the hub for the print queue (9.6). |
| 9.3 | Cash drawer kick (auto on cash, manual button) | **Missing** | No drawer code; "cash drawer" only names cash accounts (`services/accounting.rs:80`). | Build (ESC/POS `ESC p` pulse through the receipt printer). Manual button needs a new permission (proposed `till.open_drawer`), audited. |
| 9.4 | Barcode labels (templates, products or goods receipt, preview, ZPL/TSPL/A4; EAN-13, EAN-8, Code 128, QR) and generate-internal-barcode | **Missing** | Not rebuilt from v1 (CHANGELOG Phase 6 notes, A-14/A-71/A-58). No barcode encoder in either package. Barcode uniqueness: per table only — `ux_product_variants_barcode` and `ux_unit_conversions_barcode` (`0002_catalog_stock.sql`), duplicate check within one product (`services/catalog.rs:550-577`). I found **no check across variants and pack sizes**. | Build in full. Barcode encoders (EAN check digit, Code 128, QR) in Rust with unit tests per type; label preview rendered from the same layout. Internal barcodes: EAN-13 in the in-store prefix range (20–29), checked against **both** tables. |
| 9.5 | Printer test in first start and in Settings, clear error per failure | **Missing** | CHANGELOG Phase 7/8 "Not done": onboarding printer test. `src/features/auth/setup.tsx` has none. | Build. Error codes per failure (not found, refused, timeout, paper/offline status when the printer reports it). |
| 9.6 | Hub print queue (counters print to the hub's printer), retry, visible status; never block a sale; reprint | **Missing** | Hub routes are `/v1/health`, `/v1/pair`, `/v1/call/:command`, `/v1/sync/*` only (`src-tauri/src/hub/mod.rs:227-232`). | Build. Queue table on the hub (migration 0010), jobs submitted through `api::dispatch` like any command, worker in `jobs.rs`. Sale save and printing stay separate calls, so a print failure can't fail a sale; the till shows "Reprint". |

## Phase 10: Inventory depth

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 10.1 | Batch and expiry everywhere, FEFO on the till with override | **Partial** | **Schema complete:** `batches` (`0002:164`), `batch_id` on every line table and on `stock_movements`; `stock_levels` keyed by `batch_key` (`0002:214-223`). **Product flag:** `track_batches`/`track_expiry` (`catalog.rs:406-408`); the editor ties expiry to batches (`product-editor.tsx:179`). **Capture works on goods receipt and bill:** `batch_for` finds or creates the batch with expiry (`purchases.rs:127-168`), UI column `purchase-pages.tsx:585-594`. **Backend accepts a batch** on sales, adjustments and transfers (`sales.rs:55,124,209`, `stock.rs:100,306`) and checks per-batch stock (`stock_posting.rs:42-51`); returns copy the invoice line's batch (`sales.rs:1867`). **But:** the till and documents always send `batchId: null` (`pos/cart.ts:129`, `sales/document-pages.tsx:198`); opening stock has no batch (`catalog.rs:844`); no FEFO; no batch on counts UI, prints or labels. Offline: `AllowRestricted` already excludes batch items (`settings.rs:181`). | Shrink to the missing parts: FEFO allocation in the service (split a line across batches when needed), batch picker with override, opening-stock and adjustment batch capture, count lines by batch, batch/expiry on prints and labels. No new tables for batches. |
| 10.2 | Expiry report, dashboard card, hourly notices, block/warn selling expired, write-off | **Missing** | Only the partial index `ix_batches_expiry`. `jobs.rs` has `low_stock`, `integrity`, `cleanup` and backups; no expiry job. | Build. New `inventory.expired_stock_policy` setting (default `block`), expiry job beside `low_stock`, write-off = a stock adjustment with a reason. |
| 10.3 | Price lists (screens, assignment, slabs, effective dates, bulk update) in the shared engine | **Missing** (schema only) | `price_lists`, `price_list_items` with `unit_id` and `min_qty` slabs (`0002:132-162`), `parties.price_list_id` and `sales_invoices.price_list_id` (`0003:18,98`). No service, command, screen or pricing use (`domain/pricing.rs`, `packages/shared/src/tax.ts` have no price list). **No effective-date columns.** | Build. Migration adds `valid_from`/`valid_to`. Price resolution (list → slab → unit → fallback to variant price) goes into `domain::pricing` **and** `packages/shared`, with new golden vectors in `test-vectors/pricing.json`. |
| 10.4 | FIFO valuation, layers, switch routine with dry run, reports | **Missing** | `inventory.valuation_method` accepts `fifo` but behaves as weighted average (`settings.rs:113-122,367`; A-41; CHANGELOG Phase 3 note). `domain/` has no `valuation.rs` (ARCHITECTURE §4 planned one). Cost lives in `product_variants.cost_price` and `stock_movements.unit_cost/value`. | Build. Layer and layer-consumption tables in a migration; `domain::valuation` pure functions with golden cases; switch routine audited, with a dry-run report, owner only. |
| 10.5 | Unit conversions and pack sizes; base-unit display | **Partial (mostly done)** | Per-product pack sizes with their own barcode: `catalog.rs:415,671-685,882`, editor `product-editor.tsx:543-551`. Selling in a pack unit: `sales.rs:195-203`; scan by pack barcode: `catalog.rs:1339`. Schema and the sale lookup also allow global conversions (`product_id IS NULL`, `0002:111`), but **no screen edits them**. | Shrink: a small global-conversions screen under Masters, plus a test that stock is always stored and shown in the base unit. |

## Phase 11: Accounting depth and reports

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 11.1 | Reviewed manual journals (draft → approve → post, reversing only, attachments, restricted accounts) | **Missing** | `services/journal.rs` posts only from documents (`post`, `reverse_source`); `journal_entries.status` is `posted`/`reversed` only (`0005:31-41`); A-71 says manual journals were deliberately not offered. | Build. Separate draft table (posted entries stay append-only), `journal.manage` and `journal.approve` permissions, `source_type = 'manual'`. Attachments reuse the picture store's folder pattern (files beside the DB, path in a table). |
| 11.2 | Closed-day lock, `period.override`, owner reopen, financial-year close | **Missing** | Day close is "a record and checkpoint, not a lock" (`services/accounting.rs:280`, A-70). Back-dating is controlled only by `sales.backdate`. No FY close. | Build. One lock check in a shared helper that every document-creating service calls. FY close = closing entry to retained earnings + opening balances carried forward. |
| 11.3 | Bank reconciliation (CSV import with saved mapping, auto/manual match, statement, history), post-dated cheques | **Missing** | `payments` has mode `cheque` and `reference_no` (`0005:143,148`), no cheque date or cleared date. No statement tables. | Build. Shares the CSV reader and column-mapping UI with Phase 12 import; build that component here first. |
| 11.4 | Audit export (CSV/Excel, same filters, `audit.export`) | **Missing** | `src/features/settings/audit-page.tsx` has no export; A-73. CSV/Excel writers exist (`src/lib/export.ts`, `write-excel-file`). | Build (small). Server returns the filtered rows; reuse `export.ts`. |
| 11.5 | Reports: group-by, drill-down, saved views per user, scheduled export | **Missing** | `reports_run` returns one flat table per report (A-67); `src/features/reports/reports.tsx` has no drill-down or saved views. | Build, keeping one `reports_run` and the 5,000-row guard. Saved views need a per-user store: use the same table as Phase 13.3 preferences, created here. |
| 11.6 | Day-close polish (till differences with reasons, approval above a threshold, print) | **Partial** | Till close records counted cash, difference and free-text notes (`services/payments.rs:560-564`; `accounting.rs:163`). No reason codes, no approval threshold, no day-close print. | Shrink to the missing parts; print through the Phase 9 templates. |

## Phase 12: Data in and out

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 12.1 | Excel and CSV import (products, parties, opening balances), mapping, per-row errors, dry run, idempotent re-import | **Missing** | No file import anywhere in `src/` (the only file input is the product picture). v1's `xlsx` import was removed (A-82). | Build. Parse in the webview with `read-excel-file` (and a CSV parser), validate and commit in Rust in one transaction, so a counter imports through the hub like any command. |
| 12.2 | Backups include pictures | **Missing** | `services/backup.rs` has no image handling; A-90. | Build. Archive format version bump; old archives without images stay valid. |
| 12.3 | Counter picture cache | **Missing** | `src-tauri/src/client/` caches no images; A-90. | Build. Thumbnails only, size cap, cleanup. |
| 12.4 | First-start recovery screen | **Partial** | Start-up `PRAGMA quick_check`, keeps a `.corrupt-<time>` copy and refuses to open (`src-tauri/src/app.rs:155-185`, test at `:240-250`); A-79. Verify and restore exist for a running app (`backup_verify`, `backup_restore`). No screen when start-up fails. | Build the screen and a "recovery mode" start that serves only verify/restore/start-fresh commands. Reuse the restore code. |
| 12.5 | Full data export (CSV bundle) | **Missing** | Only per-report CSV/Excel (`src/lib/export.ts`). | Build (owner only, audited). |
| 12.6 | Product bulk edit; duplicate-barcode detector | **Missing** | No bulk selection in `products-page.tsx`. Barcodes are unique per table (see 9.4), so a detector matters for **pack vs variant collisions** and for imports. | Bulk edit: build. Detector: shrink to the cross-table check (also enforced on save from Phase 9) and an import-time report. |

## Phase 13: Multi-device and speed

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 13.1 | Offline credit sales with per-customer offline limit | **Missing** | Offline sales are fully paid only (A-48; `src-tauri/src/client/offline.rs`). `sync_exceptions` exists (`0009:26`). | Build as specified, default off. |
| 13.2 | Hub-served web UI for tablets and phones, browser pairing, sessions, touch till | **Partial** | HTTPS API with `/v1/call/:command` through `api::dispatch` (`hub/mod.rs:227-232`); `httpTransport` exists (`src/api/transport.ts:41`, used by the dev bridge). No static file serving, no browser pairing, no cookie/header strategy, no touch layout (A-5, A-47). | Build the missing parts on the existing API and transport. |
| 13.3 | Per-user preferences on the hub | **Missing** | Theme, spacing, columns and shortcuts are per-PC browser storage (A-74, A-88, A-92). | Build. Table created in Phase 11.5 (saved views) if that ships first. |
| 13.4 | `dashboard_summary` | **Missing** | Dashboard makes several `reports_run` calls (`src/features/dashboard/dashboard.tsx:135-145`; A-87). | Build. |
| 13.5 | Purchase preview | **Missing** | Purchase screens estimate; exact tax only on save (A-64). | Build; screens stop estimating. |
| 13.6 | Remote approval (optional) | **Missing** | Approval is inline only (A-40). | Build only if you confirm (optional item). |
| 13.7 | Incremental catalogue sync | **Missing** | Full snapshot every 15 minutes (`/v1/sync/catalogue`; A-52). | Build with the 20,000-item switch. |

## Phase 14: Customer-facing features

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 14.1 | UPI QR on receipts and invoices; record UPI payment | **Partial** | Payment mode `upi` exists (`0005:143`) and can be recorded with a reference. `invoice.show_qr` toggle exists but is unused (fact 4). No UPI ID setting, no QR encoder. | Build; reuse the Phase 9 QR encoder; make `show_qr` work. |
| 14.2 | PDF of invoices, quotations, statements; share (system share, `wa.me` link), save to folder, optional SMTP | **Missing** | PDF only through the print dialog (A-60). | Build. Needs a PDF writer on the Rust side (or the webview's print-to-PDF); decided in Phase 14 with an assumption. |
| 14.3 | Due reminders / receivables follow-up | **Partial** | Receivables report, party statement and open documents exist (`reports_run`, `parties_statement`, `parties_open_documents`). No promised date, notes, last contact, templates or dashboard card. | Shrink to the follow-up screen and its small table. |
| 14.4 | Recurring invoices | **Missing** | — | Build; runs in `jobs.rs` with an idempotent run key. |
| 14.5 | Loyalty and promotions (optional) | **Missing** | — | Build only if you confirm; pricing in the shared engine with golden vectors. |
| 14.6 | Customer display (optional) | **Missing** | — | Build only if you confirm. |
| 14.x | Leads (CRM) and lead-to-quotation | **Partial (data only)** | `leads` table (`0003:429`) filled by the v1 import; no service, command or screen (A-14). | Build service and screen on the existing table. |

## Phase 15: Release hardening

| # | Item | Status | Evidence | Decision |
|---|---|---|---|---|
| 15.1 | Code signing in CI, signature check | **Missing** | `.github/workflows/release.yml:25` builds unsigned; steps in `docs/RELEASE.md`. | Prepare; you supply the certificate. |
| 15.2 | Auto-update, channels, rollback, migration safety | **Missing** (one guard exists) | No updater plugin in `Cargo.toml` or `tauri.conf.json`. Newer-schema refusal already exists (`src-tauri/src/db/migrate.rs:136`, `NewerThanApp`). No safety backup before migrating on upgrade. | Prepare; you supply the signing key and hosting. |
| 15.3 | Packaged-window smoke test | **Missing** | A-80; e2e runs against the web build with the CSP. | Build the harness; needs a Windows runner. |
| 15.4 | Clean-PC install test | **Missing** | RELEASE.md checklist only. | Script and checklist; you run it on a VM. |
| 15.5 | Two-PC network test | **Missing** | Tests run hub and counters on 127.0.0.1 (CHANGELOG Phase 4). | Test plan; you run it on hardware. |
| 15.6 | Benchmarks and budgets | **Partial** | `services/tests/perf.rs:75` (50k products, 150k movements, ignored test). No year-of-documents generator, app-start or invoice-save timing (A-76). | Add generators and timings; CI budgets where stable. |
| 15.7 | Assistive-technology audit | **Partial** | axe scan in both themes (`e2e/a11y.spec.ts`; A-89). No NVDA or keyboard-only pass (A-75). | Keyboard pass and fixes here; NVDA pass needs a person (checklist provided). |
| 15.8 | Security pass, fuzz/property tests, encryption-at-rest guidance | **Partial** | `docs/SECURITY_REVIEW.md`, audit job in `ci.yml`. No fuzz or property tests. A-78 BitLocker note. | Extend for every new surface; add property tests for importers and print builders. |
| 15.9 | Localisation (Hindi, Malayalam), locale formats, Unicode printing | **Missing** | English only (A-74); money/date formatting by business locale exists (`src/lib/business.ts`, `src/lib/format.ts`). | Build. Translations need a native-speaker review (you). |
| 15.10 | macOS/Linux builds compiling in CI | **Partial** | Frontend job on `ubuntu-latest`; Rust and e2e on `windows-latest` only (`ci.yml:15,34,52`). | Add Rust build jobs for Linux/macOS (compile and test, not packaged). |

## Open questions (with my default if you don't answer)

1. **"Phases 9–16":** the map ends at 15. *Default: there is no Phase 16.*
2. **Report grouping, drill-down and saved views:** Phase 11 item 5 or Phase 12? *Default: Phase 11
   (it has the detailed spec); Phase 12 only gets anything left over.*
3. **Optional items** (13.6 remote approval, 14.5 loyalty/promotions, 14.6 customer display):
   *Default: build 13.6 and 14.5 last within their phase, skip 14.6 unless asked.*
4. **USB printing route:** RAW through the Windows printer queue (needs the vendor driver
   installed) vs talking to the USB device directly (needs a USB driver library and admin
   rights on some PCs). *Default: Windows queue in RAW mode.*
5. **Leads (CRM) scope:** not in the Phase 14 scope list. *Default: list, editor, stages and
   convert-to-quotation only.*
