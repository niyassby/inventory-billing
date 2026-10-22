-- 0010: printing (Phase 9): the hub's print queue, barcode label templates, and the printing
-- permissions. Printers themselves are per PC (printers.json next to the database), not here.

-- Documents a counter (or the hub itself) sent to one of the hub's shared printers. The rendered
-- job is kept so a retry prints exactly the same bytes. Printed and cancelled jobs are removed by
-- the daily clean-up after 7 days; they are working data, not business records.
CREATE TABLE print_jobs (
  id            TEXT PRIMARY KEY,
  -- The printer's id in the hub's printers.json (not a database row).
  printer_id    TEXT NOT NULL,
  printer_name  TEXT NOT NULL,
  title         TEXT NOT NULL,
  doc_type      TEXT NOT NULL CHECK (doc_type IN ('receipt', 'return_slip', 'till_close', 'held_slip', 'labels', 'test')),
  data          BLOB NOT NULL,
  status        TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'printed', 'failed', 'cancelled')),
  attempts      INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  last_error    TEXT,
  error_kind    TEXT,
  -- When the background job tries a failed job again (NULL: no more automatic tries).
  next_try_at   TEXT,
  printed_at    TEXT,
  -- The counter that sent it; NULL when it was printed from the hub's own window.
  device_id     TEXT REFERENCES devices (id),
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT,
  CHECK (status <> 'printed' OR printed_at IS NOT NULL)
) STRICT;
CREATE INDEX ix_print_jobs_status ON print_jobs (status, next_try_at);
CREATE INDEX ix_print_jobs_created ON print_jobs (created_at);

CREATE TABLE label_templates (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  width_mm       INTEGER NOT NULL CHECK (width_mm BETWEEN 15 AND 150),
  height_mm      INTEGER NOT NULL CHECK (height_mm BETWEEN 10 AND 150),
  gap_mm         INTEGER NOT NULL DEFAULT 2 CHECK (gap_mm BETWEEN 0 AND 10),
  columns        INTEGER NOT NULL DEFAULT 1 CHECK (columns BETWEEN 1 AND 4),
  symbology      TEXT NOT NULL DEFAULT 'code128' CHECK (symbology IN ('ean13', 'ean8', 'code128', 'qr')),
  show_business  INTEGER NOT NULL DEFAULT 0 CHECK (show_business IN (0, 1)),
  show_name      INTEGER NOT NULL DEFAULT 1 CHECK (show_name IN (0, 1)),
  show_price     INTEGER NOT NULL DEFAULT 1 CHECK (show_price IN (0, 1)),
  show_mrp       INTEGER NOT NULL DEFAULT 0 CHECK (show_mrp IN (0, 1)),
  show_sku       INTEGER NOT NULL DEFAULT 0 CHECK (show_sku IN (0, 1)),
  show_barcode   INTEGER NOT NULL DEFAULT 1 CHECK (show_barcode IN (0, 1)),
  show_batch     INTEGER NOT NULL DEFAULT 0 CHECK (show_batch IN (0, 1)),
  show_expiry    INTEGER NOT NULL DEFAULT 0 CHECK (show_expiry IN (0, 1)),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  updated_by     TEXT REFERENCES users (id),
  deleted_at     TEXT
) STRICT;
CREATE UNIQUE INDEX ux_label_templates_name ON label_templates (lower(name)) WHERE deleted_at IS NULL;

-- Two common sizes to start from (both can be edited or removed).
INSERT INTO label_templates (id, name, width_mm, height_mm, gap_mm, columns, symbology, show_name, show_price,
  show_mrp, show_barcode, created_at, updated_at) VALUES
  ('01920000-0000-7000-8000-00000000a001', 'Shelf label 50 × 25 mm', 50, 25, 2, 1, 'ean13', 1, 1, 1, 1,
   '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z'),
  ('01920000-0000-7000-8000-00000000a002', 'Small tag 38 × 25 mm, 2 across', 38, 25, 2, 2, 'code128', 1, 1, 0, 1,
   '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z');

INSERT INTO permissions (code, description) VALUES
  ('printing.manage', 'Manage (printing)'),
  ('labels.print', 'Print (labels)'),
  ('sales.open_drawer', 'Open drawer (sales)');
-- Owner and manager get all three; stock keepers print labels. Cashiers open the drawer only
-- with a cash sale or a manager's approval.
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000001', 'printing.manage'),
  ('01920000-0000-7000-8000-000000000001', 'labels.print'),
  ('01920000-0000-7000-8000-000000000001', 'sales.open_drawer'),
  ('01920000-0000-7000-8000-000000000002', 'printing.manage'),
  ('01920000-0000-7000-8000-000000000002', 'labels.print'),
  ('01920000-0000-7000-8000-000000000002', 'sales.open_drawer'),
  ('01920000-0000-7000-8000-000000000004', 'labels.print');
