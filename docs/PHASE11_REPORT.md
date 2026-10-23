# Phase 11 report: accounting depth and reports

*2026-10-23. Gap analysis: [GAP_ANALYSIS_PHASE9.md](GAP_ANALYSIS_PHASE9.md) §Phase 11. Decisions:
[ASSUMPTIONS.md](ASSUMPTIONS.md) A-118 to A-135. Migration 0012.*

## What was built

- **Reviewed manual journals.** Draft → submitted → approved (`journal.approve`, the owner) →
  posted (`journal.manage`). Balanced-check before submitting; send back with a note; files
  attached (PDF or picture). Posted journals are frozen by triggers and corrected only by a
  reversal journal that goes through the same review. Every entry is tagged "Manual journal"
  in its narration, the day book and the audit log. Control accounts need an owner's reason per
  journal; receivable and payable lines also post to the party; stock accounts are refused.
- **Closed-day lock.** One check (`services::period::check`) in every dated document and void:
  sales, returns, payments, goods receipts, bills, purchase returns, expenses, stock adjustments,
  transfers, counts and journals. Writing into a locked day needs `period.override` (own or a
  manager's approval) and is audited; offline sales are kept and flagged. The owner turns the
  lock on, reopens days with a reason, and closes or reopens financial years (Retained Earnings,
  dry run, expected-profit check).
- **Bank reconciliation.** CSV import with guessed and remembered columns, exact amounts,
  duplicate-safe re-imports; automatic matching by amount, date window and reference; matching
  one line to several, ignore, tick; the reconciliation statement and its history.
- **Post-dated cheques** with a due list and notices; clearing records the payment, a bounce
  voids it.
- **Audit export** (CSV and Excel) with the viewer's filters, audited.
- **Reports**: grouping and drill-down down to the invoice, a "Sales invoices" report, the state
  in the address, saved views per user, and scheduled CSV export to a folder on the hub.
- **Day close**: till differences need a reason (and a manager above the limit); the day close
  shows them, shows the lock, and prints a summary.

## Tests (2026-10-23)

- `cargo test`: 219 passed, 1 ignored. New: `services/tests/accounting_depth.rs` (13 tests:
  journal review and reversal, control accounts, the lock on every document kind, year close
  and reopen, bank import/matching/reconciliation on fixtures, ties and voided pairs, cheques,
  till differences, audit export, drill-down, scheduled exports, a permission matrix), the 0012
  migration on a database in use, the CSV and amount parsers, the day-close print snapshot.
- `npm run rust:lint`: clean. `npm run check`: clean (lint warnings are the 15 earlier ones);
  Vitest 271 passed, 98.7% of lines.
- End-to-end: 6 passed, including the new journal → statement import → audit export flow and the
  accessibility scan of Journals, Bank reconciliation, Cheques, Periods and Settings →
  Accounting in both themes.
- Seen on this PC: in 3 of about 10 end-to-end runs, the browser's calls to the test bridge
  stopped being answered for over a minute (twice right after the bridge was rebuilt). The
  same commands answer instantly when called directly, and the next run passed; the cause was
  not found (see Risks).

## Deliberately not built

- Posting till differences to a "cash over/short" account (A-130): recorded and printed only.
- Excel or PDF bank statements (CSV only, A-126); one book line split over several statement
  lines (A-127).
- Excel files or e-mail for scheduled exports (CSV to a folder, A-133).
- Attachments in backups (Phase 12, with pictures).

## Risks

- The lock is **off by default** to keep today's behaviour; a shop that never turns it on gets
  no protection from back-dating beyond the existing `sales.backdate` approval.
- Automatic matching never guesses between equal candidates; statements with many identical
  amounts on the same days will need more matching by hand.
- The intermittent stall of the end-to-end bridge described above: not reproduced outside the
  test runner. If CI shows it, capture the bridge's log (Playwright `webServer.stdout: "pipe"`).
- Scheduled exports depend on the hub running; a missed day is exported at the next run only for
  the current period (earlier missed periods are not back-filled).

## Check by hand before approving

1. Money → Journals → New journal: General Expenses debit ₹150, Cash credit ₹150, attach a PDF,
   submit, approve (as owner), post. The day book shows "manual journal". Reverse it and post the
   reversal: Cash is back.
2. A journal to Accounts Receivable: refused until the owner allows control accounts; after
   posting, the customer's balance changed by the same amount.
3. Settings → Accounting → Lock closed days. Close yesterday (Day close → choose yesterday).
   Back-date a bill to yesterday as a cashier: the manager PIN is asked; the audit log shows
   `PERIOD_OVERRIDE`. Reopen yesterday (Periods and year end) and try again.
4. Money → Bank reconciliation → Import statement: your bank's CSV. Check the guessed columns and
   the preview, import, **Match automatically**, match the rest by hand, and finish the
   reconciliation at the statement's last date.
5. Money → Cheques: record a cheque dated tomorrow; next day **Cleared**; then **Bounced** and
   check the customer owes the bill again.
6. Close a till ₹500 short: a reason is asked, then a manager's PIN; Day close shows both and
   **Print summary** prints them.
7. Reports → Sales summary → By customer → click a customer → the invoices → an invoice opens.
   Save the view; on the hub, schedule it daily to a folder and check the CSV next day.
8. Settings → Audit log → filter by an action → CSV and Excel.
