-- 0002 catalogue and stock: products, variants (the stock-keeping unit), units, tax rates,
-- price lists, batches, the stock ledger and its cache, adjustments, transfers and counts.

CREATE TABLE categories (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  parent_id   TEXT REFERENCES categories (id),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT REFERENCES users (id),
  updated_by  TEXT REFERENCES users (id),
  deleted_at  TEXT,
  CHECK (parent_id IS NULL OR parent_id <> id)
) STRICT;
CREATE UNIQUE INDEX ux_categories_name ON categories (coalesce(parent_id, ''), lower(name))
  WHERE deleted_at IS NULL;

CREATE TABLE brands (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT REFERENCES users (id),
  updated_by  TEXT REFERENCES users (id),
  deleted_at  TEXT
) STRICT;
CREATE UNIQUE INDEX ux_brands_name ON brands (lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE units (
  id          TEXT PRIMARY KEY,
  code        TEXT NOT NULL,
  name        TEXT NOT NULL,
  -- Decimal places a quantity in this unit may have (pcs 0, kg 3).
  decimals    INTEGER NOT NULL DEFAULT 0 CHECK (decimals BETWEEN 0 AND 3),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT REFERENCES users (id),
  updated_by  TEXT REFERENCES users (id),
  deleted_at  TEXT
) STRICT;
CREATE UNIQUE INDEX ux_units_code ON units (lower(code)) WHERE deleted_at IS NULL;

CREATE TABLE tax_rates (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  -- gst: split into CGST+SGST or IGST at billing time; vat/other: single component.
  tax_type    TEXT NOT NULL CHECK (tax_type IN ('gst', 'vat', 'none', 'other')),
  rate        INTEGER NOT NULL CHECK (rate BETWEEN 0 AND 1000000),
  cess_rate   INTEGER NOT NULL DEFAULT 0 CHECK (cess_rate BETWEEN 0 AND 1000000),
  is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT REFERENCES users (id),
  updated_by  TEXT REFERENCES users (id),
  deleted_at  TEXT,
  CHECK (tax_type <> 'gst' OR rate % 2 = 0)
) STRICT;
CREATE UNIQUE INDEX ux_tax_rates_name ON tax_rates (lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE products (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  description    TEXT,
  category_id    TEXT REFERENCES categories (id),
  brand_id       TEXT REFERENCES brands (id),
  base_unit_id   TEXT NOT NULL REFERENCES units (id),
  tax_rate_id    TEXT REFERENCES tax_rates (id),
  hsn_sac        TEXT,
  has_variants   INTEGER NOT NULL DEFAULT 0 CHECK (has_variants IN (0, 1)),
  track_batches  INTEGER NOT NULL DEFAULT 0 CHECK (track_batches IN (0, 1)),
  track_expiry   INTEGER NOT NULL DEFAULT 0 CHECK (track_expiry IN (0, 1)),
  is_service     INTEGER NOT NULL DEFAULT 0 CHECK (is_service IN (0, 1)),
  image_path     TEXT,
  is_active      INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  updated_by     TEXT REFERENCES users (id),
  deleted_at     TEXT
) STRICT;
CREATE INDEX ix_products_category ON products (category_id);
CREATE INDEX ix_products_brand ON products (brand_id);

-- The sellable, stock-keeping unit. A simple product has exactly one variant (variant_name NULL).
CREATE TABLE product_variants (
  id               TEXT PRIMARY KEY,
  product_id       TEXT NOT NULL REFERENCES products (id),
  variant_name     TEXT,
  attributes_json  TEXT CHECK (attributes_json IS NULL OR json_valid(attributes_json)),
  sku              TEXT NOT NULL,
  barcode          TEXT,
  sale_price       INTEGER NOT NULL DEFAULT 0 CHECK (sale_price >= 0),
  mrp              INTEGER CHECK (mrp IS NULL OR mrp >= 0),
  -- Current valuation cost per base unit (weighted average); history lives in stock_movements.
  cost_price       INTEGER NOT NULL DEFAULT 0 CHECK (cost_price >= 0),
  reorder_level    INTEGER NOT NULL DEFAULT 0 CHECK (reorder_level >= 0),
  reorder_qty      INTEGER NOT NULL DEFAULT 0 CHECK (reorder_qty >= 0),
  is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  created_by       TEXT REFERENCES users (id),
  updated_by       TEXT REFERENCES users (id),
  deleted_at       TEXT
) STRICT;
CREATE UNIQUE INDEX ux_product_variants_sku ON product_variants (lower(sku)) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_product_variants_barcode ON product_variants (barcode)
  WHERE barcode IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX ix_product_variants_product ON product_variants (product_id);

-- 1 from_unit = factor_num / factor_den to_unit. product_id NULL = applies to every product.
CREATE TABLE unit_conversions (
  id            TEXT PRIMARY KEY,
  product_id    TEXT REFERENCES products (id),
  from_unit_id  TEXT NOT NULL REFERENCES units (id),
  to_unit_id    TEXT NOT NULL REFERENCES units (id),
  factor_num    INTEGER NOT NULL CHECK (factor_num > 0),
  factor_den    INTEGER NOT NULL DEFAULT 1 CHECK (factor_den > 0),
  -- Barcode printed on the pack (e.g. the carton of 12).
  barcode       TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT,
  CHECK (from_unit_id <> to_unit_id)
) STRICT;
CREATE UNIQUE INDEX ux_unit_conversions ON unit_conversions (coalesce(product_id, ''), from_unit_id, to_unit_id)
  WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_unit_conversions_barcode ON unit_conversions (barcode)
  WHERE barcode IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE price_lists (
  id                  TEXT PRIMARY KEY,
  name                TEXT NOT NULL,
  prices_include_tax  INTEGER NOT NULL DEFAULT 1 CHECK (prices_include_tax IN (0, 1)),
  is_default          INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  created_by          TEXT REFERENCES users (id),
  updated_by          TEXT REFERENCES users (id),
  deleted_at          TEXT
) STRICT;
CREATE UNIQUE INDEX ux_price_lists_name ON price_lists (lower(name)) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_price_lists_default ON price_lists (is_default) WHERE is_default = 1 AND deleted_at IS NULL;

CREATE TABLE price_list_items (
  id             TEXT PRIMARY KEY,
  price_list_id  TEXT NOT NULL REFERENCES price_lists (id),
  variant_id     TEXT NOT NULL REFERENCES product_variants (id),
  unit_id        TEXT REFERENCES units (id),
  min_qty        INTEGER NOT NULL DEFAULT 0 CHECK (min_qty >= 0),
  price          INTEGER NOT NULL CHECK (price >= 0),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  updated_by     TEXT REFERENCES users (id),
  deleted_at     TEXT
) STRICT;
CREATE UNIQUE INDEX ux_price_list_items ON price_list_items (price_list_id, variant_id, coalesce(unit_id, ''), min_qty)
  WHERE deleted_at IS NULL;
CREATE INDEX ix_price_list_items_variant ON price_list_items (variant_id);

CREATE TABLE batches (
  id           TEXT PRIMARY KEY,
  variant_id   TEXT NOT NULL REFERENCES product_variants (id),
  batch_no     TEXT NOT NULL,
  mfg_date     TEXT,
  expiry_date  TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  created_by   TEXT REFERENCES users (id),
  updated_by   TEXT REFERENCES users (id),
  deleted_at   TEXT
) STRICT;
CREATE UNIQUE INDEX ux_batches ON batches (variant_id, lower(batch_no)) WHERE deleted_at IS NULL;
CREATE INDEX ix_batches_expiry ON batches (expiry_date) WHERE expiry_date IS NOT NULL;

-- The stock ledger: the source of truth for stock. Append-only; corrections are new rows
-- (reversal_of points at the corrected row). qty is signed, in thousandths of the base unit.
CREATE TABLE stock_movements (
  id              TEXT PRIMARY KEY,
  business_date   TEXT NOT NULL,
  occurred_at     TEXT NOT NULL,
  warehouse_id    TEXT NOT NULL REFERENCES warehouses (id),
  variant_id      TEXT NOT NULL REFERENCES product_variants (id),
  batch_id        TEXT REFERENCES batches (id),
  movement_type   TEXT NOT NULL CHECK (movement_type IN (
                    'opening', 'sale', 'sale_return', 'purchase', 'purchase_return',
                    'adjustment_in', 'adjustment_out', 'transfer_in', 'transfer_out',
                    'count_variance', 'void_reversal')),
  qty             INTEGER NOT NULL CHECK (qty <> 0),
  unit_cost       INTEGER NOT NULL CHECK (unit_cost >= 0),
  -- Signed value of the movement (qty × unit_cost / 1000, rounded once by the service).
  value           INTEGER NOT NULL,
  source_type     TEXT NOT NULL,
  source_id       TEXT NOT NULL,
  source_line_id  TEXT,
  reversal_of     TEXT REFERENCES stock_movements (id),
  note            TEXT,
  created_at      TEXT NOT NULL,
  created_by      TEXT REFERENCES users (id),
  CHECK ((qty > 0 AND value >= 0) OR (qty < 0 AND value <= 0)),
  CHECK (movement_type NOT IN ('sale', 'purchase_return', 'adjustment_out', 'transfer_out') OR qty < 0),
  CHECK (movement_type NOT IN ('sale_return', 'purchase', 'adjustment_in', 'transfer_in') OR qty > 0)
) STRICT;
CREATE INDEX ix_stock_movements_variant ON stock_movements (variant_id, warehouse_id, business_date);
CREATE INDEX ix_stock_movements_source ON stock_movements (source_type, source_id);
CREATE INDEX ix_stock_movements_date ON stock_movements (business_date);
CREATE INDEX ix_stock_movements_batch ON stock_movements (batch_id) WHERE batch_id IS NOT NULL;

-- Current stock: a cache of SUM(stock_movements) kept in step by the trigger below and
-- verifiable with v_stock_level_mismatches. batch_key is '' when the row has no batch.
CREATE TABLE stock_levels (
  variant_id    TEXT NOT NULL REFERENCES product_variants (id),
  warehouse_id  TEXT NOT NULL REFERENCES warehouses (id),
  batch_key     TEXT NOT NULL DEFAULT '',
  batch_id      TEXT REFERENCES batches (id),
  qty           INTEGER NOT NULL,
  value         INTEGER NOT NULL,
  updated_at    TEXT NOT NULL,
  PRIMARY KEY (variant_id, warehouse_id, batch_key),
  CHECK (batch_key = coalesce(batch_id, ''))
) STRICT, WITHOUT ROWID;
CREATE INDEX ix_stock_levels_warehouse ON stock_levels (warehouse_id);

CREATE TRIGGER trg_stock_movements_no_update BEFORE UPDATE ON stock_movements
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: stock_movements rows cannot be changed; post a reversal');
END;
CREATE TRIGGER trg_stock_movements_no_delete BEFORE DELETE ON stock_movements
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: stock_movements rows cannot be deleted; post a reversal');
END;
CREATE TRIGGER trg_stock_movements_level AFTER INSERT ON stock_movements
BEGIN
  INSERT INTO stock_levels (variant_id, warehouse_id, batch_key, batch_id, qty, value, updated_at)
  VALUES (NEW.variant_id, NEW.warehouse_id, coalesce(NEW.batch_id, ''), NEW.batch_id, NEW.qty, NEW.value, NEW.created_at)
  ON CONFLICT (variant_id, warehouse_id, batch_key) DO UPDATE SET
    qty = qty + excluded.qty,
    value = value + excluded.value,
    updated_at = excluded.updated_at;
END;

CREATE VIEW v_stock_level_mismatches AS
WITH ledger AS (
  SELECT variant_id, warehouse_id, coalesce(batch_id, '') AS batch_key,
         sum(qty) AS qty, sum(value) AS value
  FROM stock_movements
  GROUP BY variant_id, warehouse_id, coalesce(batch_id, '')
)
SELECT coalesce(l.variant_id, s.variant_id) AS variant_id,
       coalesce(l.warehouse_id, s.warehouse_id) AS warehouse_id,
       coalesce(l.batch_key, s.batch_key) AS batch_key,
       l.qty AS ledger_qty, s.qty AS cached_qty, l.value AS ledger_value, s.value AS cached_value
FROM ledger l
FULL OUTER JOIN stock_levels s
  ON s.variant_id = l.variant_id AND s.warehouse_id = l.warehouse_id AND s.batch_key = l.batch_key
WHERE l.qty IS NOT s.qty OR l.value IS NOT s.value;

-- Shared shape of the stock documents below: draft → finalized (posts stock_movements) → voided
-- (posts reversals). Finalized and voided documents are immutable apart from the void fields.
CREATE TABLE stock_adjustments (
  id             TEXT PRIMARY KEY,
  warehouse_id   TEXT NOT NULL REFERENCES warehouses (id),
  series_id      TEXT REFERENCES number_series (id),
  number_period  TEXT,
  doc_seq        INTEGER,
  doc_no         TEXT,
  business_date  TEXT NOT NULL,
  reason         TEXT NOT NULL,
  notes          TEXT,
  status         TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'finalized', 'voided')),
  approved_by    TEXT REFERENCES users (id),
  finalized_at   TEXT,
  finalized_by   TEXT REFERENCES users (id),
  voided_at      TEXT,
  voided_by      TEXT REFERENCES users (id),
  void_reason    TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  updated_by     TEXT REFERENCES users (id),
  deleted_at     TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX ux_stock_adjustments_doc_no ON stock_adjustments (doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX ix_stock_adjustments_date ON stock_adjustments (business_date);

CREATE TABLE stock_adjustment_items (
  id             TEXT PRIMARY KEY,
  adjustment_id  TEXT NOT NULL REFERENCES stock_adjustments (id) ON DELETE CASCADE,
  line_no        INTEGER NOT NULL CHECK (line_no >= 1),
  variant_id     TEXT NOT NULL REFERENCES product_variants (id),
  batch_id       TEXT REFERENCES batches (id),
  qty_change     INTEGER NOT NULL CHECK (qty_change <> 0),
  unit_cost      INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
  reason         TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  UNIQUE (adjustment_id, line_no)
) STRICT;
CREATE INDEX ix_stock_adjustment_items_variant ON stock_adjustment_items (variant_id);

CREATE TABLE stock_transfers (
  id                 TEXT PRIMARY KEY,
  from_warehouse_id  TEXT NOT NULL REFERENCES warehouses (id),
  to_warehouse_id    TEXT NOT NULL REFERENCES warehouses (id),
  series_id          TEXT REFERENCES number_series (id),
  number_period      TEXT,
  doc_seq            INTEGER,
  doc_no             TEXT,
  business_date      TEXT NOT NULL,
  status             TEXT NOT NULL DEFAULT 'draft'
                       CHECK (status IN ('draft', 'in_transit', 'received', 'voided')),
  notes              TEXT,
  dispatched_at      TEXT,
  dispatched_by      TEXT REFERENCES users (id),
  received_at        TEXT,
  received_by        TEXT REFERENCES users (id),
  voided_at          TEXT,
  voided_by          TEXT REFERENCES users (id),
  void_reason        TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  created_by         TEXT REFERENCES users (id),
  updated_by         TEXT REFERENCES users (id),
  deleted_at         TEXT,
  CHECK (from_warehouse_id <> to_warehouse_id),
  CHECK (status = 'draft' OR doc_no IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX ux_stock_transfers_doc_no ON stock_transfers (doc_no) WHERE doc_no IS NOT NULL;

CREATE TABLE stock_transfer_items (
  id           TEXT PRIMARY KEY,
  transfer_id  TEXT NOT NULL REFERENCES stock_transfers (id) ON DELETE CASCADE,
  line_no      INTEGER NOT NULL CHECK (line_no >= 1),
  variant_id   TEXT NOT NULL REFERENCES product_variants (id),
  batch_id     TEXT REFERENCES batches (id),
  qty          INTEGER NOT NULL CHECK (qty > 0),
  unit_cost    INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  UNIQUE (transfer_id, line_no)
) STRICT;

-- Physical stock-take. system_qty is frozen when counting starts; variance posts on finalize.
CREATE TABLE stock_counts (
  id             TEXT PRIMARY KEY,
  warehouse_id   TEXT NOT NULL REFERENCES warehouses (id),
  series_id      TEXT REFERENCES number_series (id),
  number_period  TEXT,
  doc_seq        INTEGER,
  doc_no         TEXT,
  business_date  TEXT NOT NULL,
  scope_note     TEXT,
  status         TEXT NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft', 'counting', 'finalized', 'voided')),
  finalized_at   TEXT,
  finalized_by   TEXT REFERENCES users (id),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  updated_by     TEXT REFERENCES users (id),
  deleted_at     TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX ux_stock_counts_doc_no ON stock_counts (doc_no) WHERE doc_no IS NOT NULL;

CREATE TABLE stock_count_lines (
  id            TEXT PRIMARY KEY,
  count_id      TEXT NOT NULL REFERENCES stock_counts (id) ON DELETE CASCADE,
  variant_id    TEXT NOT NULL REFERENCES product_variants (id),
  batch_id      TEXT REFERENCES batches (id),
  system_qty    INTEGER NOT NULL,
  counted_qty   INTEGER CHECK (counted_qty IS NULL OR counted_qty >= 0),
  variance_qty  INTEGER GENERATED ALWAYS AS (counted_qty - system_qty) VIRTUAL,
  unit_cost     INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
  counted_by    TEXT REFERENCES users (id),
  counted_at    TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
) STRICT;
CREATE UNIQUE INDEX ux_stock_count_lines ON stock_count_lines (count_id, variant_id, coalesce(batch_id, ''));

-- Lock finalized stock documents (shared rule; see 0003 for the billing documents).
CREATE TRIGGER trg_stock_adjustments_lock BEFORE UPDATE OF
  warehouse_id, doc_no, doc_seq, series_id, number_period, business_date, reason, finalized_at,
  finalized_by, created_at, created_by
ON stock_adjustments WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: stock adjustment is finalized');
END;
CREATE TRIGGER trg_stock_adjustments_status BEFORE UPDATE OF status ON stock_adjustments
WHEN NOT ((OLD.status = 'draft' AND NEW.status IN ('draft', 'finalized'))
       OR (OLD.status = 'finalized' AND NEW.status IN ('finalized', 'voided'))
       OR (OLD.status = 'voided' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: stock adjustment status change not allowed');
END;
CREATE TRIGGER trg_stock_adjustments_no_delete BEFORE DELETE ON stock_adjustments
WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: stock adjustment is finalized');
END;
CREATE TRIGGER trg_stock_adjustment_items_ins BEFORE INSERT ON stock_adjustment_items
WHEN (SELECT status FROM stock_adjustments WHERE id = NEW.adjustment_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: stock adjustment is finalized');
END;
CREATE TRIGGER trg_stock_adjustment_items_upd BEFORE UPDATE ON stock_adjustment_items
WHEN (SELECT status FROM stock_adjustments WHERE id = OLD.adjustment_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: stock adjustment is finalized');
END;
CREATE TRIGGER trg_stock_adjustment_items_del BEFORE DELETE ON stock_adjustment_items
WHEN (SELECT status FROM stock_adjustments WHERE id = OLD.adjustment_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: stock adjustment is finalized');
END;

-- Product search (POS lookup): FTS5 over name, SKU and barcode, kept in step by triggers.
-- Exact barcode / SKU hits use the unique indexes above; this serves prefix and word search.
CREATE VIRTUAL TABLE product_search USING fts5 (
  variant_id UNINDEXED,
  name,
  sku,
  barcode,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER trg_product_search_variant_ins AFTER INSERT ON product_variants
WHEN NEW.deleted_at IS NULL
BEGIN
  INSERT INTO product_search (variant_id, name, sku, barcode)
  SELECT NEW.id, p.name || coalesce(' ' || NEW.variant_name, ''), NEW.sku, coalesce(NEW.barcode, '')
  FROM products p WHERE p.id = NEW.product_id;
END;
CREATE TRIGGER trg_product_search_variant_upd AFTER UPDATE ON product_variants
BEGIN
  DELETE FROM product_search WHERE variant_id = OLD.id;
  INSERT INTO product_search (variant_id, name, sku, barcode)
  SELECT NEW.id, p.name || coalesce(' ' || NEW.variant_name, ''), NEW.sku, coalesce(NEW.barcode, '')
  FROM products p WHERE p.id = NEW.product_id AND NEW.deleted_at IS NULL;
END;
CREATE TRIGGER trg_product_search_variant_del AFTER DELETE ON product_variants
BEGIN
  DELETE FROM product_search WHERE variant_id = OLD.id;
END;
CREATE TRIGGER trg_product_search_product_upd AFTER UPDATE OF name ON products
BEGIN
  DELETE FROM product_search
  WHERE variant_id IN (SELECT id FROM product_variants WHERE product_id = NEW.id);
  INSERT INTO product_search (variant_id, name, sku, barcode)
  SELECT v.id, NEW.name || coalesce(' ' || v.variant_name, ''), v.sku, coalesce(v.barcode, '')
  FROM product_variants v WHERE v.product_id = NEW.id AND v.deleted_at IS NULL;
END;
