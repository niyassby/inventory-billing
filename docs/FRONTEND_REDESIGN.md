# Frontend redesign: final report

Frontend only; no Rust, API or shared-package changes. The Phase 0 audit is in
[FRONTEND_AUDIT.md](FRONTEND_AUDIT.md). Commits: `96eaa8c` (design system), `336c16d` (shell),
`3c14dd0` (shared components), `79fe2a6` (dashboard and till), `1a4048d` (sales, purchases, stock,
parties), `4146a6c` (reports, settings, sign-in) and the polish commit.

## 1. The design language

**Tokens** (`src/index.css`, one place, light and dark, oklch):

- Surfaces: `background` (page), `card` (panels), `raised` (menus, dialogs, toasts), `muted`,
  `accent`; text `foreground` / `muted-foreground`; `border`, `input`, `ring`.
- One restrained brand accent, indigo (`primary`, `primary-soft`).
- Status colours, reserved for meaning, each with a solid and a `-soft` tint:
  `success` (paid, received), `warning` (due, low stock), `danger` (void, out of stock, errors),
  `info` (informational, open).
- Chart ramp `chart-1..5`; sidebar tokens; radius scale (`xs` 3px to `2xl` 16px from `--radius:
  0.5rem`); three shadow levels plus `xs`; motion 150 ms with an ease-out curve, and
  `prefers-reduced-motion` switches it off.
- **Type scale** as named utilities: `text-display` (till total), `text-page-title`,
  `text-section-title`, body 14 px, `text-table` 13 px, `text-caption`, and `num` (tabular figures,
  no wrap) for every money, quantity and table number. Figtree kept.
- **Density**: comfortable / compact (`html[data-density]`) shrinks type and table rows.
- Both themes; **system** option through `next-themes`.

**Components**: all 27 remaining primitives in `src/components/ui` are TypeScript. Radius follows
the token (the generator's `rounded-4xl` pills are gone), inputs are white with a quiet shadow,
buttons have `soft`, solid `destructive` and an `xl` till size, badges have `success`, `warning`,
`info`. New: `Alert` (inline notices), `Avatar` (letter avatars). Icons are **lucide only**.

**App components**: `Page` (title, description, actions, toolbar), `Section`, empty / error /
loading states, `DataTable` (sticky header, density-aware rows, **column choice** remembered per
table, keyboard rows, virtualised when long), `StatusBadge` (colour by meaning), `StockBadge`,
`BalanceChip`, `SearchInput`, `Timeline`, `SaveBar` (sticky "unsaved changes"), `TrendChart`.

## 2. Screen by screen

| Screen | Before | After |
|---|---|---|
| Shell | Wide sidebar, title echo, one button for sign-out | 15.5 rem grouped sidebar (Sell, Stock, Contacts, Money, Insights, Admin) with an active indicator; breadcrumbs; search (Ctrl K); connection chip; theme button; New sale; user menu (spacing, sign out). Skip link and F2 kept. |
| Dashboard | Today's bills and notices | 6 KPI cards (sales with change vs yesterday, bills, purchases, receivables, payables, low stock), 14-day sales chart, top sellers, today's bills, low-stock list, notices, quick actions. Cashiers see only what they may. |
| Point of sale | Plain panels | Large total (`text-display`), customer chip, selected-row accent, big quantities, 48 px Pay button, offline/hub notice, **quick-tender buttons** (exact + round notes) in the payment dialog; all F-keys unchanged; a whole sale is **keyboard-only** (tested). |
| Invoices, quotations, returns, purchases | Grey statuses | Coloured statuses, search with icon, column choice; invoice detail adds a **timeline** (issued, payments, returns, void). |
| Products | Coloured stock text | Avatar, **Low / Out of stock badges** (not colour alone), column choice, required markers on the editor, sticky save bar. |
| Customers and suppliers | Number in a fact | **Balance chip** (They owe / Paid ahead / Settled) beside the name; amber amounts in lists. |
| Reports | List of text cards | Icon cards; any one-row-per-day report gets a **trend chart**; calmer table with a totals bar. |
| Expenses, accounts, day close | Cards | Same page frame, tokens, states. |
| Settings | Save button at the bottom | Sticky **save bar** when something changed; users with avatars and role chips; **role permissions as a grid of areas and action chips** instead of 40 checkboxes. |
| Sign-in, setup, new password | Centre card | Branded split layout (product promise panel, form card); collapses to the card on small windows. |

## 3. Files

Added: `src/app/user-menu.tsx`, `src/components/app/{charts,save-bar,search-input,status,timeline}.ts(x)`
(+ tests), `src/components/ui/{alert,avatar}.tsx`, `src/features/dashboard/metrics.ts` (+ test),
`src/features/reports/chart.ts` (+ test), `src/features/sales/timeline.ts` (+ test),
`src/features/settings/{permission-grid.tsx,permissions.ts}` (+ test), `src/features/dev/kitchen-sink.tsx`
(dev-only route `/dev/ui`), `e2e/a11y.spec.ts`, `docs/FRONTEND_AUDIT.md`, this file.

Rewritten or restyled: `src/index.css`, every remaining primitive, `src/app/{shell,nav,command-palette}`,
`DataTable`, `page.tsx`, `badges.tsx`, pickers, line editor, and the screens listed above.

Removed: 29 unused or untyped primitives (accordion, calendar, chart, combobox, context-menu,
drawer, menubar, navigation-menu, pagination, resizable, select, slider, switch, toggle, …); the
`hugeicons`, `vaul`, `input-otp`, `react-resizable-panels`, `react-day-picker`, `date-fns`,
`react-barcode` and `react-qr-code` dependencies (none was imported). Added: `react-is` (a
`recharts` peer), `@axe-core/playwright` (dev).

## 4. Skipped or deferred

- **Saved table views**: only column choice is remembered (per PC). Filters and sorting are not
  saved.
- **Server-side sorting**: tables still sort the loaded page (the API pages and sorts by its own
  keys); that is a backend change.
- **POS recent and favourite items**: need somewhere to keep them per user (see §5).
- **Report tables are not virtualised** (capped at 5,000 rows by the backend); charts only for
  one-row-per-day reports.
- **Setup wizard**: reframed, not re-laid-out.
- **Masters, settings sections, audit**: tokens and components only; a few still lack bespoke empty
  states.
- **Windows below 1024 px** (the app's minimum) are not designed for.
- Screens were checked in Chrome at 1366×768, 1024×640 and 1920×1080 and by axe in both themes; **not
  inside the packaged Tauri window** (A-80 still applies) and not with a screen reader.

## 5. Backend-dependent ideas

Still to do: 1, 2, 3, 6, 7 and 8. Done: 4 and 5.

1. A single `dashboard_summary` command (KPIs, trend, top sellers, low stock): the dashboard now
   makes seven requests, which a hub or a slow LAN notices.
2. Overdue ageing for receivables (due dates on invoices, buckets) so "overdue" can be a real status.
3. Per-user preferences on the hub (theme, density, saved table views, POS favourites and recent
   items), instead of per-PC browser storage.
4. ~~Server-side sort and filter keys for every list, so sorting works across pages.~~ **Done**: every list
   command takes `sort` (a whitelisted key, never a column name) and `descending`, plus page and page
   size; the tables keep that state in the address (A-91).
5. ~~Product images (the schema has `image_path`; there is no upload or serve command).~~ **Done**: upload,
   replace, remove, thumbnails in the list, search, till and dashboard, served through the hub (A-90).
6. A customer "recent documents and last payment" endpoint for the party page header.
7. Stock history per item for sparklines on the product page.
8. A "restore on first start" screen (A-79).

## 6. How to run and review

```bash
npm run dev:bridge      # the Rust backend over HTTP on 127.0.0.1:1430 (first build is slow)
npm run dev             # the UI on http://localhost:1420
```

Sign in with the demo shop: `demo@example.test` / `Demo-shop-2026` (test data only). The design
system page is `http://localhost:1420/dev/ui` (development builds only): use the sun/moon button for
dark. **Ctrl K** opens search; the account menu (top right) sets spacing; **F2** opens the till.

Checks: `npm run check` (types, lint, format, unit tests with coverage), `npm run test:e2e` (the
billing flow with the packaged CSP, plus an axe accessibility scan of the main screens in both
themes).
