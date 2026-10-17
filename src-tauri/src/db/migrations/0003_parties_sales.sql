-- 0003 parties and sales: customers/suppliers with a party ledger, sales invoices (immutable once
-- finalized, with line snapshots), sales returns / credit notes, quotations, orders and challans,
-- held POS bills, and the CRM pipeline.

CREATE TABLE parties (
  id                TEXT PRIMARY KEY,
  party_type        TEXT NOT NULL CHECK (party_type IN ('customer', 'supplier', 'both')),
  code              TEXT,
  name              TEXT NOT NULL,
  phone             TEXT,
  email             TEXT,
  gstin             TEXT,
  state_code        TEXT,
  billing_address   TEXT,
  shipping_address  TEXT,
  credit_limit      INTEGER CHECK (credit_limit IS NULL OR credit_limit >= 0),
  credit_days       INTEGER CHECK (credit_days IS NULL OR credit_days >= 0),
  price_list_id     TEXT REFERENCES price_lists (id),
  -- Cache of SUM(party_ledger.debit - credit): > 0 the party owes us, < 0 we owe the party.
  balance           INTEGER NOT NULL DEFAULT 0,
  notes             TEXT,
  is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  created_by        TEXT REFERENCES users (id),
  updated_by        TEXT REFERENCES users (id),
  deleted_at        TEXT
) STRICT;
CREATE INDEX ix_parties_name ON parties (lower(name));
CREATE INDEX ix_parties_phone ON parties (phone) WHERE phone IS NOT NULL;
CREATE UNIQUE INDEX ux_parties_code ON parties (lower(code)) WHERE code IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX ix_parties_gstin ON parties (gstin) WHERE gstin IS NOT NULL;

-- Party ledger: every receivable / payable change. Append-only; balance cache kept by trigger.
CREATE TABLE party_ledger (
  id             TEXT PRIMARY KEY,
  party_id       TEXT NOT NULL REFERENCES parties (id),
  business_date  TEXT NOT NULL,
  entry_type     TEXT NOT NULL CHECK (entry_type IN (
                   'opening', 'sales_invoice', 'sales_return', 'payment_in', 'payment_out',
                   'purchase_bill', 'purchase_return', 'expense', 'adjustment', 'void_reversal')),
  source_type    TEXT NOT NULL,
  source_id      TEXT NOT NULL,
  debit          INTEGER NOT NULL DEFAULT 0 CHECK (debit >= 0),
  credit         INTEGER NOT NULL DEFAULT 0 CHECK (credit >= 0),
  narration      TEXT,
  created_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id),
  CHECK ((debit = 0) <> (credit = 0))
) STRICT;
CREATE INDEX ix_party_ledger_party ON party_ledger (party_id, business_date);
CREATE INDEX ix_party_ledger_source ON party_ledger (source_type, source_id);

CREATE TRIGGER trg_party_ledger_no_update BEFORE UPDATE ON party_ledger
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: party_ledger rows cannot be changed; post a reversal');
END;
CREATE TRIGGER trg_party_ledger_no_delete BEFORE DELETE ON party_ledger
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: party_ledger rows cannot be deleted; post a reversal');
END;
CREATE TRIGGER trg_party_ledger_balance AFTER INSERT ON party_ledger
BEGIN
  UPDATE parties SET balance = balance + NEW.debit - NEW.credit WHERE id = NEW.party_id;
END;

CREATE VIEW v_party_balance_mismatches AS
SELECT p.id AS party_id, p.name, p.balance AS cached_balance,
       coalesce(sum(l.debit - l.credit), 0) AS ledger_balance
FROM parties p
LEFT JOIN party_ledger l ON l.party_id = p.id
GROUP BY p.id
HAVING p.balance <> coalesce(sum(l.debit - l.credit), 0);

-- Sales invoice. Draft → finalized (number assigned, stock/ledger/journal posted) → voided.
-- Finalized invoices are immutable (triggers below); customer and item details are snapshots.
CREATE TABLE sales_invoices (
  id                  TEXT PRIMARY KEY,
  branch_id           TEXT NOT NULL REFERENCES branches (id),
  warehouse_id        TEXT NOT NULL REFERENCES warehouses (id),
  register_session_id TEXT REFERENCES register_sessions (id),
  device_id           TEXT REFERENCES devices (id),
  series_id           TEXT REFERENCES number_series (id),
  number_period       TEXT,
  doc_seq             INTEGER CHECK (doc_seq IS NULL OR doc_seq >= 1),
  doc_no              TEXT,
  business_date       TEXT NOT NULL,
  due_date            TEXT,
  status              TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'finalized', 'voided')),
  party_id            TEXT REFERENCES parties (id),
  customer_name       TEXT NOT NULL,
  customer_phone      TEXT,
  customer_gstin      TEXT,
  billing_address     TEXT,
  shipping_address    TEXT,
  place_of_supply     TEXT,
  is_inter_state      INTEGER NOT NULL DEFAULT 0 CHECK (is_inter_state IN (0, 1)),
  price_list_id       TEXT REFERENCES price_lists (id),
  prices_include_tax  INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0, 1)),
  currency            TEXT NOT NULL DEFAULT 'INR',
  gross               INTEGER NOT NULL DEFAULT 0 CHECK (gross >= 0),
  line_discount       INTEGER NOT NULL DEFAULT 0 CHECK (line_discount >= 0),
  bill_discount       INTEGER NOT NULL DEFAULT 0 CHECK (bill_discount >= 0),
  taxable             INTEGER NOT NULL DEFAULT 0 CHECK (taxable >= 0),
  tax_total           INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  round_off           INTEGER NOT NULL DEFAULT 0,
  total               INTEGER NOT NULL DEFAULT 0 CHECK (total >= 0),
  -- Cache of payment allocations, maintained by trigger.
  amount_paid         INTEGER NOT NULL DEFAULT 0,
  balance_due         INTEGER GENERATED ALWAYS AS (total - amount_paid) VIRTUAL,
  payment_status      TEXT GENERATED ALWAYS AS (
                        CASE WHEN status = 'voided' THEN 'void'
                             WHEN amount_paid >= total THEN 'paid'
                             WHEN amount_paid > 0 THEN 'partial'
                             ELSE 'unpaid' END) VIRTUAL,
  source_document_id  TEXT REFERENCES sales_documents (id),
  notes               TEXT,
  terms               TEXT,
  idempotency_key     TEXT UNIQUE,
  snapshot_source     TEXT NOT NULL DEFAULT 'live' CHECK (snapshot_source IN ('live', 'migrated')),
  finalized_at        TEXT,
  finalized_by        TEXT REFERENCES users (id),
  voided_at           TEXT,
  voided_by           TEXT REFERENCES users (id),
  void_reason         TEXT,
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  created_by          TEXT REFERENCES users (id),
  updated_by          TEXT REFERENCES users (id),
  deleted_at          TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL),
  CHECK (total = taxable + tax_total + round_off),
  CHECK (amount_paid BETWEEN 0 AND total),
  CHECK (status <> 'voided' OR voided_at IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX ux_sales_invoices_doc_no ON sales_invoices (doc_no) WHERE doc_no IS NOT NULL;
CREATE UNIQUE INDEX ux_sales_invoices_seq ON sales_invoices (series_id, number_period, doc_seq)
  WHERE series_id IS NOT NULL AND doc_seq IS NOT NULL;
CREATE INDEX ix_sales_invoices_date ON sales_invoices (business_date, status);
CREATE INDEX ix_sales_invoices_party ON sales_invoices (party_id, business_date);
CREATE INDEX ix_sales_invoices_branch_date ON sales_invoices (branch_id, business_date);
CREATE INDEX ix_sales_invoices_receivable ON sales_invoices (party_id)
  WHERE status = 'finalized' AND amount_paid < total;

CREATE TABLE sales_invoice_items (
  id                   TEXT PRIMARY KEY,
  invoice_id           TEXT NOT NULL REFERENCES sales_invoices (id) ON DELETE CASCADE,
  line_no              INTEGER NOT NULL CHECK (line_no >= 1),
  variant_id           TEXT NOT NULL REFERENCES product_variants (id),
  batch_id             TEXT REFERENCES batches (id),
  unit_id              TEXT NOT NULL REFERENCES units (id),
  unit_factor_num      INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_num > 0),
  unit_factor_den      INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_den > 0),
  -- Snapshot at the time of sale: later product edits never change this invoice.
  item_name            TEXT NOT NULL,
  sku                  TEXT,
  hsn_sac              TEXT,
  unit_name            TEXT,
  qty                  INTEGER NOT NULL CHECK (qty > 0),
  base_qty             INTEGER NOT NULL CHECK (base_qty > 0),
  unit_price           INTEGER NOT NULL CHECK (unit_price >= 0),
  mrp                  INTEGER,
  line_discount        INTEGER NOT NULL DEFAULT 0 CHECK (line_discount >= 0),
  bill_discount_share  INTEGER NOT NULL DEFAULT 0 CHECK (bill_discount_share >= 0),
  taxable              INTEGER NOT NULL CHECK (taxable >= 0),
  tax_rate_id          TEXT REFERENCES tax_rates (id),
  tax_rate             INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  cgst                 INTEGER NOT NULL DEFAULT 0 CHECK (cgst >= 0),
  sgst                 INTEGER NOT NULL DEFAULT 0 CHECK (sgst >= 0),
  igst                 INTEGER NOT NULL DEFAULT 0 CHECK (igst >= 0),
  other_tax            INTEGER NOT NULL DEFAULT 0 CHECK (other_tax >= 0),
  cess                 INTEGER NOT NULL DEFAULT 0 CHECK (cess >= 0),
  tax_total            INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  line_total           INTEGER NOT NULL CHECK (line_total >= 0),
  -- Cost of goods per base unit at the time of sale (for item profit and COGS).
  unit_cost            INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  UNIQUE (invoice_id, line_no),
  CHECK (tax_total = cgst + sgst + igst + other_tax + cess),
  CHECK (line_total = taxable + tax_total)
) STRICT;
CREATE INDEX ix_sales_invoice_items_variant ON sales_invoice_items (variant_id);

CREATE TRIGGER trg_sales_invoices_lock BEFORE UPDATE OF
  branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date, party_id,
  customer_name, customer_phone, customer_gstin, billing_address, shipping_address,
  place_of_supply, is_inter_state, prices_include_tax, currency, gross, line_discount,
  bill_discount, taxable, tax_total, round_off, total, snapshot_source, finalized_at,
  finalized_by, created_at, created_by
ON sales_invoices WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales invoice is finalized');
END;
CREATE TRIGGER trg_sales_invoices_status BEFORE UPDATE OF status ON sales_invoices
WHEN NOT ((OLD.status = 'draft' AND NEW.status IN ('draft', 'finalized'))
       OR (OLD.status = 'finalized' AND NEW.status IN ('finalized', 'voided'))
       OR (OLD.status = 'voided' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: sales invoice status change not allowed');
END;
CREATE TRIGGER trg_sales_invoices_no_delete BEFORE DELETE ON sales_invoices
WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales invoice is finalized');
END;
CREATE TRIGGER trg_sales_invoice_items_ins BEFORE INSERT ON sales_invoice_items
WHEN (SELECT status FROM sales_invoices WHERE id = NEW.invoice_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales invoice is finalized');
END;
CREATE TRIGGER trg_sales_invoice_items_upd BEFORE UPDATE ON sales_invoice_items
WHEN (SELECT status FROM sales_invoices WHERE id = OLD.invoice_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales invoice is finalized');
END;
CREATE TRIGGER trg_sales_invoice_items_del BEFORE DELETE ON sales_invoice_items
WHEN (SELECT status FROM sales_invoices WHERE id = OLD.invoice_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales invoice is finalized');
END;

-- Sales return / credit note. restock = 1 puts goods back into stock.
CREATE TABLE sales_returns (
  id                   TEXT PRIMARY KEY,
  branch_id            TEXT NOT NULL REFERENCES branches (id),
  warehouse_id         TEXT NOT NULL REFERENCES warehouses (id),
  series_id            TEXT REFERENCES number_series (id),
  number_period        TEXT,
  doc_seq              INTEGER,
  doc_no               TEXT,
  business_date        TEXT NOT NULL,
  status               TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'finalized', 'voided')),
  original_invoice_id  TEXT REFERENCES sales_invoices (id),
  party_id             TEXT REFERENCES parties (id),
  customer_name        TEXT NOT NULL,
  customer_phone       TEXT,
  customer_gstin       TEXT,
  place_of_supply      TEXT,
  is_inter_state       INTEGER NOT NULL DEFAULT 0 CHECK (is_inter_state IN (0, 1)),
  reason               TEXT,
  gross                INTEGER NOT NULL DEFAULT 0 CHECK (gross >= 0),
  discount             INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable              INTEGER NOT NULL DEFAULT 0 CHECK (taxable >= 0),
  tax_total            INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  round_off            INTEGER NOT NULL DEFAULT 0,
  total                INTEGER NOT NULL DEFAULT 0 CHECK (total >= 0),
  amount_refunded      INTEGER NOT NULL DEFAULT 0,
  idempotency_key      TEXT UNIQUE,
  finalized_at         TEXT,
  finalized_by         TEXT REFERENCES users (id),
  voided_at            TEXT,
  voided_by            TEXT REFERENCES users (id),
  void_reason          TEXT,
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  created_by           TEXT REFERENCES users (id),
  updated_by           TEXT REFERENCES users (id),
  deleted_at           TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL),
  CHECK (total = taxable + tax_total + round_off),
  CHECK (amount_refunded BETWEEN 0 AND total)
) STRICT;
CREATE UNIQUE INDEX ux_sales_returns_doc_no ON sales_returns (doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX ix_sales_returns_date ON sales_returns (business_date, status);
CREATE INDEX ix_sales_returns_invoice ON sales_returns (original_invoice_id);
CREATE INDEX ix_sales_returns_party ON sales_returns (party_id, business_date);

CREATE TABLE sales_return_items (
  id                TEXT PRIMARY KEY,
  return_id         TEXT NOT NULL REFERENCES sales_returns (id) ON DELETE CASCADE,
  line_no           INTEGER NOT NULL CHECK (line_no >= 1),
  original_item_id  TEXT REFERENCES sales_invoice_items (id),
  variant_id        TEXT NOT NULL REFERENCES product_variants (id),
  batch_id          TEXT REFERENCES batches (id),
  unit_id           TEXT NOT NULL REFERENCES units (id),
  unit_factor_num   INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_num > 0),
  unit_factor_den   INTEGER NOT NULL DEFAULT 1 CHECK (unit_factor_den > 0),
  item_name         TEXT NOT NULL,
  sku               TEXT,
  hsn_sac           TEXT,
  qty               INTEGER NOT NULL CHECK (qty > 0),
  base_qty          INTEGER NOT NULL CHECK (base_qty > 0),
  unit_price        INTEGER NOT NULL CHECK (unit_price >= 0),
  discount          INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable           INTEGER NOT NULL CHECK (taxable >= 0),
  tax_rate          INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  cgst              INTEGER NOT NULL DEFAULT 0 CHECK (cgst >= 0),
  sgst              INTEGER NOT NULL DEFAULT 0 CHECK (sgst >= 0),
  igst              INTEGER NOT NULL DEFAULT 0 CHECK (igst >= 0),
  other_tax         INTEGER NOT NULL DEFAULT 0 CHECK (other_tax >= 0),
  cess              INTEGER NOT NULL DEFAULT 0 CHECK (cess >= 0),
  tax_total         INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  line_total        INTEGER NOT NULL CHECK (line_total >= 0),
  unit_cost         INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
  restock           INTEGER NOT NULL DEFAULT 1 CHECK (restock IN (0, 1)),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  UNIQUE (return_id, line_no),
  CHECK (tax_total = cgst + sgst + igst + other_tax + cess),
  CHECK (line_total = taxable + tax_total)
) STRICT;
CREATE INDEX ix_sales_return_items_variant ON sales_return_items (variant_id);
CREATE INDEX ix_sales_return_items_original ON sales_return_items (original_item_id);

CREATE TRIGGER trg_sales_returns_lock BEFORE UPDATE OF
  branch_id, warehouse_id, series_id, number_period, doc_seq, doc_no, business_date,
  original_invoice_id, party_id, customer_name, customer_gstin, place_of_supply, is_inter_state,
  gross, discount, taxable, tax_total, round_off, total, finalized_at, finalized_by, created_at,
  created_by
ON sales_returns WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales return is finalized');
END;
CREATE TRIGGER trg_sales_returns_status BEFORE UPDATE OF status ON sales_returns
WHEN NOT ((OLD.status = 'draft' AND NEW.status IN ('draft', 'finalized'))
       OR (OLD.status = 'finalized' AND NEW.status IN ('finalized', 'voided'))
       OR (OLD.status = 'voided' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: sales return status change not allowed');
END;
CREATE TRIGGER trg_sales_returns_no_delete BEFORE DELETE ON sales_returns
WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales return is finalized');
END;
CREATE TRIGGER trg_sales_return_items_ins BEFORE INSERT ON sales_return_items
WHEN (SELECT status FROM sales_returns WHERE id = NEW.return_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales return is finalized');
END;
CREATE TRIGGER trg_sales_return_items_upd BEFORE UPDATE ON sales_return_items
WHEN (SELECT status FROM sales_returns WHERE id = OLD.return_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales return is finalized');
END;
CREATE TRIGGER trg_sales_return_items_del BEFORE DELETE ON sales_return_items
WHEN (SELECT status FROM sales_returns WHERE id = OLD.return_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: sales return is finalized');
END;

-- Quotations, sales orders and delivery challans share one shape. They never touch the
-- ledgers; converting one creates a sales invoice (converted_invoice_id).
CREATE TABLE sales_documents (
  id                    TEXT PRIMARY KEY,
  doc_type              TEXT NOT NULL CHECK (doc_type IN ('quotation', 'sales_order', 'delivery_challan')),
  branch_id             TEXT NOT NULL REFERENCES branches (id),
  series_id             TEXT REFERENCES number_series (id),
  number_period         TEXT,
  doc_seq               INTEGER,
  doc_no                TEXT,
  business_date         TEXT NOT NULL,
  valid_until           TEXT,
  status                TEXT NOT NULL DEFAULT 'draft' CHECK (status IN (
                          'draft', 'open', 'accepted', 'declined', 'delivered', 'converted', 'cancelled')),
  party_id              TEXT REFERENCES parties (id),
  customer_name         TEXT NOT NULL,
  customer_phone        TEXT,
  customer_gstin        TEXT,
  billing_address       TEXT,
  shipping_address      TEXT,
  prices_include_tax    INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0, 1)),
  gross                 INTEGER NOT NULL DEFAULT 0 CHECK (gross >= 0),
  discount              INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable               INTEGER NOT NULL DEFAULT 0 CHECK (taxable >= 0),
  tax_total             INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  round_off             INTEGER NOT NULL DEFAULT 0,
  total                 INTEGER NOT NULL DEFAULT 0 CHECK (total >= 0),
  source_document_id    TEXT REFERENCES sales_documents (id),
  converted_invoice_id  TEXT REFERENCES sales_invoices (id),
  notes                 TEXT,
  terms                 TEXT,
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  created_by            TEXT REFERENCES users (id),
  updated_by            TEXT REFERENCES users (id),
  deleted_at            TEXT,
  CHECK (status = 'draft' OR doc_no IS NOT NULL),
  CHECK (total = taxable + tax_total + round_off),
  CHECK (status <> 'converted' OR converted_invoice_id IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX ux_sales_documents_doc_no ON sales_documents (doc_type, doc_no) WHERE doc_no IS NOT NULL;
CREATE INDEX ix_sales_documents_type_date ON sales_documents (doc_type, business_date);
CREATE INDEX ix_sales_documents_party ON sales_documents (party_id);

CREATE TABLE sales_document_items (
  id              TEXT PRIMARY KEY,
  document_id     TEXT NOT NULL REFERENCES sales_documents (id) ON DELETE CASCADE,
  line_no         INTEGER NOT NULL CHECK (line_no >= 1),
  variant_id      TEXT NOT NULL REFERENCES product_variants (id),
  unit_id         TEXT NOT NULL REFERENCES units (id),
  item_name       TEXT NOT NULL,
  sku             TEXT,
  qty             INTEGER NOT NULL CHECK (qty > 0),
  base_qty        INTEGER NOT NULL CHECK (base_qty > 0),
  unit_price      INTEGER NOT NULL CHECK (unit_price >= 0),
  discount        INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
  taxable         INTEGER NOT NULL CHECK (taxable >= 0),
  tax_rate        INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  tax_total       INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  line_total      INTEGER NOT NULL CHECK (line_total >= 0),
  delivered_qty   INTEGER NOT NULL DEFAULT 0 CHECK (delivered_qty >= 0),
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  UNIQUE (document_id, line_no),
  CHECK (line_total = taxable + tax_total)
) STRICT;
CREATE INDEX ix_sales_document_items_variant ON sales_document_items (variant_id);

-- Held / parked POS bills. Recalling a bill soft-deletes it so it can't be billed twice.
CREATE TABLE held_bills (
  id                   TEXT PRIMARY KEY,
  device_id            TEXT REFERENCES devices (id),
  register_session_id  TEXT REFERENCES register_sessions (id),
  label                TEXT NOT NULL,
  party_id             TEXT REFERENCES parties (id),
  cart_json            TEXT NOT NULL CHECK (json_valid(cart_json)),
  cart_format          TEXT NOT NULL DEFAULT 'v2' CHECK (cart_format IN ('v2', 'legacy-v1')),
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  created_by           TEXT REFERENCES users (id),
  updated_by           TEXT REFERENCES users (id),
  deleted_at           TEXT
) STRICT;
CREATE INDEX ix_held_bills_open ON held_bills (created_at) WHERE deleted_at IS NULL;

-- CRM pipeline (kept from v1; docs/ASSUMPTIONS.md A-14).
CREATE TABLE leads (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  contact_name  TEXT,
  phone         TEXT,
  email         TEXT,
  stage         TEXT NOT NULL DEFAULT 'new'
                  CHECK (stage IN ('new', 'contacted', 'proposal', 'negotiation', 'won', 'lost')),
  deal_value    INTEGER NOT NULL DEFAULT 0 CHECK (deal_value >= 0),
  notes         TEXT,
  party_id      TEXT REFERENCES parties (id),
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  updated_by    TEXT REFERENCES users (id),
  deleted_at    TEXT
) STRICT;
CREATE INDEX ix_leads_stage ON leads (stage) WHERE deleted_at IS NULL;
