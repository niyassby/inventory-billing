# User guide

For the people who use the app every day: owners, managers and cashiers. For setting up several
counters, backups and troubleshooting, see the [Admin guide](ADMIN_GUIDE.md).

> Screenshots: each section has a placeholder (`[screenshot: …]`) to fill in from your own shop.

## 1. First start

The first time the app opens it asks for your business name, address, GSTIN (if you have one),
and the **owner** account (your name, e-mail and a password of at least 10 characters).
There is no default password. Write the owner password down: if every owner forgets theirs, the
data is still safe but only a backup restore can get you back in.

Next it offers to set up the **receipt printer**: say how it is connected, press **Print a test
page**, and **Use this printer**. **Skip for now** keeps printing through the normal print dialog;
you can add printers later in Settings → Printers and drawer.

[screenshot: first-run setup]

## 2. Signing in

Type your e-mail and password. After 5 wrong tries the account waits 15 minutes. If you leave the
PC for 30 minutes you are signed out. Managers can also give a cashier a **PIN** for quick
sign-in at the till.

## 3. Selling at the till

Open **Point of sale** (or press **F2** anywhere).

| Do this            | How                                                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| Add an item        | Scan the barcode, or type part of the name or SKU and press **Enter**                                                    |
| Change quantity    | **+** and **−**, or double-click the line                                                                                |
| Remove a line      | **Delete**, or the bin button                                                                                            |
| Discount           | Double-click a line (that item) or **Discount** (the whole bill). Large discounts ask for a manager.                     |
| Choose a customer  | Search at the top right. Needed for credit sales.                                                                        |
| Hold the bill      | **Hold** (or leave it and start another); **F9** brings it back                                                          |
| Pay                | **F10**. Choose cash, card, UPI or a mix. For cash, type what the customer gave: the change shows. **Enter** completes. |

After payment the receipt prints by itself when the PC has a receipt printer set to do so, and
the cash drawer opens for cash. **P** prints (another) copy, marked REPRINT if one already came
out. Without a receipt printer, P opens the print dialog (58 mm, 80 mm or A4) as before.

If the printer fails (paper out, cable, switched off) **the sale is still saved**: a message says
what is wrong and has a **Reprint** button. Fix the printer and press it.

**Prices** come from the customer's price list, else the till's (once the till is open), else the
default list, else the product's own price; quantity slabs (for example "10 kg or more") apply by themselves as the
quantity grows.

**Batches** (medicines, food): the till sells the batch that expires first, splitting a line
across batches when one runs out, and the line shows which batch. To sell a particular batch,
double-click the line and choose it. Expired batches are not sold unless the owner allows it
(Settings → Inventory); the receipt and invoice show each batch and its expiry month.

Holding a bill prints a small slip with its name when a printer is set for held bills. Closing
the till offers **Print summary** (the cash-up with any difference, and signature lines).

[screenshot: the till]

**Open the till** (cash drawer) at the start of the shift and close it at the end: count the cash
and the app shows any difference. If the cash is short or over, choose why (counting error,
wrong change, …); a large difference needs a manager's PIN.

### If a sale needs a manager

Price changes, big discounts, returns and voids ask for a manager's approval. The manager types
their own PIN or password on your screen. Their name is saved with the sale.

## 4. Returns and cancelled sales

Open the invoice (Sell → Invoices), then **Return** to take items back (stock and money are
adjusted) or **Void** to cancel the whole sale. Documents are never edited or deleted afterwards:
a correction is a new entry that points at the old one.

## 5. Products and stock

- **Products**: add items with a name, SKU, barcode, prices, tax rate and unit. One product can
  have variants (size, colour). Opening stock is entered once when the item is created. An item
  without a barcode can get an **in-store barcode** (the barcode button next to the empty field,
  on a saved product): a 13-digit number starting with 29 that no other product or pack has.
- **Barcode labels** (Stock → Barcode labels): add products by search or scan, or take a whole
  goods receipt (one label per unit received, with batch and expiry), set how many labels, check
  the preview and **Print**. With a label printer they print at once; otherwise they print on A4
  sticker sheets through the print dialog (**A4 sheet** does that on purpose). Label templates
  (size, labels across, barcode type, what is shown) are edited on the same screen.
- **Stock moves** shows every change, with the reason. Use **Adjust** for damage or found stock,
  **Transfer** between stores, and **Count** to compare shelves with the system.
- The low-stock list is on the dashboard and under Products → _Low stock_.
- **Batches and expiry**: tick *Track batches* on a product. Opening stock, goods receipts,
  bills and *stock in* adjustments then ask for the batch number and expiry; *stock out*,
  transfers and returns to the supplier take the batch that expires first unless you choose one.
  A stock count lists each batch on its own line.
- **Expiry** (Stock → Expiry): batches that have expired or will expire soon, soonest first, with
  their value. *Write off* takes a batch out of stock as an adjustment ("Expired"). The
  dashboard shows a card when something is expiring, and a notice comes up once when a batch
  enters the warning window and once when it expires.
- **Price lists** (Stock → Price lists, owners and managers): make a list (retail, wholesale or
  custom), then add prices: per piece or per pack, from a quantity, from and until a date. Give a
  list to a customer (on the customer's form) or to a till (on the same screen); one list can be
  the default for everyone else. *Change by %* raises or lowers every price on the list (or a
  category) by a percentage or an amount, rounded if you like, and shows the old and new prices
  before you apply it. *Change product prices* does the same to the products' own prices.
- **Units** (Masters → Units): besides each product's pack sizes, a shop-wide conversion such as
  "1 box = 12 pcs" lets any product counted in pieces be sold by the box. Stock is always shown
  in the product's base unit.

[screenshot: products]

## 6. Buying

Purchases → **New bill** from a supplier. Receiving the bill adds stock and updates the cost.
Pay now or leave it as a payable. **Purchase return** sends goods back.

## 7. Customers, suppliers and payments

Each customer and supplier has a ledger: what they owe or are owed, with every bill and payment.
**Payments** records money received or paid and settles bills (oldest first, or the ones you pick).

## 8. Expenses, accounts and closing the day

- **Expenses**: rent, power, wages. Paid now or on credit.
- **Accounts**: cash and bank balances. Add a bank account here.
- **Day close**: at the end of the day, check the summary (sales, tax, returns, payments by
  mode, expenses) and close. It won't close while a till is still open.

## 8a. Journals, bank and cheques

- **Journals** (Money → Journals): for corrections the documents don't make (an accrual, a
  wrong account, opening balances). Add lines until debits equal credits, attach the bill or
  letter, and **Submit for approval**. The owner **Approves** (or sends it back with a note), then
  **Post to the books**. A posted journal can't be edited: **Reverse** drafts a new journal with
  every line swapped, which goes through the same steps. Receivables, payables and tax need the
  owner's permission on that journal; stock is changed with a stock adjustment, never a journal.
- **Bank reconciliation** (Money → Bank reconciliation): **Import statement** (the CSV file from
  your bank's website), check the columns and the preview, **Import**. **Match automatically**
  pairs the obvious ones; for the rest, choose a statement line on the left and the book lines it
  stands for on the right, then **Match**. Book lines the bank cleared without a statement line
  can be **Marked cleared**. Below, the reconciliation statement shows whether the books and the
  statement agree; **Finish reconciliation** keeps it.
- **Cheques** (Money → Cheques): **Record a cheque** you received or gave with a later date.
  Nothing is booked until it clears: when its date comes you get a notice; **Cleared** records the
  payment, **Bounced** undoes it, **Cancel** if it won't be deposited.

## 9. Reports

Choose **By …** to group a report (by day, month, customer, cashier, item or category for sales;
supplier or month for purchases). Click a row to see the invoices behind it, and an invoice to
open it. **Save view** keeps the period, grouping and filters under your own name; on the hub an
owner or manager can **Schedule export** of a view as a CSV file into a folder every day, week or
month.

Reports → pick one → choose the period (today, this month, this year, or dates). Every report can
be printed, saved as PDF (choose "Save as PDF" in the print window) or exported to CSV or Excel
if your role allows it. Sales, purchases, stock valuation and movement, item profit, receivables
and payables, tax summary, profit and loss, trial balance, day book, cash book and bank book.

[screenshot: a report]

## 10. Search and shortcuts

| Key                 | Does                                                |
| ------------------- | --------------------------------------------------- |
| **Ctrl+K**          | Search screens and products from anywhere           |
| **F2**              | Open the till                                       |
| **F9 / F10**        | Held bills / Pay (at the till)                      |
| **Esc**             | Close a window                                      |
| **Tab / Shift+Tab** | Move between fields; **Enter** on a button presses it |

The sun/moon button in the top bar switches between light and dark. "Use compact spacing" in the
Ctrl+K list fits more on small screens.

## 11. The status light

The top bar always shows how this PC is connected: **Online** (talking to the hub), **Offline**
(working alone; sales are saved and sent to the hub later), **Syncing**, or the hub's own state.
If a counter is offline, keep selling: invoice numbers come from a reserved block.

## 12. When something goes wrong

- A red message says what to do. If it says **"needs a manager"**, ask one to approve.
- "Out of stock" lets you continue only if your shop allows selling below zero.
- If the app seems stuck, close and reopen it: nothing is lost, because every sale is saved as
  one complete step or not at all.
- For anything else, tell whoever looks after the PCs; they use the Admin guide, and logs are
  in `%LOCALAPPDATA%\com.niyas.tauri-app\logs`.
