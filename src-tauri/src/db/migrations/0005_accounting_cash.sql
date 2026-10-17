-- 0005 accounting-lite and cash: chart of accounts, double-entry journal, payments with
-- allocations to documents, expenses, cash registers, register sessions and day close.

CREATE TABLE accounts (
  id               TEXT PRIMARY KEY,
  code             TEXT NOT NULL,
  name             TEXT NOT NULL,
  account_type     TEXT NOT NULL CHECK (account_type IN ('asset', 'liability', 'equity', 'income', 'expense')),
  subtype          TEXT NOT NULL DEFAULT 'other' CHECK (subtype IN (
                     'cash', 'bank', 'receivable', 'payable', 'inventory', 'tax_input',
                     'tax_output', 'sales', 'sales_returns', 'cogs', 'round_off', 'discount',
                     'stock_adjustment', 'expense', 'equity', 'other')),
  parent_id        TEXT REFERENCES accounts (id),
  is_system        INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
  is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  bank_name        TEXT,
  bank_account_no  TEXT,
  bank_ifsc        TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  created_by       TEXT REFERENCES users (id),
  updated_by       TEXT REFERENCES users (id),
  deleted_at       TEXT,
  CHECK (parent_id IS NULL OR parent_id <> id)
) STRICT;
CREATE UNIQUE INDEX ux_accounts_code ON accounts (code) WHERE deleted_at IS NULL;
CREATE INDEX ix_accounts_subtype ON accounts (subtype);

-- Journal entries are posted by the services in the same transaction as the business document.
-- Never edited; a mistake is corrected by a reversing entry (reversal_of).
CREATE TABLE journal_entries (
  id             TEXT PRIMARY KEY,
  business_date  TEXT NOT NULL,
  source_type    TEXT NOT NULL,
  source_id      TEXT,
  narration      TEXT NOT NULL,
  status         TEXT NOT NULL DEFAULT 'posted' CHECK (status IN ('posted', 'reversed')),
  reversal_of    TEXT REFERENCES journal_entries (id),
  created_at     TEXT NOT NULL,
  created_by     TEXT REFERENCES users (id)
) STRICT;
CREATE INDEX ix_journal_entries_date ON journal_entries (business_date);
CREATE INDEX ix_journal_entries_source ON journal_entries (source_type, source_id);

CREATE TABLE journal_lines (
  id          TEXT PRIMARY KEY,
  entry_id    TEXT NOT NULL REFERENCES journal_entries (id),
  line_no     INTEGER NOT NULL CHECK (line_no >= 1),
  account_id  TEXT NOT NULL REFERENCES accounts (id),
  party_id    TEXT REFERENCES parties (id),
  debit       INTEGER NOT NULL DEFAULT 0 CHECK (debit >= 0),
  credit      INTEGER NOT NULL DEFAULT 0 CHECK (credit >= 0),
  memo        TEXT,
  created_at  TEXT NOT NULL,
  UNIQUE (entry_id, line_no),
  CHECK ((debit = 0) <> (credit = 0))
) STRICT;
CREATE INDEX ix_journal_lines_account ON journal_lines (account_id, entry_id);
CREATE INDEX ix_journal_lines_party ON journal_lines (party_id) WHERE party_id IS NOT NULL;

CREATE TRIGGER trg_journal_entries_lock BEFORE UPDATE OF
  business_date, source_type, source_id, narration, reversal_of, created_at, created_by
ON journal_entries
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: journal entries cannot be changed; post a reversal');
END;
CREATE TRIGGER trg_journal_entries_status BEFORE UPDATE OF status ON journal_entries
WHEN NOT (OLD.status = NEW.status OR (OLD.status = 'posted' AND NEW.status = 'reversed'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: journal entry status change not allowed');
END;
CREATE TRIGGER trg_journal_entries_no_delete BEFORE DELETE ON journal_entries
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: journal entries cannot be deleted; post a reversal');
END;
CREATE TRIGGER trg_journal_lines_no_update BEFORE UPDATE ON journal_lines
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: journal lines cannot be changed; post a reversal');
END;
CREATE TRIGGER trg_journal_lines_no_delete BEFORE DELETE ON journal_lines
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: journal lines cannot be deleted; post a reversal');
END;

-- Entries whose debits and credits don't match (or have no lines). Must always be empty for
-- entries posted by v2; migrated legacy entries that were already unbalanced are reported.
CREATE VIEW v_unbalanced_journal_entries AS
SELECT e.id AS entry_id, e.business_date, e.source_type, e.narration,
       coalesce(sum(l.debit), 0) AS total_debit, coalesce(sum(l.credit), 0) AS total_credit
FROM journal_entries e
LEFT JOIN journal_lines l ON l.entry_id = e.id
GROUP BY e.id
HAVING coalesce(sum(l.debit), 0) <> coalesce(sum(l.credit), 0) OR count(l.id) = 0;

CREATE TABLE cash_registers (
  id               TEXT PRIMARY KEY,
  branch_id        TEXT NOT NULL REFERENCES branches (id),
  name             TEXT NOT NULL,
  device_id        TEXT REFERENCES devices (id),
  cash_account_id  TEXT NOT NULL REFERENCES accounts (id),
  is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  created_by       TEXT REFERENCES users (id),
  updated_by       TEXT REFERENCES users (id),
  deleted_at       TEXT
) STRICT;
CREATE UNIQUE INDEX ux_cash_registers_name ON cash_registers (branch_id, lower(name)) WHERE deleted_at IS NULL;

-- A cashier's shift on a register: opening float in, counted cash out.
CREATE TABLE register_sessions (
  id              TEXT PRIMARY KEY,
  register_id     TEXT NOT NULL REFERENCES cash_registers (id),
  status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
  opened_at       TEXT NOT NULL,
  opened_by       TEXT NOT NULL REFERENCES users (id),
  opening_float   INTEGER NOT NULL DEFAULT 0 CHECK (opening_float >= 0),
  closed_at       TEXT,
  closed_by       TEXT REFERENCES users (id),
  expected_cash   INTEGER,
  counted_cash    INTEGER CHECK (counted_cash IS NULL OR counted_cash >= 0),
  difference      INTEGER GENERATED ALWAYS AS (counted_cash - expected_cash) VIRTUAL,
  notes           TEXT,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  CHECK (status = 'open' OR (closed_at IS NOT NULL AND counted_cash IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX ux_register_sessions_open ON register_sessions (register_id) WHERE status = 'open';

-- Money in or out. Split payments at the till are several rows allocated to the same invoice.
CREATE TABLE payments (
  id                   TEXT PRIMARY KEY,
  branch_id            TEXT NOT NULL REFERENCES branches (id),
  series_id            TEXT REFERENCES number_series (id),
  number_period        TEXT,
  doc_seq              INTEGER,
  doc_no               TEXT NOT NULL,
  business_date        TEXT NOT NULL,
  direction            TEXT NOT NULL CHECK (direction IN ('in', 'out')),
  party_id             TEXT REFERENCES parties (id),
  account_id           TEXT NOT NULL REFERENCES accounts (id),
  mode                 TEXT NOT NULL CHECK (mode IN (
                         'cash', 'card', 'upi', 'bank_transfer', 'cheque', 'wallet', 'other')),
  amount               INTEGER NOT NULL CHECK (amount > 0),
  -- Cash handed over and change returned (POS); amount is what was applied.
  tendered             INTEGER CHECK (tendered IS NULL OR tendered >= amount),
  change_given         INTEGER NOT NULL DEFAULT 0 CHECK (change_given >= 0),
  reference_no         TEXT,
  status               TEXT NOT NULL DEFAULT 'finalized' CHECK (status IN ('finalized', 'voided')),
  register_session_id  TEXT REFERENCES register_sessions (id),
  device_id            TEXT REFERENCES devices (id),
  idempotency_key      TEXT UNIQUE,
  notes                TEXT,
  voided_at            TEXT,
  voided_by            TEXT REFERENCES users (id),
  void_reason          TEXT,
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  created_by           TEXT REFERENCES users (id),
  updated_by           TEXT REFERENCES users (id),
  deleted_at           TEXT,
  CHECK (tendered IS NULL OR change_given = tendered - amount)
) STRICT;
CREATE UNIQUE INDEX ux_payments_doc_no ON payments (doc_no);
CREATE INDEX ix_payments_date ON payments (business_date, direction);
CREATE INDEX ix_payments_party ON payments (party_id, business_date);
CREATE INDEX ix_payments_account ON payments (account_id, business_date);
CREATE INDEX ix_payments_session ON payments (register_session_id) WHERE register_session_id IS NOT NULL;

CREATE TABLE payment_allocations (
  id           TEXT PRIMARY KEY,
  payment_id   TEXT NOT NULL REFERENCES payments (id),
  target_type  TEXT NOT NULL CHECK (target_type IN (
                 'sales_invoice', 'purchase_bill', 'sales_return', 'purchase_return', 'expense')),
  target_id    TEXT NOT NULL,
  amount       INTEGER NOT NULL CHECK (amount > 0),
  created_at   TEXT NOT NULL,
  created_by   TEXT REFERENCES users (id)
) STRICT;
CREATE INDEX ix_payment_allocations_payment ON payment_allocations (payment_id);
CREATE INDEX ix_payment_allocations_target ON payment_allocations (target_type, target_id);

CREATE TABLE expense_categories (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  account_id  TEXT NOT NULL REFERENCES accounts (id),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  created_by  TEXT REFERENCES users (id),
  updated_by  TEXT REFERENCES users (id),
  deleted_at  TEXT
) STRICT;
CREATE UNIQUE INDEX ux_expense_categories_name ON expense_categories (lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE expenses (
  id                    TEXT PRIMARY KEY,
  branch_id             TEXT NOT NULL REFERENCES branches (id),
  series_id             TEXT REFERENCES number_series (id),
  number_period         TEXT,
  doc_seq               INTEGER,
  doc_no                TEXT NOT NULL,
  business_date         TEXT NOT NULL,
  category_id           TEXT NOT NULL REFERENCES expense_categories (id),
  party_id              TEXT REFERENCES parties (id),
  title                 TEXT NOT NULL,
  description           TEXT,
  reference_no          TEXT,
  amount                INTEGER NOT NULL CHECK (amount > 0),
  tax_rate              INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 1000000),
  tax_total             INTEGER NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
  total                 INTEGER NOT NULL CHECK (total >= 0),
  amount_paid           INTEGER NOT NULL DEFAULT 0,
  status                TEXT NOT NULL DEFAULT 'finalized' CHECK (status IN ('finalized', 'voided')),
  voided_at             TEXT,
  voided_by             TEXT REFERENCES users (id),
  void_reason           TEXT,
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  created_by            TEXT REFERENCES users (id),
  updated_by            TEXT REFERENCES users (id),
  deleted_at            TEXT,
  CHECK (total = amount + tax_total),
  CHECK (amount_paid BETWEEN 0 AND total)
) STRICT;
CREATE UNIQUE INDEX ux_expenses_doc_no ON expenses (doc_no);
CREATE INDEX ix_expenses_date ON expenses (business_date);
CREATE INDEX ix_expenses_category ON expenses (category_id, business_date);

CREATE TABLE day_closes (
  id             TEXT PRIMARY KEY,
  branch_id      TEXT NOT NULL REFERENCES branches (id),
  business_date  TEXT NOT NULL,
  closed_at      TEXT NOT NULL,
  closed_by      TEXT NOT NULL REFERENCES users (id),
  summary_json   TEXT NOT NULL CHECK (json_valid(summary_json)),
  notes          TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  UNIQUE (branch_id, business_date)
) STRICT;

-- Payments: created finalized, then only voidable.
CREATE TRIGGER trg_payments_lock BEFORE UPDATE OF
  branch_id, series_id, number_period, doc_seq, doc_no, business_date, direction, party_id,
  account_id, mode, amount, tendered, change_given, created_at, created_by
ON payments
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: payments cannot be edited; void and re-enter');
END;
CREATE TRIGGER trg_payments_status BEFORE UPDATE OF status ON payments
WHEN NOT (OLD.status = NEW.status OR (OLD.status = 'finalized' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: payment status change not allowed');
END;
CREATE TRIGGER trg_payments_no_delete BEFORE DELETE ON payments
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: payments cannot be deleted; void instead');
END;

CREATE TRIGGER trg_expenses_lock BEFORE UPDATE OF
  branch_id, doc_no, business_date, category_id, party_id, amount, tax_rate, tax_total, total,
  created_at, created_by
ON expenses
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: expenses cannot be edited; void and re-enter');
END;
CREATE TRIGGER trg_expenses_status BEFORE UPDATE OF status ON expenses
WHEN NOT (OLD.status = NEW.status OR (OLD.status = 'finalized' AND NEW.status = 'voided'))
BEGIN
  SELECT RAISE(ABORT, 'INVALID_STATUS: expense status change not allowed');
END;
CREATE TRIGGER trg_expenses_no_delete BEFORE DELETE ON expenses
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: expenses cannot be deleted; void instead');
END;

-- Allocations: validate the target, the direction and the remaining payment amount, then keep
-- the paid / refunded caches on the target documents in step.
CREATE TRIGGER trg_payment_allocations_check BEFORE INSERT ON payment_allocations
BEGIN
  SELECT RAISE(ABORT, 'INVALID_ALLOCATION: payment is not finalized')
  WHERE (SELECT status FROM payments WHERE id = NEW.payment_id) IS NOT 'finalized';

  SELECT RAISE(ABORT, 'INVALID_ALLOCATION: target document not found, not finalized, or wrong direction')
  WHERE NOT (
    CASE NEW.target_type
      WHEN 'sales_invoice' THEN
        EXISTS (SELECT 1 FROM sales_invoices WHERE id = NEW.target_id AND status = 'finalized')
        AND (SELECT direction FROM payments WHERE id = NEW.payment_id) = 'in'
      WHEN 'purchase_return' THEN
        EXISTS (SELECT 1 FROM purchase_returns WHERE id = NEW.target_id AND status = 'finalized')
        AND (SELECT direction FROM payments WHERE id = NEW.payment_id) = 'in'
      WHEN 'purchase_bill' THEN
        EXISTS (SELECT 1 FROM purchase_bills WHERE id = NEW.target_id AND status = 'finalized')
        AND (SELECT direction FROM payments WHERE id = NEW.payment_id) = 'out'
      WHEN 'sales_return' THEN
        EXISTS (SELECT 1 FROM sales_returns WHERE id = NEW.target_id AND status = 'finalized')
        AND (SELECT direction FROM payments WHERE id = NEW.payment_id) = 'out'
      WHEN 'expense' THEN
        EXISTS (SELECT 1 FROM expenses WHERE id = NEW.target_id AND status = 'finalized')
        AND (SELECT direction FROM payments WHERE id = NEW.payment_id) = 'out'
      ELSE 0
    END
  );

  SELECT RAISE(ABORT, 'INVALID_ALLOCATION: allocations exceed the payment amount')
  WHERE (SELECT coalesce(sum(amount), 0) FROM payment_allocations WHERE payment_id = NEW.payment_id)
        + NEW.amount > (SELECT amount FROM payments WHERE id = NEW.payment_id);
END;

CREATE TRIGGER trg_payment_allocations_apply AFTER INSERT ON payment_allocations
BEGIN
  UPDATE sales_invoices SET amount_paid = amount_paid + NEW.amount
  WHERE NEW.target_type = 'sales_invoice' AND id = NEW.target_id;
  UPDATE purchase_bills SET amount_paid = amount_paid + NEW.amount
  WHERE NEW.target_type = 'purchase_bill' AND id = NEW.target_id;
  UPDATE sales_returns SET amount_refunded = amount_refunded + NEW.amount
  WHERE NEW.target_type = 'sales_return' AND id = NEW.target_id;
  UPDATE purchase_returns SET amount_refunded = amount_refunded + NEW.amount
  WHERE NEW.target_type = 'purchase_return' AND id = NEW.target_id;
  UPDATE expenses SET amount_paid = amount_paid + NEW.amount
  WHERE NEW.target_type = 'expense' AND id = NEW.target_id;
END;

CREATE TRIGGER trg_payment_allocations_no_update BEFORE UPDATE ON payment_allocations
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: allocations cannot be changed; void the payment');
END;
CREATE TRIGGER trg_payment_allocations_no_delete BEFORE DELETE ON payment_allocations
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: allocations cannot be deleted; void the payment');
END;

-- Voiding a payment releases everything it had paid.
CREATE TRIGGER trg_payments_void_release AFTER UPDATE OF status ON payments
WHEN OLD.status = 'finalized' AND NEW.status = 'voided'
BEGIN
  UPDATE sales_invoices SET amount_paid = amount_paid - (
    SELECT sum(a.amount) FROM payment_allocations a
    WHERE a.payment_id = NEW.id AND a.target_type = 'sales_invoice' AND a.target_id = sales_invoices.id)
  WHERE id IN (SELECT target_id FROM payment_allocations WHERE payment_id = NEW.id AND target_type = 'sales_invoice');
  UPDATE purchase_bills SET amount_paid = amount_paid - (
    SELECT sum(a.amount) FROM payment_allocations a
    WHERE a.payment_id = NEW.id AND a.target_type = 'purchase_bill' AND a.target_id = purchase_bills.id)
  WHERE id IN (SELECT target_id FROM payment_allocations WHERE payment_id = NEW.id AND target_type = 'purchase_bill');
  UPDATE sales_returns SET amount_refunded = amount_refunded - (
    SELECT sum(a.amount) FROM payment_allocations a
    WHERE a.payment_id = NEW.id AND a.target_type = 'sales_return' AND a.target_id = sales_returns.id)
  WHERE id IN (SELECT target_id FROM payment_allocations WHERE payment_id = NEW.id AND target_type = 'sales_return');
  UPDATE purchase_returns SET amount_refunded = amount_refunded - (
    SELECT sum(a.amount) FROM payment_allocations a
    WHERE a.payment_id = NEW.id AND a.target_type = 'purchase_return' AND a.target_id = purchase_returns.id)
  WHERE id IN (SELECT target_id FROM payment_allocations WHERE payment_id = NEW.id AND target_type = 'purchase_return');
  UPDATE expenses SET amount_paid = amount_paid - (
    SELECT sum(a.amount) FROM payment_allocations a
    WHERE a.payment_id = NEW.id AND a.target_type = 'expense' AND a.target_id = expenses.id)
  WHERE id IN (SELECT target_id FROM payment_allocations WHERE payment_id = NEW.id AND target_type = 'expense');
END;
