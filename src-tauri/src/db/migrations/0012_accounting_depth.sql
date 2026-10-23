-- 0012: accounting depth (Phase 11): reviewed manual journals with attachments, closed-day lock
-- and reopening, financial-year close, bank statements and reconciliation, post-dated cheques,
-- till differences with reasons, saved report views and scheduled exports.

-- Manual journals: a draft is edited freely, then submitted, approved by someone with
-- journal.approve and posted. Posting writes an ordinary journal entry (source_type
-- 'manual_journal'); after that the journal itself is frozen and only a reversal corrects it.
CREATE TABLE manual_journals (
  id                   TEXT PRIMARY KEY,
  doc_no               TEXT NOT NULL,
  number_period        TEXT NOT NULL,
  doc_seq              INTEGER NOT NULL CHECK (doc_seq >= 1),
  business_date        TEXT NOT NULL,
  narration            TEXT NOT NULL,
  status               TEXT NOT NULL DEFAULT 'draft' CHECK (status IN (
                         'draft', 'submitted', 'approved', 'posted', 'reversed')),
  -- An owner's permission to post to control accounts (receivables, payables, tax), with why.
  allow_restricted     INTEGER NOT NULL DEFAULT 0 CHECK (allow_restricted IN (0, 1)),
  restricted_reason    TEXT,
  restricted_by        TEXT REFERENCES users (id),
  -- Set on a journal that reverses another one.
  reversal_of          TEXT REFERENCES manual_journals (id),
  reversed_by_journal  TEXT REFERENCES manual_journals (id),
  journal_entry_id     TEXT REFERENCES journal_entries (id),
  idempotency_key      TEXT UNIQUE,
  submitted_at         TEXT,
  submitted_by         TEXT REFERENCES users (id),
  approved_at          TEXT,
  approved_by          TEXT REFERENCES users (id),
  posted_at            TEXT,
  posted_by            TEXT REFERENCES users (id),
  -- The last "send back" note (a rejected journal goes back to draft).
  review_note          TEXT,
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  created_by           TEXT REFERENCES users (id),
  updated_by           TEXT REFERENCES users (id),
  deleted_at           TEXT,
  CHECK (allow_restricted = 0 OR (restricted_reason IS NOT NULL AND restricted_by IS NOT NULL)),
  CHECK (status NOT IN ('posted', 'reversed') OR (journal_entry_id IS NOT NULL AND approved_by IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX ux_manual_journals_doc_no ON manual_journals (doc_no);
CREATE UNIQUE INDEX ux_manual_journals_seq ON manual_journals (number_period, doc_seq);
CREATE INDEX ix_manual_journals_status ON manual_journals (status, business_date);

CREATE TABLE manual_journal_lines (
  id          TEXT PRIMARY KEY,
  journal_id  TEXT NOT NULL REFERENCES manual_journals (id),
  line_no     INTEGER NOT NULL CHECK (line_no >= 1),
  account_id  TEXT NOT NULL REFERENCES accounts (id),
  party_id    TEXT REFERENCES parties (id),
  debit       INTEGER NOT NULL DEFAULT 0 CHECK (debit >= 0),
  credit      INTEGER NOT NULL DEFAULT 0 CHECK (credit >= 0),
  memo        TEXT,
  created_at  TEXT NOT NULL,
  UNIQUE (journal_id, line_no),
  CHECK ((debit = 0) <> (credit = 0))
) STRICT;
CREATE INDEX ix_manual_journal_lines_journal ON manual_journal_lines (journal_id);

-- A posted or reversed journal can't change, and no journal is ever deleted (soft delete of
-- drafts only).
CREATE TRIGGER trg_manual_journals_lock BEFORE UPDATE ON manual_journals
WHEN OLD.status IN ('posted', 'reversed') AND NOT (
  OLD.status = 'posted' AND NEW.status = 'reversed'
  AND NEW.doc_no = OLD.doc_no AND NEW.business_date = OLD.business_date
  AND NEW.narration = OLD.narration AND NEW.journal_entry_id IS OLD.journal_entry_id
)
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: a posted journal cannot be changed; reverse it');
END;
CREATE TRIGGER trg_manual_journals_no_delete BEFORE DELETE ON manual_journals
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: journals cannot be deleted');
END;
CREATE TRIGGER trg_manual_journal_lines_lock_ins BEFORE INSERT ON manual_journal_lines
WHEN (SELECT status FROM manual_journals WHERE id = NEW.journal_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: only a draft journal can be edited');
END;
CREATE TRIGGER trg_manual_journal_lines_lock_upd BEFORE UPDATE ON manual_journal_lines
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: journal lines are replaced, not edited');
END;
CREATE TRIGGER trg_manual_journal_lines_lock_del BEFORE DELETE ON manual_journal_lines
WHEN (SELECT status FROM manual_journals WHERE id = OLD.journal_id) <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: only a draft journal can be edited');
END;

-- Files attached to a record (journals today). The file lives in an `attachments` folder next
-- to the database; `storage_key` is its name there.
CREATE TABLE attachments (
  id            TEXT PRIMARY KEY,
  entity_type   TEXT NOT NULL CHECK (entity_type IN ('manual_journal')),
  entity_id     TEXT NOT NULL,
  file_name     TEXT NOT NULL,
  content_type  TEXT NOT NULL,
  size_bytes    INTEGER NOT NULL CHECK (size_bytes > 0),
  sha256        TEXT NOT NULL,
  storage_key   TEXT NOT NULL UNIQUE,
  created_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  deleted_at    TEXT,
  deleted_by    TEXT REFERENCES users (id)
) STRICT;
CREATE INDEX ix_attachments_entity ON attachments (entity_type, entity_id) WHERE deleted_at IS NULL;

-- Closed days can be reopened by the owner (and closed again).
ALTER TABLE day_closes ADD COLUMN status TEXT NOT NULL DEFAULT 'closed'
  CHECK (status IN ('closed', 'reopened'));
ALTER TABLE day_closes ADD COLUMN reopened_at TEXT;
ALTER TABLE day_closes ADD COLUMN reopened_by TEXT REFERENCES users (id);
ALTER TABLE day_closes ADD COLUMN reopen_reason TEXT;
CREATE INDEX ix_day_closes_status ON day_closes (status, business_date);

-- Financial-year close: income and expense balances moved to retained earnings by one journal
-- entry dated the last day of the year; the year is then locked.
CREATE TABLE fiscal_year_closes (
  id                TEXT PRIMARY KEY,
  fy_label          TEXT NOT NULL,
  start_date        TEXT NOT NULL,
  end_date          TEXT NOT NULL,
  status            TEXT NOT NULL DEFAULT 'closed' CHECK (status IN ('closed', 'reopened')),
  journal_entry_id  TEXT REFERENCES journal_entries (id),
  net_profit        INTEGER NOT NULL,
  closed_at         TEXT NOT NULL,
  closed_by         TEXT NOT NULL REFERENCES users (id),
  reason            TEXT,
  reopened_at       TEXT,
  reopened_by       TEXT REFERENCES users (id),
  reopen_reason     TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  CHECK (start_date <= end_date)
) STRICT;
CREATE UNIQUE INDEX ux_fiscal_year_closes_open ON fiscal_year_closes (start_date) WHERE status = 'closed';

INSERT INTO accounts (id, code, name, account_type, subtype, is_system, created_at, updated_at)
VALUES ('01920000-0000-7000-8001-000000003100', '3100', 'Retained Earnings', 'equity', 'equity', 1,
        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

-- Till differences: a reason, and who approved a large one.
ALTER TABLE register_sessions ADD COLUMN difference_reason TEXT;
ALTER TABLE register_sessions ADD COLUMN difference_approved_by TEXT REFERENCES users (id);

-- Bank statements. A line is the bank's view of one movement: positive = money in.
CREATE TABLE bank_statement_imports (
  id            TEXT PRIMARY KEY,
  account_id    TEXT NOT NULL REFERENCES accounts (id),
  file_name     TEXT NOT NULL,
  from_date     TEXT,
  to_date       TEXT,
  line_count    INTEGER NOT NULL DEFAULT 0,
  skipped       INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id)
) STRICT;
CREATE INDEX ix_bank_statement_imports_account ON bank_statement_imports (account_id, created_at);

CREATE TABLE bank_statement_lines (
  id           TEXT PRIMARY KEY,
  import_id    TEXT NOT NULL REFERENCES bank_statement_imports (id),
  account_id   TEXT NOT NULL REFERENCES accounts (id),
  txn_date     TEXT NOT NULL,
  description  TEXT NOT NULL,
  reference    TEXT,
  amount       INTEGER NOT NULL CHECK (amount <> 0),
  balance      INTEGER,
  -- Same date, amount, text, reference, balance and position among identical lines: the same
  -- line, so importing an overlapping statement twice adds nothing.
  line_hash    TEXT NOT NULL,
  status       TEXT NOT NULL DEFAULT 'unmatched' CHECK (status IN ('unmatched', 'matched', 'ignored')),
  ignore_note  TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  UNIQUE (account_id, line_hash)
) STRICT;
CREATE INDEX ix_bank_statement_lines_account ON bank_statement_lines (account_id, status, txn_date);

-- A cleared book line (a journal line on the bank account), matched to a statement line or
-- ticked by hand. Unmatching keeps the row (deleted_at).
CREATE TABLE bank_matches (
  id                 TEXT PRIMARY KEY,
  account_id         TEXT NOT NULL REFERENCES accounts (id),
  journal_line_id    TEXT NOT NULL REFERENCES journal_lines (id),
  statement_line_id  TEXT REFERENCES bank_statement_lines (id),
  cleared_date       TEXT NOT NULL,
  method             TEXT NOT NULL CHECK (method IN ('auto', 'manual', 'ticked')),
  created_at         TEXT NOT NULL,
  created_by         TEXT REFERENCES users (id),
  deleted_at         TEXT,
  deleted_by         TEXT REFERENCES users (id)
) STRICT;
CREATE UNIQUE INDEX ux_bank_matches_line ON bank_matches (journal_line_id) WHERE deleted_at IS NULL;
CREATE INDEX ix_bank_matches_statement ON bank_matches (statement_line_id) WHERE deleted_at IS NULL;

-- The CSV column mapping last used for an account's statements.
CREATE TABLE bank_import_profiles (
  account_id    TEXT PRIMARY KEY REFERENCES accounts (id),
  mapping_json  TEXT NOT NULL CHECK (json_valid(mapping_json)),
  updated_at    TEXT NOT NULL,
  updated_by    TEXT REFERENCES users (id)
) STRICT;

-- A finished reconciliation: the figures as they were on that statement date.
CREATE TABLE bank_reconciliations (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts (id),
  statement_date      TEXT NOT NULL,
  statement_balance   INTEGER NOT NULL,
  book_balance        INTEGER NOT NULL,
  uncleared_in        INTEGER NOT NULL,
  uncleared_out       INTEGER NOT NULL,
  difference          INTEGER NOT NULL,
  notes               TEXT,
  created_at          TEXT NOT NULL,
  created_by          TEXT REFERENCES users (id)
) STRICT;
CREATE INDEX ix_bank_reconciliations_account ON bank_reconciliations (account_id, statement_date);
CREATE TRIGGER trg_bank_reconciliations_no_update BEFORE UPDATE ON bank_reconciliations
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: a finished reconciliation cannot be changed');
END;
CREATE TRIGGER trg_bank_reconciliations_no_delete BEFORE DELETE ON bank_reconciliations
BEGIN
  SELECT RAISE(ABORT, 'APPEND_ONLY: a finished reconciliation cannot be deleted');
END;

-- Cheques received or given with a date on them. A pending cheque is not money yet: clearing it
-- records the payment; a bounce voids that payment.
CREATE TABLE cheques (
  id               TEXT PRIMARY KEY,
  direction        TEXT NOT NULL CHECK (direction IN ('in', 'out')),
  party_id         TEXT NOT NULL REFERENCES parties (id),
  account_id       TEXT NOT NULL REFERENCES accounts (id),
  cheque_no        TEXT NOT NULL,
  bank_name        TEXT,
  cheque_date      TEXT NOT NULL,
  amount           INTEGER NOT NULL CHECK (amount > 0),
  status           TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'cleared', 'bounced', 'cancelled')),
  payment_id       TEXT REFERENCES payments (id),
  cleared_date     TEXT,
  status_note      TEXT,
  notes            TEXT,
  idempotency_key  TEXT UNIQUE,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  created_by       TEXT REFERENCES users (id),
  updated_by       TEXT REFERENCES users (id),
  deleted_at       TEXT,
  CHECK (status <> 'cleared' OR (payment_id IS NOT NULL AND cleared_date IS NOT NULL))
) STRICT;
CREATE INDEX ix_cheques_due ON cheques (status, cheque_date);
CREATE INDEX ix_cheques_party ON cheques (party_id);
CREATE TRIGGER trg_cheques_no_delete BEFORE DELETE ON cheques
BEGIN
  SELECT RAISE(ABORT, 'DOCUMENT_FINALIZED: cheques cannot be deleted; cancel instead');
END;

-- Saved report views (per user) and their optional scheduled export to a folder.
CREATE TABLE report_views (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL REFERENCES users (id),
  report       TEXT NOT NULL,
  name         TEXT NOT NULL,
  params_json  TEXT NOT NULL CHECK (json_valid(params_json)),
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  deleted_at   TEXT
) STRICT;
CREATE UNIQUE INDEX ux_report_views_name ON report_views (user_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE report_schedules (
  id            TEXT PRIMARY KEY,
  view_id       TEXT NOT NULL REFERENCES report_views (id),
  frequency     TEXT NOT NULL CHECK (frequency IN ('daily', 'weekly', 'monthly')),
  folder        TEXT NOT NULL,
  is_active     INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  last_run_at   TEXT,
  last_period   TEXT,
  last_status   TEXT,
  last_error    TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  created_by    TEXT REFERENCES users (id),
  deleted_at    TEXT
) STRICT;
CREATE UNIQUE INDEX ux_report_schedules_view ON report_schedules (view_id) WHERE deleted_at IS NULL;

INSERT INTO permissions (code, description) VALUES
  ('journal.manage', 'Write and post (journals)'),
  ('journal.approve', 'Approve (journals)'),
  ('journal.restricted', 'Allow control accounts (journals)'),
  ('period.override', 'Date in a locked day (periods)'),
  ('period.manage', 'Reopen days, close years (periods)'),
  ('audit.export', 'Export (audit log)'),
  ('bank.reconcile', 'Reconcile (bank)');
-- Owner: all. Manager: journals (not approve), locked-day override, bank. Accountant: journals
-- (not approve), bank, audit export.
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000001', 'journal.manage'),
  ('01920000-0000-7000-8000-000000000001', 'journal.approve'),
  ('01920000-0000-7000-8000-000000000001', 'journal.restricted'),
  ('01920000-0000-7000-8000-000000000001', 'period.override'),
  ('01920000-0000-7000-8000-000000000001', 'period.manage'),
  ('01920000-0000-7000-8000-000000000001', 'audit.export'),
  ('01920000-0000-7000-8000-000000000001', 'bank.reconcile'),
  ('01920000-0000-7000-8000-000000000002', 'journal.manage'),
  ('01920000-0000-7000-8000-000000000002', 'period.override'),
  ('01920000-0000-7000-8000-000000000002', 'bank.reconcile'),
  ('01920000-0000-7000-8000-000000000005', 'journal.manage'),
  ('01920000-0000-7000-8000-000000000005', 'bank.reconcile'),
  ('01920000-0000-7000-8000-000000000005', 'audit.export');
