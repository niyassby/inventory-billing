-- 0009: LAN hub, offline sales, backups and background jobs (Phase 4, docs/ARCHITECTURE.md §6–8).

-- Short code per paired device ("C1", "C2", …): the prefix of its offline invoice sub-series.
ALTER TABLE devices ADD COLUMN short_code TEXT;
CREATE UNIQUE INDEX ux_devices_short_code ON devices (short_code) WHERE short_code IS NOT NULL;

-- Where an invoice was created: on the hub/standalone PC, or offline on a client counter.
ALTER TABLE sales_invoices ADD COLUMN origin TEXT NOT NULL DEFAULT 'online'
  CHECK (origin IN ('online', 'offline'));
-- The counter's own clock when an offline invoice was made (the hub's clock is authoritative).
ALTER TABLE sales_invoices ADD COLUMN client_created_at TEXT;

-- One-time pairing codes shown on the hub (valid for minutes, single use; only the hash is kept).
CREATE TABLE pairing_codes (
  code_hash   TEXT PRIMARY KEY,
  created_by  TEXT NOT NULL REFERENCES users (id),
  created_at  TEXT NOT NULL,
  expires_at  TEXT NOT NULL,
  used_at     TEXT,
  device_id   TEXT REFERENCES devices (id),
  attempts    INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0)
) STRICT, WITHOUT ROWID;

-- Things that happened offline and need a manager's eye: an offline sale that took stock below
-- zero, or an item sold at a price that had changed on the hub meanwhile. Never auto-corrected.
CREATE TABLE sync_exceptions (
  id               TEXT PRIMARY KEY,
  kind             TEXT NOT NULL CHECK (kind IN ('offline_oversell', 'offline_price_changed', 'clock_skew')),
  device_id        TEXT REFERENCES devices (id),
  source_type      TEXT NOT NULL,
  source_id        TEXT NOT NULL,
  variant_id       TEXT REFERENCES product_variants (id),
  warehouse_id     TEXT REFERENCES warehouses (id),
  -- Units short (oversell) in thousandths, or the price difference in minor units.
  amount           INTEGER NOT NULL DEFAULT 0,
  details_json     TEXT CHECK (details_json IS NULL OR json_valid(details_json)),
  created_at       TEXT NOT NULL,
  resolved_at      TEXT,
  resolved_by      TEXT REFERENCES users (id),
  resolution_note  TEXT
) STRICT;
CREATE INDEX ix_sync_exceptions_open ON sync_exceptions (resolved_at, created_at);
CREATE INDEX ix_sync_exceptions_source ON sync_exceptions (source_type, source_id);

-- Every backup the hub made or restored, with the file's checksum for verification.
CREATE TABLE backup_runs (
  id              TEXT PRIMARY KEY,
  kind            TEXT NOT NULL CHECK (kind IN ('scheduled', 'manual', 'pre_restore', 'restore')),
  status          TEXT NOT NULL CHECK (status IN ('ok', 'failed')),
  started_at      TEXT NOT NULL,
  finished_at     TEXT NOT NULL,
  file_name       TEXT,
  size_bytes      INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
  sha256          TEXT,
  encrypted       INTEGER NOT NULL DEFAULT 0 CHECK (encrypted IN (0, 1)),
  schema_version  INTEGER,
  mirrored        INTEGER NOT NULL DEFAULT 0 CHECK (mirrored IN (0, 1)),
  error           TEXT,
  created_by      TEXT REFERENCES users (id)
) STRICT;
CREATE INDEX ix_backup_runs_started ON backup_runs (started_at);

-- Messages for the dashboard (low stock, failed backup, integrity problem). One open notice per
-- dedupe key, so an hourly job doesn't repeat itself.
CREATE TABLE notifications (
  id           TEXT PRIMARY KEY,
  kind         TEXT NOT NULL,
  severity     TEXT NOT NULL DEFAULT 'info' CHECK (severity IN ('info', 'warning', 'error')),
  title        TEXT NOT NULL,
  body         TEXT,
  entity_type  TEXT,
  entity_id    TEXT,
  dedupe_key   TEXT,
  created_at   TEXT NOT NULL,
  read_at      TEXT
) STRICT;
CREATE UNIQUE INDEX ux_notifications_open ON notifications (dedupe_key)
  WHERE dedupe_key IS NOT NULL AND read_at IS NULL;
CREATE INDEX ix_notifications_created ON notifications (created_at);

-- Last run of each background job (hub and standalone only).
CREATE TABLE job_runs (
  job          TEXT PRIMARY KEY,
  last_run_at  TEXT NOT NULL,
  last_status  TEXT NOT NULL CHECK (last_status IN ('ok', 'failed', 'skipped')),
  last_error   TEXT
) STRICT, WITHOUT ROWID;

INSERT INTO permissions (code, description) VALUES
  ('devices.manage', 'Manage (devices)'),
  ('sync.review', 'Review (sync)');
-- Owner and manager have every permission except user and backup management.
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000001', 'devices.manage'),
  ('01920000-0000-7000-8000-000000000001', 'sync.review'),
  ('01920000-0000-7000-8000-000000000002', 'devices.manage'),
  ('01920000-0000-7000-8000-000000000002', 'sync.review');
