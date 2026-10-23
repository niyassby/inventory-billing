-- 0011: inventory depth (Phase 10): FIFO cost layers, price lists with effective dates and kinds,
-- a price list per till, the exact cost of each sale line, and the permissions that go with them.

-- FIFO: what is left of each receipt, per variant, store and batch ('' = no batch). Only kept
-- while inventory.valuationMethod is "fifo". value_left is kept exactly (not qty × unit cost), so
-- the layers of a key always add up to the stock ledger's value for it.
CREATE TABLE cost_layers (
  id             TEXT PRIMARY KEY,
  variant_id     TEXT NOT NULL REFERENCES product_variants (id),
  warehouse_id   TEXT NOT NULL REFERENCES warehouses (id),
  batch_key      TEXT NOT NULL DEFAULT '',
  -- The inbound movement that made it; NULL for an opening layer built when switching to FIFO.
  movement_id    TEXT REFERENCES stock_movements (id),
  -- Consumed oldest first (then by creation).
  received_at    TEXT NOT NULL,
  business_date  TEXT NOT NULL,
  qty_in         INTEGER NOT NULL CHECK (qty_in > 0),
  qty_left       INTEGER NOT NULL CHECK (qty_left >= 0 AND qty_left <= qty_in),
  unit_cost      INTEGER NOT NULL CHECK (unit_cost >= 0),
  value_left     INTEGER NOT NULL CHECK (value_left >= 0),
  created_at     TEXT NOT NULL
) STRICT;
CREATE INDEX ix_cost_layers_open ON cost_layers (variant_id, warehouse_id, batch_key, received_at)
  WHERE qty_left > 0;
CREATE INDEX ix_cost_layers_movement ON cost_layers (movement_id) WHERE movement_id IS NOT NULL;

-- Which layers an outbound movement took, and at what value (append-only).
CREATE TABLE cost_layer_uses (
  id           TEXT PRIMARY KEY,
  layer_id     TEXT NOT NULL REFERENCES cost_layers (id),
  movement_id  TEXT NOT NULL REFERENCES stock_movements (id),
  qty          INTEGER NOT NULL CHECK (qty > 0),
  value        INTEGER NOT NULL CHECK (value >= 0),
  created_at   TEXT NOT NULL
) STRICT;
CREATE INDEX ix_cost_layer_uses_movement ON cost_layer_uses (movement_id);
CREATE INDEX ix_cost_layer_uses_layer ON cost_layer_uses (layer_id);
CREATE TRIGGER trg_cost_layer_uses_no_update BEFORE UPDATE ON cost_layer_uses
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: cost_layer_uses rows cannot be changed');
END;
CREATE TRIGGER trg_cost_layer_uses_no_delete BEFORE DELETE ON cost_layer_uses
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: cost_layer_uses rows cannot be deleted');
END;

-- Layers that disagree with the stock ledger while FIFO is in use (always empty otherwise).
CREATE VIEW v_cost_layer_mismatches AS
WITH layers AS (
  SELECT variant_id, warehouse_id, batch_key, sum(qty_left) AS qty, sum(value_left) AS value
  FROM cost_layers GROUP BY variant_id, warehouse_id, batch_key
)
SELECT s.variant_id, s.warehouse_id, s.batch_key, s.qty AS ledger_qty, coalesce(l.qty, 0) AS layer_qty,
       s.value AS ledger_value, coalesce(l.value, 0) AS layer_value
FROM stock_levels s
LEFT JOIN layers l ON l.variant_id = s.variant_id AND l.warehouse_id = s.warehouse_id AND l.batch_key = s.batch_key
WHERE s.qty > 0 AND (s.qty <> coalesce(l.qty, 0) OR s.value <> coalesce(l.value, 0))
  AND (SELECT json_extract(value_json, '$.valuationMethod') FROM settings WHERE key = 'inventory') = 'fifo';

-- Price lists: a kind (retail, wholesale, custom), and per-price effective dates.
ALTER TABLE price_lists ADD COLUMN kind TEXT NOT NULL DEFAULT 'custom'
  CHECK (kind IN ('retail', 'wholesale', 'custom'));
ALTER TABLE price_lists ADD COLUMN notes TEXT;
ALTER TABLE price_list_items ADD COLUMN valid_from TEXT;
ALTER TABLE price_list_items ADD COLUMN valid_to TEXT;
DROP INDEX ux_price_list_items;
CREATE UNIQUE INDEX ux_price_list_items ON price_list_items
  (price_list_id, variant_id, coalesce(unit_id, ''), min_qty, coalesce(valid_from, ''))
  WHERE deleted_at IS NULL;

-- The price list a till sells at (a customer's own list wins).
ALTER TABLE cash_registers ADD COLUMN price_list_id TEXT REFERENCES price_lists (id);

-- The cost of a sale line exactly as it left the stock ledger (FIFO costs vary per unit);
-- NULL on lines from before this version (their cost is unit_cost × base_qty).
ALTER TABLE sales_invoice_items ADD COLUMN cost_value INTEGER;

INSERT INTO permissions (code, description) VALUES
  ('price_lists.manage', 'Manage (price lists)'),
  ('inventory.valuation', 'Change valuation method (inventory)');
-- Price lists: owner and manager. Switching the valuation method: owner only.
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000001', 'price_lists.manage'),
  ('01920000-0000-7000-8000-000000000001', 'inventory.valuation'),
  ('01920000-0000-7000-8000-000000000002', 'price_lists.manage');
