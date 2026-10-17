-- 0004 purchases: purchase orders, goods receipts (stock in), purchase bills (the payable, with
-- landed-cost charges) and purchase returns / debit notes.

CREATE TABLE purchase_orders (
  id             TEXT PRIMARY KEY,
  branch_id      TEXT NOT NULL REFERENCES branches (id),
  warehouse_id   TEXT NOT NULL REFERENCES warehouses (id),
  series_id      TEXT REFERENCES number_series (id),
  number_period  TEXT,
  doc_seq        INTEGER,
  doc_no         TEXT,
  business_date  TEXT NOT NULL,
  expected_date  TEXT,
  status         TEXT NOT NULL DEFAULT 'draft' CHECK (status IN (
                   'draft', 'open', 'partially_received', 'received', 'closed', 'cancelled')),
  party_id       TEXT NOT NULL REFERENCES parties (id),
  supplier_name  TEXT NOT NULL,
  gross          INTEGER NOT NULL DEFAULT 0 CHECK (gross >= 0),
  discount       INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable        INTEGER NOT NULL DEFAULT 0 CHECK (taxable >= 0),
  tax_total      INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  total          INTEGER NOT NULL DEFAULT 0 CHECK (total >= 0),
  notes          TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  updated_by     TEXT REFERENCES users (id),
  deleted_at     TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL),
  CHECK (total = taxable + tax_total)
) STRICT;
CREATE UNIQUE INDEX ux_purchase_orders_doc_no ON purchase_orders (doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX ix_purchase_orders_party ON purchase_orders (party_id, business_date);
CREATE INDEX ix_purchase_orders_status ON purchase_orders (status);

CREATE TABLE purchase_order_items (
  id                 TEXT PRIMARY KEY,
  purchase_order_id  TEXT NOT NULL REFERENCES purchase_orders (id) ON DELETE CASCADE,
  line_no            INTEGER NOT NULL CHECK (line_no >= 1),
  variant_id         TEXT NOT NULL REFERENCES product_variants (id),
  unit_id            TEXT NOT NULL REFERENCES units (id),
  unit_factor_num    INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_num > 0),
  unit_factor_den    INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_den > 0),
  item_name          TEXT NOT NULL,
  sku                TEXT,
  qty                INTEGER NOT NULL CHECK (qty > 0),
  base_qty           INTEGER NOT NULL CHECK (base_qty > 0),
  received_qty       INTEGER NOT NULL DEFAULT 0 CHECK (received_qty >= 0),
  unit_cost          INTEGER NOT NULL CHECK (unit_cost >= 0),
  discount           INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable            INTEGER NOT NULL CHECK (taxable >= 0),
  tax_rate           INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  tax_total          INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  line_total         INTEGER NOT NULL CHECK (line_total >= 0),
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  UNIQUE (purchase_order_id, line_no),
  CHECK (line_total = taxable + tax_total)
) STRICT;
CREATE INDEX ix_purchase_order_items_variant ON purchase_order_items (variant_id);

-- Goods receipt (GRN): brings stock in. Can come from a PO and/or be billed later.
CREATE TABLE goods_receipts (
  id                     TEXT PRIMARY KEY,
  branch_id              TEXT NOT NULL REFERENCES branches (id),
  warehouse_id           TEXT NOT NULL REFERENCES warehouses (id),
  series_id              TEXT REFERENCES number_series (id),
  number_period          TEXT,
  doc_seq                INTEGER,
  doc_no                 TEXT,
  business_date          TEXT NOT NULL,
  status                 TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'finalized', 'voided')),
  party_id               TEXT NOT NULL REFERENCES parties (id),
  purchase_order_id      TEXT REFERENCES purchase_orders (id),
  supplier_reference     TEXT,
  notes                  TEXT,
  finalized_at           TEXT,
  finalized_by           TEXT REFERENCES users (id),
  voided_at              TEXT,
  voided_by              TEXT REFERENCES users (id),
  void_reason            TEXT,
  created_at             TEXT NOT NULL,
  updated_at             TEXT NOT NULL,
  created_by             TEXT REFERENCES users (id),
  updated_by             TEXT REFERENCES users (id),
  deleted_at             TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX ux_goods_receipts_doc_no ON goods_receipts (doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX ix_goods_receipts_party ON goods_receipts (party_id, business_date);
CREATE INDEX ix_goods_receipts_po ON goods_receipts (purchase_order_id);

CREATE TABLE goods_receipt_items (
  id               TEXT PRIMARY KEY,
  receipt_id       TEXT NOT NULL REFERENCES goods_receipts (id) ON DELETE CASCADE,
  line_no          INTEGER NOT NULL CHECK (line_no >= 1),
  po_item_id       TEXT REFERENCES purchase_order_items (id),
  variant_id       TEXT NOT NULL REFERENCES product_variants (id),
  batch_id         TEXT REFERENCES batches (id),
  unit_id          TEXT NOT NULL REFERENCES units (id),
  unit_factor_num  INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_num > 0),
  unit_factor_den  INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_den > 0),
  item_name        TEXT NOT NULL,
  qty              INTEGER NOT NULL CHECK (qty > 0),
  base_qty         INTEGER NOT NULL CHECK (base_qty > 0),
  -- Cost per base unit including allocated landed costs.
  unit_cost        INTEGER NOT NULL CHECK (unit_cost >= 0),
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  UNIQUE (receipt_id, line_no)
) STRICT;
CREATE INDEX ix_goods_receipt_items_variant ON goods_receipt_items (variant_id);

-- Purchase bill: the supplier's invoice and our payable. receives_stock = 1 for a direct bill
-- without a separate GRN.
CREATE TABLE purchase_bills (
  id                     TEXT PRIMARY KEY,
  branch_id              TEXT NOT NULL REFERENCES branches (id),
  warehouse_id           TEXT NOT NULL REFERENCES warehouses (id),
  series_id              TEXT REFERENCES number_series (id),
  number_period          TEXT,
  doc_seq                INTEGER,
  doc_no                 TEXT,
  supplier_invoice_no    TEXT,
  supplier_invoice_date  TEXT,
  business_date          TEXT NOT NULL,
  due_date               TEXT,
  status                 TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'finalized', 'voided')),
  party_id               TEXT NOT NULL REFERENCES parties (id),
  supplier_name          TEXT NOT NULL,
  supplier_gstin         TEXT,
  place_of_supply        TEXT,
  is_inter_state         INTEGER NOT NULL DEFAULT 0 CHECK (is_inter_state IN (0, 1)),
  prices_include_tax     INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0, 1)),
  receives_stock         INTEGER NOT NULL DEFAULT 1 CHECK (receives_stock IN (0, 1)),
  purchase_order_id      TEXT REFERENCES purchase_orders (id),
  goods_receipt_id       TEXT REFERENCES goods_receipts (id),
  gross                  INTEGER NOT NULL DEFAULT 0 CHECK (gross >= 0),
  discount               INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable                INTEGER NOT NULL DEFAULT 0 CHECK (taxable >= 0),
  tax_total              INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  charges_total          INTEGER NOT NULL DEFAULT 0 CHECK (charges_total >= 0),
  round_off              INTEGER NOT NULL DEFAULT 0,
  total                  INTEGER NOT NULL DEFAULT 0 CHECK (total >= 0),
  amount_paid            INTEGER NOT NULL DEFAULT 0,
  balance_due            INTEGER GENERATED ALWAYS AS (total - amount_paid) VIRTUAL,
  payment_status         TEXT GENERATED ALWAYS AS (
                           CASE WHEN status = 'voided' THEN 'void'
                                WHEN amount_paid >= total THEN 'paid'
                                WHEN amount_paid > 0 THEN 'partial'
                                ELSE 'unpaid' END) VIRTUAL,
  notes                  TEXT,
  idempotency_key        TEXT UNIQUE,
  snapshot_source        TEXT NOT NULL DEFAULT 'live' CHECK (snapshot_source IN ('live', 'migrated')),
  finalized_at           TEXT,
  finalized_by           TEXT REFERENCES users (id),
  voided_at              TEXT,
  voided_by              TEXT REFERENCES users (id),
  void_reason            TEXT,
  created_at             TEXT NOT NULL,
  updated_at             TEXT NOT NULL,
  created_by             TEXT REFERENCES users (id),
  updated_by             TEXT REFERENCES users (id),
  deleted_at             TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL),
  CHECK (total = taxable + tax_total + charges_total + round_off),
  CHECK (amount_paid BETWEEN 0 AND total)
) STRICT;
CREATE UNIQUE INDEX ux_purchase_bills_doc_no ON purchase_bills (doc_no) WHERE doc_no IS NOT NULL;
-- The same supplier invoice can't be entered twice.
CREATE UNIQUE INDEX ux_purchase_bills_supplier_invoice ON purchase_bills (party_id, lower(supplier_invoice_no))
  WHERE supplier_invoice_no IS NOT NULL AND status <> 'voided';
CREATE INDEX ix_purchase_bills_date ON purchase_bills (business_date, status);
CREATE INDEX ix_purchase_bills_party ON purchase_bills (party_id, business_date);
CREATE INDEX ix_purchase_bills_payable ON purchase_bills (party_id)
  WHERE status = 'finalized' AND amount_paid < total;

CREATE TABLE purchase_bill_items (
  id               TEXT PRIMARY KEY,
  bill_id          TEXT NOT NULL REFERENCES purchase_bills (id) ON DELETE CASCADE,
  line_no          INTEGER NOT NULL CHECK (line_no >= 1),
  grn_item_id      TEXT REFERENCES goods_receipt_items (id),
  po_item_id       TEXT REFERENCES purchase_order_items (id),
  variant_id       TEXT NOT NULL REFERENCES product_variants (id),
  batch_id         TEXT REFERENCES batches (id),
  unit_id          TEXT NOT NULL REFERENCES units (id),
  unit_factor_num  INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_num > 0),
  unit_factor_den  INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_den > 0),
  item_name        TEXT NOT NULL,
  sku              TEXT,
  hsn_sac          TEXT,
  qty              INTEGER NOT NULL CHECK (qty > 0),
  base_qty         INTEGER NOT NULL CHECK (base_qty > 0),
  unit_cost        INTEGER NOT NULL CHECK (unit_cost >= 0),
  discount         INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable          INTEGER NOT NULL CHECK (taxable >= 0),
  tax_rate         INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  cgst             INTEGER NOT NULL DEFAULT 0 CHECK (cgst >= 0),
  sgst             INTEGER NOT NULL DEFAULT 0 CHECK (sgst >= 0),
  igst             INTEGER NOT NULL DEFAULT 0 CHECK (igst >= 0),
  other_tax        INTEGER NOT NULL DEFAULT 0 CHECK (other_tax >= 0),
  cess             INTEGER NOT NULL DEFAULT 0 CHECK (cess >= 0),
  tax_total        INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  line_total       INTEGER NOT NULL CHECK (line_total >= 0),
  -- Cost per base unit after landed-cost allocation (what goes into stock valuation).
  landed_unit_cost INTEGER NOT NULL DEFAULT 0 CHECK (landed_unit_cost >= 0),
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  UNIQUE (bill_id, line_no),
  CHECK (tax_total = cgst + sgst + igst + other_tax + cess),
  CHECK (line_total = taxable + tax_total)
) STRICT;
CREATE INDEX ix_purchase_bill_items_variant ON purchase_bill_items (variant_id);

-- Landed costs (freight, loading, insurance…) spread over the bill's lines.
CREATE TABLE purchase_bill_charges (
  id           TEXT PRIMARY KEY,
  bill_id      TEXT NOT NULL REFERENCES purchase_bills (id) ON DELETE CASCADE,
  description  TEXT NOT NULL,
  amount       INTEGER NOT NULL CHECK (amount >= 0),
  allocation   TEXT NOT NULL DEFAULT 'value' CHECK (allocation IN ('value', 'qty', 'none')),
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
) STRICT;
CREATE INDEX ix_purchase_bill_charges_bill ON purchase_bill_charges (bill_id);

CREATE TABLE purchase_returns (
  id                TEXT PRIMARY KEY,
  branch_id         TEXT NOT NULL REFERENCES branches (id),
  warehouse_id      TEXT NOT NULL REFERENCES warehouses (id),
  series_id         TEXT REFERENCES number_series (id),
  number_period     TEXT,
  doc_seq           INTEGER,
  doc_no            TEXT,
  business_date     TEXT NOT NULL,
  status            TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'finalized', 'voided')),
  original_bill_id  TEXT REFERENCES purchase_bills (id),
  party_id          TEXT NOT NULL REFERENCES parties (id),
  supplier_name     TEXT NOT NULL,
  supplier_gstin    TEXT,
  is_inter_state    INTEGER NOT NULL DEFAULT 0 CHECK (is_inter_state IN (0, 1)),
  reason            TEXT,
  gross             INTEGER NOT NULL DEFAULT 0 CHECK (gross >= 0),
  discount          INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable           INTEGER NOT NULL DEFAULT 0 CHECK (taxable >= 0),
  tax_total         INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  round_off         INTEGER NOT NULL DEFAULT 0,
  total             INTEGER NOT NULL DEFAULT 0 CHECK (total >= 0),
  amount_refunded   INTEGER NOT NULL DEFAULT 0,
  idempotency_key   TEXT UNIQUE,
  finalized_at      TEXT,
  finalized_by      TEXT REFERENCES users (id),
  voided_at         TEXT,
  voided_by         TEXT REFERENCES users (id),
  void_reason       TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  created_by        TEXT REFERENCES users (id),
  updated_by        TEXT REFERENCES users (id),
  deleted_at        TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL),
  CHECK (total = taxable + tax_total + round_off),
  CHECK (amount_refunded BETWEEN 0 AND total)
) STRICT;
CREATE UNIQUE INDEX ux_purchase_returns_doc_no ON purchase_returns (doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX ix_purchase_returns_party ON purchase_returns (party_id, business_date);
CREATE INDEX ix_purchase_returns_bill ON purchase_returns (original_bill_id);

CREATE TABLE purchase_return_items (
  id                TEXT PRIMARY KEY,
  return_id         TEXT NOT NULL REFERENCES purchase_returns (id) ON DELETE CASCADE,
  line_no           INTEGER NOT NULL CHECK (line_no >= 1),
  original_item_id  TEXT REFERENCES purchase_bill_items (id),
  variant_id        TEXT NOT NULL REFERENCES product_variants (id),
  batch_id          TEXT REFERENCES batches (id),
  unit_id           TEXT NOT NULL REFERENCES units (id),
  unit_factor_num   INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_num > 0),
  unit_factor_den   INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_den > 0),
  item_name         TEXT NOT NULL,
  qty               INTEGER NOT NULL CHECK (qty > 0),
  base_qty          INTEGER NOT NULL CHECK (base_qty > 0),
  unit_cost         INTEGER NOT NULL CHECK (unit_cost >= 0),
  taxable           INTEGER NOT NULL CHECK (taxable >= 0),
  tax_rate          INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  cgst              INTEGER NOT NULL DEFAULT 0 CHECK (cgst >= 0),
  sgst              INTEGER NOT NULL DEFAULT 0 CHECK (sgst >= 0),
  igst              INTEGER NOT NULL DEFAULT 0 CHECK (igst >= 0),
  other_tax         INTEGER NOT NULL DEFAULT 0 CHECK (other_tax >= 0),
  cess              INTEGER NOT NULL DEFAULT 0 CHECK (cess >= 0),
  tax_total         INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  line_total        INTEGER NOT NULL CHECK (line_total >= 0),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  UNIQUE (return_id, line_no),
  CHECK (tax_total = cgst + sgst + igst + other_tax + cess),
  CHECK (line_total = taxable + tax_total)
) STRICT;
CREATE INDEX ix_purchase_return_items_variant ON purchase_return_items (variant_id);

-- Lock finalized purchase documents.
CREATE TRIGGER trg_goods_receipts_lock BEFORE UPDATE OF
  branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, party_id,
  purchase_order_id, finalized_at, finalized_by, created_at, created_by
ON goods_receipts WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: goods receipt is finalized');
END;
CREATE TRIGGER trg_goods_receipts_status BEFORE UPDATE OF status ON goods_receipts
WHEN NOT ((OLD.status = 'draft' AND NEW.status IN ('draft', 'finalized'))
       OR (OLD.status = 'finalized' AND NEW.status IN ('finalized', 'voided'))
       OR (OLD.status = 'voided' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: goods receipt status change not allowed');
END;
CREATE TRIGGER trg_goods_receipts_no_delete BEFORE DELETE ON goods_receipts WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: goods receipt is finalized');
END;
CREATE TRIGGER trg_goods_receipt_items_ins BEFORE INSERT ON goods_receipt_items
WHEN (SELECT status FROM goods_receipts WHERE id = NEW.receipt_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: goods receipt is finalized');
END;
CREATE TRIGGER trg_goods_receipt_items_upd BEFORE UPDATE ON goods_receipt_items
WHEN (SELECT status FROM goods_receipts WHERE id = OLD.receipt_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: goods receipt is finalized');
END;
CREATE TRIGGER trg_goods_receipt_items_del BEFORE DELETE ON goods_receipt_items
WHEN (SELECT status FROM goods_receipts WHERE id = OLD.receipt_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: goods receipt is finalized');
END;

CREATE TRIGGER trg_purchase_bills_lock BEFORE UPDATE OF
  branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, supplier_invoice_no,
  supplier_invoice_date, business_date, party_id, supplier_name, supplier_gstin, place_of_supply,
  is_inter_state, prices_include_tax, receives_stock, gross, discount, taxable, tax_total,
  charges_total, round_off, total, snapshot_source, finalized_at, finalized_by, created_at,
  created_by
ON purchase_bills WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bills_status BEFORE UPDATE OF status ON purchase_bills
WHEN NOT ((OLD.status = 'draft' AND NEW.status IN ('draft', 'finalized'))
       OR (OLD.status = 'finalized' AND NEW.status IN ('finalized', 'voided'))
       OR (OLD.status = 'voided' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: purchase bill status change not allowed');
END;
CREATE TRIGGER trg_purchase_bills_no_delete BEFORE DELETE ON purchase_bills WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bill_items_ins BEFORE INSERT ON purchase_bill_items
WHEN (SELECT status FROM purchase_bills WHERE id = NEW.bill_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bill_items_upd BEFORE UPDATE ON purchase_bill_items
WHEN (SELECT status FROM purchase_bills WHERE id = OLD.bill_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bill_items_del BEFORE DELETE ON purchase_bill_items
WHEN (SELECT status FROM purchase_bills WHERE id = OLD.bill_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bill_charges_ins BEFORE INSERT ON purchase_bill_charges
WHEN (SELECT status FROM purchase_bills WHERE id = NEW.bill_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bill_charges_upd BEFORE UPDATE ON purchase_bill_charges
WHEN (SELECT status FROM purchase_bills WHERE id = OLD.bill_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;
CREATE TRIGGER trg_purchase_bill_charges_del BEFORE DELETE ON purchase_bill_charges
WHEN (SELECT status FROM purchase_bills WHERE id = OLD.bill_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase bill is finalized');
END;

CREATE TRIGGER trg_purchase_returns_lock BEFORE UPDATE OF
  branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date,
  original_bill_id, party_id, supplier_name, supplier_gstin, is_inter_state, gross, discount,
  taxable, tax_total, round_off, total, finalized_at, finalized_by, created_at, created_by
ON purchase_returns WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase return is finalized');
END;
CREATE TRIGGER trg_purchase_returns_status BEFORE UPDATE OF status ON purchase_returns
WHEN NOT ((OLD.status = 'draft' AND NEW.status IN ('draft', 'finalized'))
       OR (OLD.status = 'finalized' AND NEW.status IN ('finalized', 'voided'))
       OR (OLD.status = 'voided' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: purchase return status change not allowed');
END;
CREATE TRIGGER trg_purchase_returns_no_delete BEFORE DELETE ON purchase_returns WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase return is finalized');
END;
CREATE TRIGGER trg_purchase_return_items_ins BEFORE INSERT ON purchase_return_items
WHEN (SELECT status FROM purchase_returns WHERE id = NEW.return_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase return is finalized');
END;
CREATE TRIGGER trg_purchase_return_items_upd BEFORE UPDATE ON purchase_return_items
WHEN (SELECT status FROM purchase_returns WHERE id = OLD.return_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase return is finalized');
END;
CREATE TRIGGER trg_purchase_return_items_del BEFORE DELETE ON purchase_return_items
WHEN (SELECT status FROM purchase_returns WHERE id = OLD.return_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: purchase return is finalized');
END;
