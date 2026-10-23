# Phase 10 report: inventory depth

*2026-10-23. Gap analysis: [GAP_ANALYSIS_PHASE9.md](GAP_ANALYSIS_PHASE9.md) §Phase 10. Decisions:
[ASSUMPTIONS.md](ASSUMPTIONS.md) A-108 to A-117. Migration 0011.*

## What was built

- **Batches everywhere.** First to expire, first out (FEFO) on sales, returns to suppliers,
  transfers and stock outs, splitting a line across batches; a chosen batch overrides it (till
  line dialog, adjustments). Batch and expiry captured on opening stock, stock-in adjustments and
  counts (one line per batch) besides receipts and bills. Sale returns go back to the batches
  they came from. Receipts, A4 invoices and the invoice page show batch and expiry month.
- **Expiry.** Stock → Expiry with write-off (a stock adjustment at the batch's cost), dashboard
  card, "Expiry" report, hourly notices (near and expired, closed when the stock is gone), and
  the expired-stock policy: block (default), warn or allow.
- **Price lists.** Retail, wholesale or custom lists; per-variant, per-pack and slab prices with
  effective dates; assignment to customers and tills, plus a default list; bulk change by % or
  amount with rounding and a preview, for a list or for the products' own prices. One rule in
  Rust (`domain::price_lists`) and TypeScript (`resolveListPrice`) checked against shared golden
  vectors; used by the preview, invoices, quotations/orders and the offline counter.
- **FIFO valuation.** Cost layers per item, store and batch with exact values; sales, returns,
  transfers, voids and counts consume or restore them; the ledger and layers are reconciled in
  every test and by the weekly integrity check. A one-time, owner-only, audited switch between
  weighted average and FIFO with a dry run. Item profit uses the exact booked cost; stock
  valuation is titled by method and shows the oldest stock under FIFO.
- **Units.** Masters → Units: shop-wide conversions (1 box = 12 pcs); stock stays in the base unit.

## Tests (2026-10-23)

- `cargo test`: 200 passed, 1 ignored. New: `services/tests/inventory_depth.rs` (FEFO, expired
  policies, two counters and the last unit of a batch, returns per batch, counts and transfers
  per batch, expiry notices, FIFO golden cases, the switch both ways and on negative stock, price
  lists, bulk updates, conversions) and the 0011 migration on weighted-average history.
- `npm run rust:lint`: clean. Vitest: 266 passed, 98.7% lines; price-list golden vectors on
  both sides.
- End-to-end: 5 passed, including a till price-list sale and an expired-batch write-off, and the
  axe scan of Price lists and Expiry in both themes.

## Deliberately not built

- Restating past sales under FIFO when switching (A-111): history keeps the cost it was booked at.
- The till's own price list on an offline counter (A-114): it uses the customer's or default list.
- A batch picker on A4 invoices and quotations: FEFO picks there; the till and adjustments can
  choose (the backend accepts a batch on every line).

## Risks

- Stock below zero under FIFO is valued at the last known cost and settled on the next receipt,
  which can leave a small difference in that layer (A-110). The switch refuses negative stock.
- Price lists change what the till charges without a visible "override": check the list before
  assigning it to a busy till.
- Not tried on a large real database; FEFO and layer lookups are indexed per item and store.

## Check by hand before approving

1. Product with *Track batches*: opening stock in batch A (expires next month) and a goods
   receipt in batch B (next year). Sell 1 at the till: the line says batch A; double-click and
   choose B; the receipt shows the batch and expiry month.
2. Give batch A a past expiry (new product or receipt): the till refuses it under *Block*, warns
   under *Warn* (Settings → Inventory). Stock → Expiry → *Write off*: the stock moves list shows
   the adjustment.
3. Stock → Price lists: a wholesale list with a slab (10 kg or more at a lower price), give it
   to a customer; at the till choose that customer and raise the quantity past 10.
4. *Change by %* +5% with ₹1 rounding: check the preview, apply, see the new prices.
5. Settings → Inventory → Stock valuation: preview, switch to FIFO with a reason; the stock
   value report total does not change; make a purchase at a new cost and a sale; switch back.
6. Masters → Units: 1 box = 12 pcs; sell one box of a piece item; stock drops by 12.
