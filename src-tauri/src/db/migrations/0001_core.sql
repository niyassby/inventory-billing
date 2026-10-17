-- 0001 core: organisation, users and security, settings, numbering, audit, sync plumbing.
--
-- Conventions (docs/DATABASE.md §2):
--   * ids are TEXT UUIDs (v7 for new rows; migrated legacy v4 ids are kept)
--   * money = INTEGER minor units, quantities = INTEGER thousandths, rates = INTEGER ppm
--   * timestamps = TEXT RFC 3339 UTC; business dates = TEXT 'YYYY-MM-DD' in the business timezone
--   * business tables carry created_at, updated_at, created_by, updated_by, deleted_at
--   * every table is STRICT so a float can never land in a money column

CREATE TABLE branches (
  id            TEXT PRIMARY KEY,
  code          TEXT NOT NULL,
  name          TEXT NOT NULL,
  address       TEXT,
  phone         TEXT,
  email         TEXT,
  gstin         TEXT,
  state_code    TEXT,
  is_default    INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT
) STRICT;
CREATE UNIQUE INDEX ux_branches_code ON branches (lower(code)) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_branches_default ON branches (is_default) WHERE is_default = 1 AND deleted_at IS NULL;

CREATE TABLE warehouses (
  id            TEXT PRIMARY KEY,
  branch_id     TEXT NOT NULL REFERENCES branches (id),
  code          TEXT NOT NULL,
  name          TEXT NOT NULL,
  is_default    INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT
) STRICT;
CREATE UNIQUE INDEX ux_warehouses_code ON warehouses (lower(code)) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_warehouses_default ON warehouses (branch_id) WHERE is_default = 1 AND deleted_at IS NULL;

CREATE TABLE permissions (
  code          TEXT PRIMARY KEY,
  description   TEXT NOT NULL
) STRICT;

CREATE TABLE roles (
  id                 TEXT PRIMARY KEY,
  code               TEXT NOT NULL,
  name               TEXT NOT NULL,
  is_system          INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
  -- Largest discount this role may give without manager approval (ppm of the bill).
  max_discount_rate  INTEGER NOT NULL DEFAULT 0 CHECK (max_discount_rate BETWEEN 0 AND 1000000),
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  created_by         TEXT REFERENCES users (id),
  updated_by         TEXT REFERENCES users (id),
  deleted_at         TEXT
) STRICT;
CREATE UNIQUE INDEX ux_roles_code ON roles (code) WHERE deleted_at IS NULL;

CREATE TABLE role_permissions (
  role_id          TEXT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
  permission_code  TEXT NOT NULL REFERENCES permissions (code),
  PRIMARY KEY (role_id, permission_code)
) STRICT, WITHOUT ROWID;

CREATE TABLE users (
  id                    TEXT PRIMARY KEY,
  name                  TEXT NOT NULL,
  email                 TEXT NOT NULL,
  phone                 TEXT,
  password_hash         TEXT NOT NULL,
  pin_hash              TEXT,
  role_id               TEXT NOT NULL REFERENCES roles (id),
  default_branch_id     TEXT REFERENCES branches (id),
  is_active             INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  must_change_password  INTEGER NOT NULL DEFAULT 0 CHECK (must_change_password IN (0, 1)),
  failed_login_count    INTEGER NOT NULL DEFAULT 0 CHECK (failed_login_count >= 0),
  locked_until          TEXT,
  last_login_at         TEXT,
  password_changed_at   TEXT,
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  created_by            TEXT REFERENCES users (id),
  updated_by            TEXT REFERENCES users (id),
  deleted_at            TEXT
) STRICT;
CREATE UNIQUE INDEX ux_users_email ON users (lower(email)) WHERE deleted_at IS NULL;
CREATE INDEX ix_users_role ON users (role_id);

CREATE TABLE devices (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  mode          TEXT NOT NULL CHECK (mode IN ('standalone', 'hub', 'client')),
  branch_id     TEXT REFERENCES branches (id),
  token_hash    TEXT,
  app_version   TEXT,
  paired_at     TEXT,
  last_seen_at  TEXT,
  revoked_at    TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT
) STRICT;
CREATE UNIQUE INDEX ux_devices_token ON devices (token_hash) WHERE token_hash IS NOT NULL;

CREATE TABLE sessions (
  id            TEXT PRIMARY KEY,
  user_id       TEXT NOT NULL REFERENCES users (id),
  device_id     TEXT REFERENCES devices (id),
  token_hash    TEXT NOT NULL UNIQUE,
  created_at    TEXT NOT NULL,
  expires_at    TEXT NOT NULL,
  last_seen_at  TEXT,
  revoked_at    TEXT
) STRICT;
CREATE INDEX ix_sessions_user ON sessions (user_id);
CREATE INDEX ix_sessions_expires ON sessions (expires_at);

-- Typed, versioned settings: one row per section, value validated by the app (Zod / Rust DTO).
CREATE TABLE settings (
  key         TEXT PRIMARY KEY,
  value_json  TEXT NOT NULL CHECK (json_valid(value_json)),
  version     INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
  updated_at  TEXT NOT NULL,
  updated_by  TEXT REFERENCES users (id)
) STRICT;

-- Document numbering. A series is configuration; counters hold the next number per period
-- (financial-year label, or 'all' when the series never resets).
CREATE TABLE number_series (
  id            TEXT PRIMARY KEY,
  branch_id     TEXT NOT NULL REFERENCES branches (id),
  doc_type      TEXT NOT NULL CHECK (doc_type IN (
                  'sales_invoice', 'sales_return', 'quotation', 'sales_order', 'delivery_challan',
                  'credit_note', 'purchase_order', 'goods_receipt', 'purchase_bill',
                  'purchase_return', 'payment_in', 'payment_out', 'expense', 'stock_transfer',
                  'stock_adjustment', 'stock_count')),
  name          TEXT NOT NULL,
  prefix        TEXT NOT NULL,
  template      TEXT NOT NULL DEFAULT '{prefix}/{fy}/{seq}' CHECK (instr(template, '{seq}') > 0),
  padding       INTEGER NOT NULL DEFAULT 6 CHECK (padding BETWEEN 0 AND 12),
  reset_policy  TEXT NOT NULL DEFAULT 'yearly' CHECK (reset_policy IN ('yearly', 'never')),
  -- Set for a client device's offline sub-series (docs/ARCHITECTURE.md §6).
  device_id     TEXT REFERENCES devices (id),
  is_default    INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  is_active     INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT
) STRICT;
CREATE UNIQUE INDEX ux_number_series_default ON number_series (branch_id, doc_type)
  WHERE is_default = 1 AND device_id IS NULL AND deleted_at IS NULL;

CREATE TABLE number_series_counters (
  series_id   TEXT NOT NULL REFERENCES number_series (id),
  period      TEXT NOT NULL,
  next_seq    INTEGER NOT NULL CHECK (next_seq >= 1),
  updated_at  TEXT NOT NULL,
  PRIMARY KEY (series_id, period)
) STRICT, WITHOUT ROWID;

-- Blocks of numbers leased to a client device so it can bill while the hub is unreachable.
CREATE TABLE number_series_leases (
  id           TEXT PRIMARY KEY,
  series_id    TEXT NOT NULL REFERENCES number_series (id),
  device_id    TEXT NOT NULL REFERENCES devices (id),
  period       TEXT NOT NULL,
  from_seq     INTEGER NOT NULL CHECK (from_seq >= 1),
  to_seq       INTEGER NOT NULL,
  next_seq     INTEGER NOT NULL,
  leased_at    TEXT NOT NULL,
  released_at  TEXT,
  CHECK (to_seq >= from_seq),
  CHECK (next_seq BETWEEN from_seq AND to_seq + 1)
) STRICT;
CREATE INDEX ix_number_series_leases_device ON number_series_leases (device_id, series_id);

-- Append-only audit trail (enforced by triggers below).
CREATE TABLE audit_log (
  id           TEXT PRIMARY KEY,
  occurred_at  TEXT NOT NULL,
  user_id      TEXT REFERENCES users (id),
  device_id    TEXT REFERENCES devices (id),
  approved_by  TEXT REFERENCES users (id),
  action       TEXT NOT NULL,
  entity_type  TEXT NOT NULL,
  entity_id    TEXT,
  summary      TEXT,
  before_json  TEXT CHECK (before_json IS NULL OR json_valid(before_json)),
  after_json   TEXT CHECK (after_json IS NULL OR json_valid(after_json))
) STRICT;
CREATE INDEX ix_audit_log_occurred ON audit_log (occurred_at);
CREATE INDEX ix_audit_log_entity ON audit_log (entity_type, entity_id);
CREATE INDEX ix_audit_log_user ON audit_log (user_id, occurred_at);

CREATE TRIGGER trg_audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: audit_log rows cannot be changed');
END;
CREATE TRIGGER trg_audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: audit_log rows cannot be deleted');
END;

-- Every mutating request carries an idempotency key; the stored response is replayed on retry.
CREATE TABLE idempotency_keys (
  key            TEXT PRIMARY KEY,
  user_id        TEXT REFERENCES users (id),
  device_id      TEXT REFERENCES devices (id),
  command        TEXT NOT NULL,
  request_hash   TEXT NOT NULL,
  response_json  TEXT CHECK (response_json IS NULL OR json_valid(response_json)),
  created_at     TEXT NOT NULL
) STRICT, WITHOUT ROWID;
CREATE INDEX ix_idempotency_keys_created ON idempotency_keys (created_at);

-- Client side: writes queued while the hub is unreachable, drained in device_seq order.
CREATE TABLE sync_outbox (
  id               TEXT PRIMARY KEY,
  idempotency_key  TEXT NOT NULL UNIQUE,
  device_seq       INTEGER NOT NULL UNIQUE,
  command          TEXT NOT NULL,
  payload_json     TEXT NOT NULL CHECK (json_valid(payload_json)),
  status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'sending', 'done', 'rejected')),
  attempts         INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  last_error       TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL
) STRICT;
CREATE INDEX ix_sync_outbox_pending ON sync_outbox (status, device_seq);

-- Hub side: what each device submitted and what happened to it.
CREATE TABLE sync_log (
  id               TEXT PRIMARY KEY,
  device_id        TEXT NOT NULL REFERENCES devices (id),
  idempotency_key  TEXT NOT NULL,
  device_seq       INTEGER NOT NULL,
  command          TEXT NOT NULL,
  status           TEXT NOT NULL CHECK (status IN ('applied', 'duplicate', 'rejected')),
  error_code       TEXT,
  client_time      TEXT,
  received_at      TEXT NOT NULL,
  UNIQUE (device_id, idempotency_key)
) STRICT;

-- The original row of every migrated legacy record, so nothing from v1 is ever lost.
CREATE TABLE legacy_records (
  source_table  TEXT NOT NULL,
  source_id     TEXT NOT NULL,
  row_json      TEXT NOT NULL CHECK (json_valid(row_json)),
  migrated_at   TEXT NOT NULL,
  PRIMARY KEY (source_table, source_id)
) STRICT, WITHOUT ROWID;
