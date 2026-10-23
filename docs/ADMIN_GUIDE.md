# Admin Guide

For the shop owner or whoever looks after the PCs: how to set up several counters, what happens
when the network fails, and how backups and restores work.

> **Status (Phase 4):** the backend for everything below is built and tested. The screens for it
> (Settings → Network, Settings → Backup, the counter's pairing wizard and the "Sync issues"
> list) arrive with the Phase 5–6 screens. Until then the commands exist (`device_*`,
> `devices_*`, `backup_*`, `outbox_*`, see `packages/shared/src/generated/api.ts`) but the
> current screens don't use them yet.

## 1. Choosing a role for each PC

| Role | Use it for | Data |
|---|---|---|
| **Standalone** (default) | A shop with one PC. | On this PC. |
| **Hub** | The main PC of a shop with several counters. Must be on during trading hours. | On this PC. Backups run here. |
| **Counter** (client) | Each extra billing PC. | On the hub. The counter keeps a copy of the catalogue and any sales made while the hub was unreachable. |

Phones and tablets are not supported as counters yet (see docs/ASSUMPTIONS.md A-47).

## 2. Setting up a hub and counters

1. **On the main PC:** Settings → Network → *This PC is the hub*, then restart the app. The hub
   starts an encrypted (HTTPS) service on port **47900**.
2. **Windows firewall:** the first time, Windows asks whether to allow the app on *private
   networks*. Choose **Allow** (private networks only). If you missed it, allow
   `tauri-app.exe` (the app, until it gets its final name: ASSUMPTIONS A-29) for private networks under *Windows Security → Firewall → Allow an
   app*, or open TCP port 47900 for the private profile.
3. **Give the hub a fixed address:** in your router, reserve its IP address (DHCP reservation),
   so counters always find it. Counters also try `https://<pc-name>.local:47900` and the hub
   announces itself on the network (mDNS), so it usually shows up in the list by itself.
4. **On each counter:** start the app → *Connect to a hub*. Choose the hub from the list or type
   its address (for example `192.168.1.20`).
5. **Check the code.** The counter shows a code such as `AB12-CD34-EF56-7890`. On the hub,
   Settings → Network shows the hub's code. **They must match.** If they don't, another machine
   is answering: stop and ask for help.
6. **Pair.** On the hub, a manager clicks *Pair a counter*; a 6-digit code appears (valid for
   5 minutes, usable once). Type it on the counter. The counter restarts and is ready. It gets a
   short name (C1, C2, …) that also prefixes its offline invoice numbers.

Removing a counter (stolen PC, replaced hardware): hub → Settings → Network → Devices → *Remove*.
Its sign-ins stop working immediately. To use the same PC again, pair it again.

### Security in short

- Only devices on your local network (private addresses such as 192.168.x.x, 10.x.x.x) can
  connect. You can allow extra subnets in Settings → Network if your network uses others.
- The connection is encrypted, and each counter checks the hub's certificate against the code it
  saw when pairing. A different machine pretending to be the hub is refused.
- Every counter has its own secret credential (stored in Windows Credential Manager), and a
  user's sign-in only works on the counter where they signed in.

## 3. When the hub can't be reached

The top bar always shows the state: **Hub connected**, **Hub slow**, **Hub offline (n)** or
**Syncing (n)**.

While the hub is offline a counter can still:

- search and scan items (from its saved catalogue, refreshed every 15 minutes);
- make **fully paid** sales at catalogue prices, with discounts up to the cashier's own limit.
  They get numbers from the counter's own series (for example `C2/26-27/00014`), reserved in
  advance in blocks of 50, so numbers never clash with the hub or other counters;
- print the receipt as usual.

It can't (these need the hub and are refused with a clear message): credit sales, price changes,
discounts that need a manager, back-dated bills, returns, voids, purchases, stock changes,
reports, settings, user changes, and signing in a user who wasn't signed in before the outage.

When the hub is back, the counter sends its sales in the order they were made, automatically.
Sending twice never bills twice. The hub's clock decides the date: a counter whose clock is
wrong gets its bills dated correctly, and they are listed for review.

**Review list (Sync issues).** A manager should look at:

| Issue | Meaning | What to do |
|---|---|---|
| Offline oversell | Two counters sold the last items while offline; stock is now below zero. | Count the item and adjust stock, or receive the missing stock. |
| Price changed | The price was changed on the hub while the counter sold at the old one. | Usually nothing; note it. |
| Clock wrong | The counter's clock was off by more than 24 h. | Fix the counter's date and time. |
| Refused sale | The hub refused an offline sale (for example the cashier was disabled meanwhile). | Fix the cause and *Retry*, or *Discard* it if it was billed another way. |

To stop offline sales completely: Settings → Network → Offline sales → **Not allowed**.
*Allowed, except low-stock and batch items* is a middle way.

## 4. Backups

Backups run on the hub (or the standalone PC):

- every day at **21:00**, and every **4 hours** while the app is open;
- into `backups` next to the database (`%LOCALAPPDATA%\com.niyas.tauri-app\backups`) unless you
  choose another folder, plus an optional **second copy** to a USB drive or network share;
- kept for 7 days (one per day), 4 weeks (one per week) and 12 months (one per month). Manual
  backups are never deleted automatically.

**Set a backup passphrase** (Settings → Backup). From then on, backups are encrypted
(AES-256). Write the passphrase down and keep it somewhere safe: **without it an encrypted backup
cannot be restored on another PC**, and nobody can recover it for you.

A failed backup shows a red notice on the dashboard. Check the folder exists and has free space.

### Restore drill (do this once, and after changing PCs)

1. Settings → Backup → *Back up now*. Note the file name.
2. *Check a backup file* → choose the file. The app shows when it was made and how many products
   and invoices it holds, without changing anything.
3. To restore: *Restore* → choose the file (enter the passphrase if asked) → confirm.
   - The app first makes a **safety backup** of the current data (`…-pre_restore.invbak`).
   - It checks the file completely (checksum, encryption, database integrity) before touching
     anything; a damaged or wrong file is refused.
   - Everyone is signed out and must sign in again.
4. To undo a restore, restore the safety backup.

Restore only works on the hub or standalone PC itself, never from a counter.

### If the data file is damaged

At start the app checks the database file. If the check fails (after a disk fault, for example)
the app **does not open it, repair it or replace it**: it keeps a copy next to it
(`inventory_v2.db.corrupt-<time>`), writes the reason to the log and shows the message "The data
file failed its safety check".

1. Close the app. Do not delete anything.
2. Copy the whole data folder (`%LOCALAPPDATA%\com.niyas.tauri-app`) somewhere safe: the
   `.corrupt-…` copy can often be recovered by a specialist.
3. Move `inventory_v2.db` (and its `-wal` and `-shm` files) out of the folder, start the app and
   create the business again with the **same** name. Sign in as the owner and use
   Settings → Backup → *Restore* with the newest backup from the `backups` folder (or the USB
   copy). Everything up to that backup returns; sales made after it must be entered again, so
   keep the paper bills of that day.

A "restore on first start" screen would shorten this: a suggestion (ASSUMPTIONS A-79).

## 5. Troubleshooting

| Symptom | Check |
|---|---|
| Counter can't find the hub | Same network? Hub app running in hub mode? Firewall allows private networks? Try the IP address instead of the list. |
| "The hub's certificate is not the one this counter was paired with" | The hub was reinstalled or its keychain entry was lost. Remove the counter on the hub and pair it again. |
| "No offline invoice numbers left" | The counter used its reserved block while offline. Reconnect to the hub; it reserves more automatically. |
| Hub shows "Hub not serving" | Port 47900 is used by another program, or the firewall blocks it. Change the port in Settings → Network, restart, and pair the counters again with the new address. |
| Backups fail | Folder missing, read-only or full; USB drive unplugged (only the second copy fails then). |
| Receipts don't print | See §6, *Printers*: the error message names the problem; the sale is saved either way. |

Logs for support are in `%LOCALAPPDATA%\com.niyas.tauri-app\logs` (one file per day, 14 days).

## 6. Printers, labels and the cash drawer

Printers are set **per PC** in Settings → *Printers and drawer* (they are kept in `printers.json`
next to the database, like `device.json`). Changing them needs `printing.manage` (owners and
managers).

- **Receipt printers** speak ESC/POS (Epson TM and compatible 58 and 80 mm printers):
  - *Network*: the printer's IP address and port (usually 9100). Only addresses on the shop
    network are accepted (192.168.x.x, 10.x.x.x, 172.16–31.x.x, link-local and this PC).
  - *USB or installed in Windows*: install the printer maker's Windows driver first, then pick the
    printer by its Windows name. The app sends it raw printer commands, not a page.
  - *Serial*: the COM port and speed.
- **Label printers** speak ZPL (Zebra and compatibles) or TSPL (TSC, Xprinter, most others);
  choose the language and 203 or 300 dpi.
- **What prints where**: receipts, return slips, till-close summaries, held-bill slips and labels
  can each go to a printer, or stay with the system print dialog.
- **₹ and Indian scripts**: no receipt printer has ₹ in its character set. The app prints lines
  with ₹, Hindi or Malayalam as small pictures, using the Nirmala UI font that comes with Windows
  (or a font file you name). Choose *Rs.* per printer for plain text instead.
- **Cash drawer**: plug it into a receipt printer, tick *cash drawer attached* on that printer,
  choose it as the drawer and tick *open for cash sales*. *Open drawer (no sale)* needs
  `sales.open_drawer` (owners and managers) or a manager's approval, and is written to the audit
  log.
- **Counters and the hub's printer**: on the hub, tick *Share with counters* on a printer. A
  counter can then route documents to it; they go to the hub's **print queue**, which retries a
  failed job after 1, 2 and 5 minutes and then waits for someone to press *Retry*. The queue is
  shown on both the hub and the counters (cashiers see their own jobs). A counter's sale never
  opens the hub's drawer.
- **Test pages**: every printer has *Print a test page*. Check that the ruler line of digits fits
  on one line (else the paper width is wrong), that ₹ prints, and that the paper is cut.

| Problem shown | What to check |
|---|---|
| "can't be reached on the network" | Printer on, same network, IP address right (print the printer's own self-test page to see it). |
| "refused the connection" | Port (9100), or another PC holding the printer's only connection. |
| "isn't on this PC" | USB cable, Windows printer name, COM port. |
| "port is in use" | Another program (the maker's utility) has the serial port open. |
| Letters print as "?" | Turn on *print as a picture* for that printer, or check that a font was found. |

## 7. Reports, day close and settings

- **Reports** (sidebar, *Reports*): pick a report, choose the period and, where it applies, the
  store or account. *Print / PDF* opens the print dialog (choose "Save as PDF"); *CSV* and *Excel*
  need the `reports.export` permission. Tax summary ends with the net tax payable.
- **Expenses** record money paid out (now or on credit). A voided expense reverses its entries.
- **Accounts** shows balances; add bank or cash accounts here.
- **Day close** (*Accounts and reports → Day close*): count each till, check the summary and
  close the day. It refuses while a till is open. It is a record, not a lock (A-70).
- **Settings** (owner and permitted roles): business, tax, invoices, inventory, security,
  network, backup. Changes apply at once and are written to the audit log with before and after
  values. **Users and roles** are owner-only; the Owner role can't be edited.
- **Audit log** lists who did what and who approved it; it can't be edited or deleted.

## 8. Batches, expiry, price lists and stock valuation

- **Expired stock** (Settings → Inventory): *Block* (default) never sells an expired batch;
  *Warn* sells it only when nothing else is left and marks the bill; *Allow* sells it like any
  other. A counter selling while the hub is down always behaves as *Warn*. The warning window
  (default 30 days) drives the Expiry screen, the dashboard card and the notices.
- **Price lists** need `price_lists.manage` (owners and managers). A customer's list wins over
  the till's, the till's over the default list. The till's list applies once the till is opened
  for the shift. A counter that can't reach the hub uses the
  customer's list or the default list (it does not know the till's list). A list in use by a
  customer or a till can't be deleted: move them to another list first.
- **Stock valuation** (Settings → Inventory → Stock valuation, owner only): weighted average
  (the default) or FIFO. *Preview the switch* lists every item's quantity and value; the switch
  needs a reason and only runs if the figures still match the preview. It does not change the
  stock value or past profit, and it is refused while any item's stock is below zero (fix those
  first with a count or an adjustment). Under FIFO each receipt is a cost layer; sales use the
  oldest layers first, and the stock valuation report shows the oldest stock date.
- **Checks**: the weekly integrity check also compares the FIFO layers with the stock ledger. A
  difference shows as a notice; report it, do not adjust stock to hide it.

## 9. Locked days, year end, journals and the bank

- **Lock closed days** (Settings → Accounting, owner only): once on, anything dated on or before
  the last closed day (bills, payments, expenses, stock changes, journals, and voids of them)
  needs a manager's approval and is written to the audit log as `PERIOD_OVERRIDE`. To correct a
  closed day properly, the owner reopens it (Money → Periods and year end → Reopen, with a
  reason), makes the change, and closes the day again from Day close. A counter that was offline
  may still send a sale dated in a closed day: it is kept, audited and shown as a notice.
- **Year end** (Money → Periods and year end → Close the year, owner): only after the year has
  ended and every journal of the year is posted. Check the dry run (income, expenses, net
  profit), then close. The year is locked from then on; reopening reverses the closing entry.
- **Who does what** with journals: accountants and managers write and post; only the owner
  approves and allows control accounts. Change this in Settings → Users and roles.
- **Statement columns** are remembered per bank account. If your bank changes its file layout,
  import once with the new columns and tick *Remember these columns*.
- **Scheduled exports** run on the hub (or a standalone PC) while the app is open, once per
  period, into the folder you chose. A failed export (folder missing, user no longer allowed)
  shows as a notice and in the view's schedule.
- **Till differences**: the limit for a manager's approval is in Settings → Accounting (default
  ₹100).
