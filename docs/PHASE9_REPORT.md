# Phase 9 report: printing and labels

*2026-10-22. Gap analysis first: [GAP_ANALYSIS_PHASE9.md](GAP_ANALYSIS_PHASE9.md). Decisions:
[ASSUMPTIONS.md](ASSUMPTIONS.md) A-96 to A-107.*

## What was built

- **Receipt printers (ESC/POS, 58 and 80 mm)** over the network (raw TCP), a serial port or a
  Windows print queue (USB printers with their driver), from this PC or through the hub. Receipt,
  return slip, till close, held-bill slip and test page. Code page per printer; ₹, Hindi and
  Malayalam printed as pictures (shaped text in Nirmala UI); copies; cut. The print dialog stays
  the fallback for every document without a printer.
- **Settings → Printers and drawer**, per PC: printers (find Windows printers and COM ports, test
  page before saving), what prints where, automatic receipt after a sale, cash drawer.
- **Cash drawer**: opens with a cash sale; by hand with `sales.open_drawer` or a manager's
  approval, audited.
- **The hub's print queue**: counters print to printers the hub shares; failures are retried
  after 1, 2 and 5 minutes, then wait for Retry or Cancel; a lost answer never prints twice.
- **Barcode labels**: from products or a goods receipt (one per unit, with batch and expiry),
  templates, preview drawn by the backend, ZPL or TSPL label printers or A4 sheets; EAN-13,
  EAN-8, Code 128, QR. In-store EAN-13 barcodes (prefix 29), unique across products and pack
  sizes; product save now checks that too.
- **First start** offers the receipt printer set-up with a test print.

## Deliberately not built

- Reading printer status (paper out, cover open): failures are connection errors (A-100).
- A direct USB transport without a Windows driver (A-96).
- Batch and expiry on receipts: batches are chosen on the till only from Phase 10.
- Pre-cut A4 label sheets with fixed positions (A-102).

## Risks

- Not tried on real printers or drawers here; only on a fake network printer and byte snapshots.
  ESC/POS clones differ (code-page numbers, `GS v 0` support, cutter commands).
- Pictures of text are slower on old printers (a line with ₹ is about 3 KB). *Rs.* is the fallback.
- Windows queue printing reports success when the spooler takes the job, not when it prints.

## Check by hand before approving

1. Settings → Printers and drawer → Add printer, your thermal printer (network IP, or "USB or
   installed in Windows"), **Print a test page**: the digit ruler fits on one line, ₹ and the
   Hindi/Malayalam line are readable, the paper is cut.
2. Route *Till receipts* to it, tick *Print the receipt as soon as a sale is saved*; choose the
   drawer and *Open the drawer for cash sales*. Make a cash sale: the drawer opens and the total
   matches the screen. Make a card sale: no drawer.
3. Switch the printer off and sell: the sale is saved, the message says what is wrong, **Reprint**
   works after switching it on.
4. Sign in as a cashier: *Open drawer (no sale)* asks for a manager; the audit log shows it.
5. Stock → Barcode labels: pick a goods receipt, check the preview, print to the label printer
   (choose ZPL or TSPL) and on an A4 sheet.
6. On a hub with a counter: share a printer on the hub, route the counter's receipts to it, sell
   on the counter; switch the printer off, sell again, and watch the queue retry.
