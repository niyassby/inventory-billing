# Accounts audit

Scope: ledgers (party, stock, journal), receivables and payables, payments and receipts, expenses,
journal entries, the till and day close, and the balance reports, in the service layer and in what
the screens show. Method: 23 scenarios in `src-tauri/src/services/tests/accounts_audit*.rs`. Each ends
by checking that the books agree with themselves (`Kit::assert_books_balance`: balanced journal,
control accounts against party balances, inventory against stock) and, where a screen shows a
figure, that the figure equals the database.

## Bugs found and fixed

| # | Where | What was wrong | Fix |
|---|---|---|---|
| F1 | Till (`sales::record_payment`, register session) | "Expected cash" only counted cash from till sales. Cash refunds, cash expenses, receipts on account and supplier cash payments were left out, so the day close showed a false shortage or surplus. | Cash payments with no explicit session are attributed to the operator's open till session. |
| F2 | Party balances report, control reconciliation | A party that is both customer and supplier has one net balance, but the report listed it on both sides, so receivables and payables did not add up to the control accounts and a payable could show as negative. | Such a party is listed once, on the side of its net sign. The test invariant is `AR − AP = Σ party balances`, with per-side checks only when no "both" party exists. |

## Checked and correct

- Journal entries balance (debits = credits); database triggers refuse edits and deletes of journal
  lines and entries, party ledger rows, stock movements, audit rows and posted documents.
- Running balances: party ledger and stock card (the stock card's running balance is a window
  function; closing = opening + in-period).
- Part payments, auto-allocation, overpayment kept as an advance and applied later; the same
  document twice in one request is refused; a replayed payment (same idempotency key) is not posted
  twice.
- Void payment, void invoice, void bill and void expense reverse everything they posted (journal,
  party balance, stock) and a second void is refused.
- Returns cannot exceed what was sold; zero and negative amounts and quantities are refused;
  discounts cannot go negative; card and other non-cash payments cannot exceed the bill; rounding
  adds up to the invoice total.
- A party with a balance cannot be deleted; rewriting an opening balance does not corrupt the cached
  balance.
- Figures shown on screens equal the database: receivables, sales summary, stock valuation, trial
  balance, cash book and the day-close summary.

## Observations not changed

1. The expense list loads each row's detail separately (an N+1); fine at shop volumes, slow on
   tens of thousands of expenses.
2. A walk-in (no customer) return needs an explicit refund line; there is no "credit to customer"
   because there is no customer.
3. "Both" parties are netted into one balance (A-94). This is a design choice that reads as set-off;
   gross figures per side would need two balances per party.
4. Backups do not include product pictures, and a counter that is offline shows the placeholder
   (A-90).
5. The till has no product grid, so pictures appear in search results and on cart lines only.
6. "New sale" is Hold or Clear; there is no separate action.
7. Till shortcuts are per PC and user (A-92); the hub has no preference store.

## Not verified

- No run against a large real shop database; the scenarios use small generated data.
- Tax on returns in the day-close summary is accepted either as gross of returns or net of them
  (`day.tax_total`); the audit test allows both, it does not decide which the shop should see.
