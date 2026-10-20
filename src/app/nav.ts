import type { LucideIcon } from "lucide-react";
import {
  ArrowLeftRight,
  ChartColumn,
  FileText,
  HandCoins,
  House,
  Landmark,
  Package,
  Receipt,
  Settings,
  ShoppingCart,
  Tags,
  Truck,
  Undo2,
  Users,
  ClipboardList,
  Wallet,
  CalendarCheck,
} from "lucide-react";

export interface NavItem {
  title: string;
  to: string;
  icon: LucideIcon;
  /** Shown only with this permission (the backend enforces it anyway). */
  permission?: string;
  /** Keyboard shortcut shown next to the item. */
  shortcut?: string;
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  {
    label: "Home",
    items: [{ title: "Dashboard", to: "/", icon: House, permission: "dashboard.view" }],
  },
  {
    label: "Sell",
    items: [
      {
        title: "Point of sale",
        to: "/pos",
        icon: ShoppingCart,
        permission: "sales.create",
        shortcut: "F2",
      },
      { title: "Invoices", to: "/sales/invoices", icon: Receipt, permission: "sales.view" },
      {
        title: "Quotations & orders",
        to: "/sales/documents",
        icon: FileText,
        permission: "sales.view",
      },
      { title: "Sales returns", to: "/sales/returns", icon: Undo2, permission: "sales.view" },
    ],
  },
  {
    label: "Stock",
    items: [
      { title: "Products", to: "/inventory/products", icon: Package, permission: "products.view" },
      {
        title: "Stock moves",
        to: "/inventory/stock",
        icon: ArrowLeftRight,
        permission: "stock.view",
      },
      {
        title: "Categories & units",
        to: "/inventory/masters",
        icon: Tags,
        permission: "products.view",
      },
      { title: "Purchases", to: "/purchases", icon: Truck, permission: "purchases.view" },
    ],
  },
  {
    label: "Contacts",
    items: [
      { title: "Customers", to: "/parties/customers", icon: Users, permission: "parties.view" },
      {
        title: "Suppliers",
        to: "/parties/suppliers",
        icon: ClipboardList,
        permission: "parties.view",
      },
    ],
  },
  {
    label: "Money",
    items: [
      { title: "Payments", to: "/payments", icon: HandCoins, permission: "payments.receive" },
      {
        title: "Expenses",
        to: "/accounting/expenses",
        icon: Wallet,
        permission: "expenses.manage",
      },
      {
        title: "Accounts",
        to: "/accounting/accounts",
        icon: Landmark,
        permission: "accounting.view",
      },
      {
        title: "Day close",
        to: "/accounting/day-close",
        icon: CalendarCheck,
        permission: "day_close.perform",
      },
    ],
  },
  {
    label: "Insights",
    items: [
      { title: "Reports", to: "/reports", icon: ChartColumn, permission: "reports.view" },
      { title: "Settings", to: "/settings", icon: Settings, permission: "settings.view" },
    ],
  },
  // {
  //   label: "Admin",
  //   items: [{ title: "Settings", to: "/settings", icon: Settings, permission: "settings.view" }],
  // },
];

/** The groups and items a user with these permissions can open. */
export function visibleNav(permissions: readonly string[]): NavGroup[] {
  return NAV.map((g) => ({
    ...g,
    items: g.items.filter((i) => !i.permission || permissions.includes(i.permission)),
  })).filter((g) => g.items.length > 0);
}

/** The item a path belongs to (longest matching prefix). */
export function activeItem(pathname: string): NavItem | undefined {
  let best: NavItem | undefined;
  for (const item of NAV.flatMap((g) => g.items)) {
    const matches =
      item.to === "/"
        ? pathname === "/"
        : pathname === item.to || pathname.startsWith(`${item.to}/`);
    if (matches && (!best || item.to.length > best.to.length)) best = item;
  }
  return best;
}

export interface Crumb {
  label: string;
  /** Present when the crumb is a link (not the page you are on). */
  to?: string;
}

function humanize(segment: string): string {
  if (segment === "new") return "New";
  if (segment === "edit") return "Edit";
  // Ids (UUIDs, document numbers) are not worth showing: "Details".
  if (/^[0-9a-f-]{8,}$/i.test(segment) && /\d/.test(segment)) return "Details";
  const text = segment.replace(/[-_]+/g, " ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Group › screen › detail, for the top bar. Empty for paths outside the menu. */
export function breadcrumbs(pathname: string): Crumb[] {
  const item = activeItem(pathname);
  if (!item) return [];
  const group = NAV.find((g) => g.items.includes(item));
  const rest = pathname
    .slice(item.to === "/" ? 1 : item.to.length)
    .split("/")
    .filter(Boolean);
  const crumbs: Crumb[] = [];
  if (group && group.label !== "Home") crumbs.push({ label: group.label });
  crumbs.push(rest.length > 0 ? { label: item.title, to: item.to } : { label: item.title });
  if (rest.length > 0) {
    const last = rest[rest.length - 1];
    crumbs.push({ label: humanize(last === "edit" && rest.length > 1 ? "edit" : (last ?? "")) });
  }
  return crumbs;
}
