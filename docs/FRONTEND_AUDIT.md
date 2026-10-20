# Frontend redesign: Phase 0 audit

Written 2026-10-20 from the code at commit `b5609be` plus the app running in Chrome against the
demo shop (screenshots of every main screen at 1366×768). Frontend only: no Rust or shared-package
changes are planned.

## 1. Screens (router in `src/App.tsx`)

`/pos` renders outside the shell (full screen). Everything else is inside `Shell`
(`src/app/shell.tsx`) and lazy-loaded. 12k lines of TSX in `src/features`.

| Route(s) | Feature file | Main components | Current problems | Priority |
|---|---|---|---|---|
| `/` | `dashboard/dashboard.tsx` (126 lines) | `Page`, `Section`, `QueryView`, `StatusBadge` | Only "today's bills" and notices: no KPIs, no chart, no low stock, no receivables/payables, no top products, no quick actions. Empty right-hand half of the screen. Notices icon uses raw `text-amber-500`. | **P0** |
| `/pos` | `pos/pos-page.tsx`, `payment-dialog.tsx`, `pos-dialogs.tsx` | `ProductPicker`, `PartyPicker`, line table, payment/receipt/till dialogs | Works, but: search field is a heavy bordered pill; cart table is plain; total is large but the "amount to pay" and tender flow are not prominent; payment dialog has no quick-tender buttons (check); no layout reservation for the search dropdown; hard-coded `text-amber-600`; shortcut hints at the bottom are tiny; no touch sizing. | **P0** |
| `/sales/invoices`, `/:id` | `sales/invoice-pages.tsx` (708) | `DataTable`, `Facts`, totals block | List is plain; detail page has no timeline; status badge tones are all `secondary`/`outline` (paid, finalized and received look identical). | P1 |
| `/sales/documents`, `/new`, `/:id`, `/:id/edit` | `sales/document-pages.tsx` (541) | `ItemLines`, `DataTable` | Same as above; editor is a long page with no sticky totals. | P1 |
| `/sales/returns`, `/new`, `/:id` | `sales/invoice-pages.tsx` | `DataTable`, `ItemLines` | Same. | P2 |
| `/purchases` + orders/receipts/bills/returns | `purchases/purchase-pages.tsx` (1094) | `DataTable`, `ItemLines`, `Facts` | Largest file; duplicated header/totals markup across 5 document kinds; tabs for kinds are inline. | P1 |
| `/inventory/products`, `/:id` | `inventory/products-page.tsx`, `product-editor.tsx` | `DataTable`, filters row, form | No avatar/initial, no stock badge (stock is coloured text only: colour-only meaning), category empty for some rows, filter row uses native select + checkboxes; editor is one long form. | **P0** |
| `/inventory/masters` | `masters-page.tsx` (431) | tabs of small tables and dialogs | Dense, no empty states (0 `EmptyState`). | P2 |
| `/inventory/stock`, adjustments, transfers, counts, documents, stock card | `stock-pages.tsx` (785) | `DataTable`, `ItemLines`, `WarehouseSelect` | Movement list has no type badges/direction colour; forms are long. | P1 |
| `/parties/customers`, `/suppliers`, `/:id`, `/payments` | `party-pages.tsx` (714), `payments-page.tsx` | `DataTable`, ledger table, dialogs | No balance chip; detail page is stacked sections; ledger numbers not colour-coded (due vs advance). | P1 |
| `/reports`, `/reports/:name` | `reports/reports.tsx` (460) | `ReportsIndex`, `ReportTableView`, preset chips | Index is a plain list of cards; report view has no chart; period control is a row of buttons plus dates. | P1 |
| `/accounting/expenses`, `/accounts`, `/day-close` | `accounting/accounting-pages.tsx` (742) | `DataTable`, dialogs, summary blocks | Day-close summary is text; no charts. | P2 |
| `/settings/*` | `settings/*` (≈1900 lines) | `SectionForm`, `UsersSection`, `RolesSection`, `AuditSection` | Per-section Save button at the bottom (not sticky); role permission grid is a long checkbox list; audit has filters but no empty/error styling. | P1 |
| Sign-in, change password, setup | `auth/sign-in.tsx`, `setup.tsx` (305) | `AuthFrame`, `Field` | Functional, unbranded centre card; no loading/skeleton. | P2 |

Cross-screen: only `products-page`, `audit-page` and part of lists use `EmptyState`/skeleton
consistently. Files with **zero** empty-state handling: `masters-page`, `settings-*`, `auth/*`,
`pos-*`. Hard-coded palette colours appear in 14 places (`connection-badge` 4, `pos-page` 2, and one
each in 8 other files); everything else already uses tokens. 87 uses of `tabular-nums`, 0
arbitrary pixel sizes.

## 2. Design system state

- **`src/index.css`**: Tailwind 4 `@theme inline` maps shadcn-style variables (`--background`,
  `--primary`, …). Light in `:root`, dark in `.dark`. **No semantic status tokens** (success,
  warning, danger, info), no surface/raised distinction, no type scale, no shadow or motion tokens.
  Radius token (`--radius`) exists but the components ignore it (see below). Figtree variable font
  is self-hosted (`@fontsource-variable/figtree`): fine with the CSP.
- **Uncommitted edits by you** (found in the working tree, not mine): indigo `--primary`, pink
  chart colours, `--radius: 0.3rem`, sidebar width 18rem, a `className="da rk"` on `Sidebar`
  (typo), `border-t` on the sidebar footer. I keep the **indigo accent** and the compact radius
  intent, replace the pink chart ramp with a coherent one, set the sidebar back to a narrower width,
  and drop the typo. They are folded into Phase 1.
- **`components.json`**: style `base-maia`, `tsx: false`, `iconLibrary: hugeicons`.
  Maia gives every control `rounded-4xl` (a pill): 28 of 52 primitives hard-code it, which fights
  both the radius token and the "Linear/Stripe" direction.
- **Primitives** (`src/components/ui`, 52 files): 24 are TypeScript, **28 are untyped `.jsx`**.
  Imported by application code: `button` 26 files, `input` 20, `native-select` 16, `field` 15,
  `dialog` 10, `checkbox` 9, `textarea` 8, `badge` 4, `tabs` 4, `tooltip` 3, `kbd` 2, `command`,
  `separator`, `sidebar`, `skeleton`, `sonner` 1 each. **34 primitives are never imported**
  (accordion, alert, alert-dialog, aspect-ratio, avatar, breadcrumb, button-group, calendar, card,
  chart, collapsible, combobox, context-menu, direction, drawer, dropdown-menu, empty, hover-card,
  input-group*, input-otp, item, label, menubar, navigation-menu, pagination, popover, progress,
  radio-group, resizable, scroll-area, select, sheet, slider, spinner, switch, table, toggle,
  toggle-group). (*`input-group` is used by `command`.) Nothing uses `Card`, `Table`, `Select`
  (native selects everywhere) or `DropdownMenu`; there is **no user menu**, no breadcrumb, no
  charts on any screen.
- **Icons**: `lucide-react` in 26 application files (A-8 decided it), `hugeicons` in 19 primitive
  files (the shadcn generator's default). Two libraries are mixed, one of them only inside
  primitives. Decision: **standardise on lucide**; replace the hugeicons usage in the primitives
  that stay and remove the dependency.
- **Dark theme** works since Phase 7 (it could not be switched on before). Never reviewed screen
  by screen.

## 3. Shell (`src/app/*`)

- `shell.tsx`: `Sidebar` (collapsible to icons), groups from `nav.ts` (Overview, Sell, Stock, Buy,
  People & money, Accounts and reports, Admin), `SidebarTrigger` + page title + search button +
  theme cycle button + `ConnectionBadge` + "New sale" in a 56 px header. Skip link, focus-to-main and
  window title (Phase 7). **F2** → `/pos` handled in `useGlobalKeys`; Ctrl+K palette in
  `command-palette.tsx`. The sidebar footer is the only user control ("Sign out").
- Problems: no breadcrumb or page-level context in the top bar (it only repeats the nav title),
  no user menu (profile, theme, sign-out), sidebar is wide and its group labels are loose, the
  "New sale" button and search have different shapes, connection badge uses raw colours.
- `nav.ts` (permission-aware, `visibleNav`, `activeItem`) is covered by `nav.test.ts` and must
  keep its exports. Group names in the brief (Sell, Stock, Contacts, Money, Insights, Admin) differ
  from today's; changing labels/grouping is a data change in `nav.ts` plus its test.

## 4. Shared app components (`src/components/app`)

| Component | State |
|---|---|
| `page.tsx` `Page`, `Section`, `EmptyState`, `ErrorState`, `LoadingRows`, `QueryView`, `Facts` | The right idea, used by every screen; plain styling, no breadcrumb, no sticky action area, `Section` always a card. |
| `data-table.tsx` `DataTable`, `Pager` | TanStack Table + Virtual, keyboard rows (↑/↓/Enter), per-page sorting, skeleton/empty/error built in. **No column visibility, no density, no sticky header, no sticky footer totals**; row height fixed 44 px. |
| `inputs.tsx` `MoneyInput`, `QtyInput`, `RateInput` | Fine behaviour; styling through `Input`. |
| `pickers.tsx` `SearchSelect`, `ProductPicker`, `PartyPicker` | Fine; the list is a custom absolutely positioned popover (not the `popover` primitive). |
| `dialogs.tsx` `ApprovalDialog`, `ConfirmDialog`, `confirm()` | Imperative confirm used across screens; styling follows `dialog`. |
| `item-lines.tsx` `ItemLines` | The document line editor used by sales docs, purchases, adjustments; table-in-form. |
| `badges.tsx` `StatusBadge` | Tones map to only `secondary`/`outline`/`destructive`: **paid, received, finalized and accepted are all the same grey**. This is the single biggest "status colours carry meaning" gap. |
| `warehouse-select.tsx` | Native select. |

## 5. Icons

See §2. Lucide is used consistently in app code; the mix exists only because primitives came
from the generator with hugeicons.

## 6. Constraints

- **e2e (`e2e/billing.spec.ts`)** relies on: label "E-mail" and "Password", button "Sign in",
  a *link* named "Point of sale" (sidebar), label "Find an item", a table *row* with the item name,
  a button matching `/^Pay/`, a button matching `/Complete sale/`, heading `/Sale complete/`,
  heading "Sales summary", the Ctrl+K placeholder `/Go to a screen/` and role `option`. These
  stay (or the test is updated in the same commit).
- **Vitest** has no component tests (`environment: node`, UI files are not in the coverage list).
  UI changes are verified by `typecheck`, `lint`, the e2e test and the screenshots. Pure helpers
  I add (for example chart data shaping, status tones) get unit tests.
- **CSP** (`src-tauri/tauri.conf.json`): `script-src 'self'`, `style-src 'self' 'unsafe-inline'`,
  `img-src 'self' data: blob:`, `font-src 'self' data:`, `connect-src 'self' ipc: …`. So: no remote
  fonts or images, no inline scripts, no `eval`. Recharts and the fontsource font are compatible
  (the e2e test serves the build with this policy and fails on any violation).
- **`npm run check`** = `check:api` + `typecheck` (strict, three tsconfigs) + `eslint` (strict type
  checked for TS; JS only warns) + `prettier --check` + vitest with an 80 % coverage gate on the
  listed logic modules. Converting `.jsx` primitives to TS moves them under the strict rules.
- Printing (`sales/print.ts`) builds its own HTML for an iframe and is independent of the theme.

## 7. Risks

1. **Behaviour parity**: 12k lines of screens and no component tests. Mitigation: change markup
   and classes, not hooks or handlers; keep existing props; screenshots before/after; the e2e test;
   typecheck catches prop drift.
2. **Your uncommitted theme edits** touch the files I rework first (see §2): handled, and called
   out in the final report.
3. **Generated primitives**: the shadcn/Base UI components are partly untyped. Converting all 28
   `.jsx` is large and most are unused: I convert those the redesign uses and **delete** the unused
   ones (git keeps them).
4. **Dashboard data**: there is no dashboard endpoint. KPIs must come from existing commands:
   `reports_run` (sales summary by day, item profit, party balances for receivables/payables,
   purchases, stock valuation, cash book) and `products_list` with `lowStockOnly`. Those need
   `reports.view`; users without it (a cashier) get a reduced dashboard.
5. **Keyboard flow at the till** (F1/F3/F4/F7/F8/F9/F10, ↑/↓, +/−, Delete, Enter in the payment
   dialog) must survive the redesign untouched.
6. **Bundle size**: charts add recharts (already a dependency and split into its own chunk) to
   the dashboard route only.
7. **Window sizes**: the app sets a 1024×640 minimum; layouts are designed for 1024 to 1920 wide.
8. **Naming mismatch**: "Contacts / Money / Insights" vs today's nav groups needs a nav-config
   change, kept permission-aware and tested.
