# Release and production checklist

## Build

```bash
npm ci
npm run check          # types, lint, format, unit tests with coverage
npm run rust:fmt && npm run rust:lint && npm run rust:test
npm run test:e2e       # builds the web build and the bridge, drives Chrome
npm run tauri build    # Windows installers: src-tauri/target/release/bundle/{nsis,msi}
```

Version number: change `version` in `package.json`, `src-tauri/tauri.conf.json` and
`src-tauri/Cargo.toml` together, then add the CHANGELOG entry. The installer is named from
`productName`. The data folder comes from the identifier (`com.niyas.tauri-app`) and **must not
change**, or the app would look for its data in a new, empty folder.

Releases on GitHub: push a tag such as `v1.2.3`; `.github/workflows/release.yml` builds the
Windows installers and attaches them to a draft release.

## Before every release

- [ ] CI green (frontend, Rust, end-to-end, audit).
- [ ] `docs/SECURITY_REVIEW.md` dependency section re-checked (`npm audit`, `cargo audit`).
- [ ] Installer built and installed on a **clean Windows PC**; first-run setup completed there.
- [ ] On that PC: sell, print a receipt, run a report, export CSV, make a backup, run the restore
      drill (ADMIN_GUIDE §4), restart the app and check the data is still there.
- [ ] The release build shows no developer tools and contains no sample data.
- [ ] Upgrade test: install over the previous version with a copy of a real database; it
      migrates and every old document opens.
- [ ] CHANGELOG updated; tag pushed.

## Code signing

Unsigned installers work, but Windows SmartScreen shows "unknown publisher" until the file has
reputation. To sign:

1. Buy a code-signing certificate (OV, or EV for immediate SmartScreen trust) or use a cloud
   signer (Azure Trusted Signing).
2. In `tauri.conf.json` set `bundle.windows.certificateThumbprint` (and `timestampUrl`), or use
   `signCommand` for a cloud signer.
3. In CI keep the certificate and its password as repository **secrets**, never in the repo.

## Auto-update

Not enabled: it needs a signing key pair and a place to host `latest.json`.

1. `npm run tauri signer generate -- -w ~/.tauri/inventory.key`. Keep the private key and its
   password offline and backed up; without them customers can't receive updates.
2. Add `tauri-plugin-updater` (Rust and JS), put the **public** key and the update URL in
   `tauri.conf.json` under `plugins.updater`, and set `bundle.createUpdaterArtifacts`.
3. Host `latest.json` and the installers (GitHub releases work).
4. Updates must migrate data safely: the app migrates the database in versioned steps, and a
   failed migration leaves the old file intact (docs/DATABASE.md).

## Running in production

- Logs: `%LOCALAPPDATA%\com.niyas.tauri-app\logs`, one file a day, 14 days. To get help, zip that
  folder (it holds no passwords or card data).
- Data: `%LOCALAPPDATA%\com.niyas.tauri-app\inventory_v2.db` (plus `-wal` / `-shm` files while the
  app runs). Never copy the database while the app runs; use the backup button.
- Closing the app is safe at any time: every write is a database transaction, so a power cut never
  leaves half a sale. The database is checked at start; a damaged file is not opened (see
  ADMIN_GUIDE, "If the data file is damaged").
