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

## 5. Troubleshooting

| Symptom | Check |
|---|---|
| Counter can't find the hub | Same network? Hub app running in hub mode? Firewall allows private networks? Try the IP address instead of the list. |
| "The hub's certificate is not the one this counter was paired with" | The hub was reinstalled or its keychain entry was lost. Remove the counter on the hub and pair it again. |
| "No offline invoice numbers left" | The counter used its reserved block while offline. Reconnect to the hub; it reserves more automatically. |
| Hub shows "Hub not serving" | Port 47900 is used by another program, or the firewall blocks it. Change the port in Settings → Network, restart, and pair the counters again with the new address. |
| Backups fail | Folder missing, read-only or full; USB drive unplugged (only the second copy fails then). |

Logs for support are in `%LOCALAPPDATA%\com.niyas.tauri-app\logs` (one file per day, 14 days).
