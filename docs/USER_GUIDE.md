# User guide

For the people who use the app every day: owners, managers and cashiers. For setting up several
counters, backups and troubleshooting, see the [Admin guide](ADMIN_GUIDE.md).

> Screenshots: each section has a placeholder (`[screenshot: …]`) to fill in from your own shop.

## 1. First start

The first time the app opens it asks for your business name, address, GSTIN (if you have one),
and the **owner** account (your name, e-mail and a password of at least 10 characters).
There is no default password. Write the owner password down: if every owner forgets theirs, the
data is still safe but only a backup restore can get you back in.

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

After payment you can print a receipt (58 mm, 80 mm or A4) or start the next bill.

[screenshot: the till]

**Open the till** (cash drawer) at the start of the shift and close it at the end: count the cash
and the app shows any difference.

### If a sale needs a manager

Price changes, big discounts, returns and voids ask for a manager's approval. The manager types
their own PIN or password on your screen. Their name is saved with the sale.

## 4. Returns and cancelled sales

Open the invoice (Sell → Invoices), then **Return** to take items back (stock and money are
adjusted) or **Void** to cancel the whole sale. Documents are never edited or deleted afterwards:
a correction is a new entry that points at the old one.

## 5. Products and stock

- **Products**: add items with a name, SKU, barcode, prices, tax rate and unit. One product can
  have variants (size, colour). Opening stock is entered once when the item is created.
- **Stock moves** shows every change, with the reason. Use **Adjust** for damage or found stock,
  **Transfer** between stores, and **Count** to compare shelves with the system.
- The low-stock list is on the dashboard and under Products → _Low stock_.

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

## 9. Reports

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
