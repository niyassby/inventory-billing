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
    label: "Overview",
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
    ],
  },
  {
    label: "Buy",
    items: [{ title: "Purchases", to: "/purchases", icon: Truck, permission: "purchases.view" }],
  },
  {
    label: "People & money",
    items: [
      { title: "Customers", to: "/parties/customers", icon: Users, permission: "parties.view" },
      {
        title: "Suppliers",
        to: "/parties/suppliers",
        icon: ClipboardList,
        permission: "parties.view",
      },
      { title: "Payments", to: "/payments", icon: HandCoins, permission: "payments.receive" },
    ],
  },
  {
    label: "Coming next",
    items: [
      { title: "Reports", to: "/reports", icon: ChartColumn, permission: "reports.view" },
      { title: "Accounts", to: "/accounting", icon: Landmark, permission: "accounting.view" },
      { title: "Settings", to: "/settings", icon: Settings, permission: "settings.view" },
    ],
  },
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
