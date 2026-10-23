# Security review summary (Phase 8)

Scope: the code in this repository at the Phase 8 commit. Method: review against the Phase 0
findings (docs/AUDIT.md S-1 … S-12), `npm audit`, `cargo audit`, license listing and the tests.
It is **not** an independent penetration test.

## Phase 0 findings, now

| ID   | Finding                                                     | Status                                                                                                                                                                                      |
| ---- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S-1  | Seeded default admin, shown on the login screen             | **Fixed.** No default account; the owner is created at first run (min. 10 characters).                                                                                                      |
| S-2  | No lockout or rate limit                                    | **Fixed.** 5 failures lock the account for 15 minutes; sign-in is rate limited per e-mail; failures are audited.                                                                            |
| S-3  | Backup and restore with arbitrary paths, unencrypted        | **Fixed.** Restore verifies checksum, encryption and integrity before touching data and makes a safety backup first; backups can be AES-256-GCM encrypted. Backup management is owner-only. |
| S-4  | No Content-Security-Policy                                  | **Fixed in config.** Strict CSP in `tauri.conf.json`; the end-to-end test serves the build with the same policy and fails on any violation. Not yet confirmed inside a packaged window (RELEASE.md). |
| S-5  | Token in `localStorage`, no idle timeout or device binding  | **Mostly fixed.** Tokens are random and stored hashed on the backend, expire after 12 h / 30 min idle, and LAN sessions are bound to the device. The browser-bridge build still keeps the token in storage (development only). |
| S-6  | Free-text roles, no permissions or approvals                | **Fixed.** Permission-based roles, last-owner protection, manager approval flow.                                                                                                            |
| S-7  | No server-side validation                                   | **Fixed.** Services validate everything; money is integer minor units.                                                                                                                      |
| S-8  | Plain password named `password_hash`                        | **Fixed.** Argon2id hashes, password policy, change-password flow.                                                                                                                          |
| S-9  | Mutable audit log with PII dumps                            | **Fixed.** Append-only (triggers), records who approved what; summaries instead of row dumps.                                                                                              |
| S-10 | No encryption at rest                                       | **Open (accepted).** The database file is not encrypted; use Windows BitLocker. Backups can be encrypted. See ASSUMPTIONS A-78.                                                             |
| S-11 | Raw database errors shown                                   | **Fixed.** Typed error codes with friendly messages; details only in logs.                                                                                                                  |
| S-12 | Remote image on the login page                              | **Fixed.** No remote content; the CSP forbids it.                                                                                                                                           |

## Network (LAN hub)

HTTPS with a self-signed certificate that counters pin at pairing (fingerprint compared by eye),
one-time pairing codes, per-device credentials stored hashed on the hub and in the OS keychain
on the counter, requests only from private addresses, rate limited pairing. The hub speaks no
plain HTTP. Not covered: a hostile device already on the shop network beyond what the above gives.

## Printing (Phase 9)

- Only the Rust side talks to printers; the window asks for a document and gets a result. The
  printer commands are built in Rust from the backend's own figures.
- Network printers must be an IP address on a private, link-local or loopback network, checked
  on save and before every job (A-105); serial port names and Windows printer names are checked
  for shape. A printer address can't send printer bytes off the shop network.
- Windows printing uses the spooler in RAW mode through `windows-sys` (`OpenPrinterW`,
  `WritePrinter`); the unsafe blocks are small, documented and close the handle on every path.
- Counters print to the hub only through the normal authenticated API (`print_jobs_submit`),
  with idempotency keys, permission checks per document type and only to printers the hub
  marked as shared. A replayed request never prints twice.
- Opening the drawer without a sale needs `sales.open_drawer` or a manager's approval and is
  audited. Printer changes are audited on the hub (best effort from an offline counter, A-97).
- New dependencies: `qrcode`, `rustybuzz`, `fontdue`, `serialport` (MIT / Apache-2.0) and
  `windows-sys` (already in the tree through Tauri). `cargo audit` to be re-run in CI.
- Fonts are read from the Windows font folder or a path the owner names; a font file that fails
  to parse is ignored (the line prints as text).

## Inventory depth (Phase 10)

- No new dependencies, network calls or secrets. Price-list, batch and expiry data travel only
  through the authenticated API; counters cache price lists like the rest of the catalogue.
- New permissions `price_lists.manage` (owner, manager) and `inventory.valuation` (owner only).
  Changing product prices in bulk needs `products.price_change`; write-offs need `stock.adjust`.
- The valuation switch is owner-only, needs a reason, is refused unless the total matches the
  dry run the owner saw and is audited (`VALUATION_SWITCHED`). The settings form can no longer
  change the method. Price-list changes are audited (`PRICE_LIST_SAVED`, `PRICE_LIST_PRICES`,
  `PRICE_BULK_UPDATE`, `PRICE_LIST_ASSIGNED`, `PRICE_LIST_DELETED`).
- Every list query uses bound parameters; sort keys come from fixed lists. Bulk updates never
  set a negative price.
- Selling expired stock is blocked by default; offline counters warn instead (A-109).

## Dependencies (checked 2026-10-20)

- **npm** (`npm audit --omit=dev`): the one finding (`xlsx`, SheetJS, unfixed prototype-pollution
  and ReDoS advisories) was removed by replacing it with `write-excel-file` (MIT, writes only).
- **Rust** (`cargo audit`): 2 advisories, both `quick-xml 0.39.4` (RUSTSEC-2026-0194 and -0195,
  denial of service) pulled in by `plist` ← `tauri-utils` ← `tauri` / `tauri-build`. The app never
  feeds untrusted XML to it (it handles Apple `Info.plist` files at build time). Fixed upstream
  in `quick-xml` 0.41: **update Tauri when it moves**. 13 "unmaintained / unsound" warnings are
  transitive (GTK/zbus on Linux, `unic-*`, `proc-macro-error`, `rustls-pemfile`, `anyhow`,
  `event-listener`); none is reachable from user input in this app.
- **Licenses**: Rust crates are MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode (5 MPL-2.0,
  file-level copyleft, unmodified; 2 "OR LGPL-2.1" with an MIT/Apache option). npm packages are
  MIT / ISC / BSD / Apache-2.0 / MPL-2.0 / 0BSD plus one OFL-1.1 font (Figtree). No GPL or
  AGPL in the shipped app.

## Open items

1. Run the packaged build once on a clean Windows PC and confirm the CSP and printing there.
2. Code-sign the installer (SmartScreen warns on unsigned installers).
3. Re-run `npm audit` and `cargo audit` before every release (the CI job does).
4. Database encryption at rest (SQLCipher) if a shop needs it: a suggestion, not built.
