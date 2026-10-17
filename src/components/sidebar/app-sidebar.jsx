"use client";

import * as React from "react";
import { APP_NAME } from "@inventory/shared";

import { NavMain } from "@/components/sidebar/nav-main";
import { NavProjects } from "@/components/sidebar/nav-projects";
import { NavUser } from "@/components/sidebar/nav-user";
import { TeamSwitcher } from "@/components/sidebar/team-switcher";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from "@/components/ui/sidebar";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  LayoutBottomIcon,
  AudioWave01Icon,
  CommandIcon,
  ComputerTerminalIcon,
  RoboticIcon,
  BookOpen02Icon,
  Settings05Icon,
  CropIcon,
  PieChartIcon,
  MapsIcon,
} from "@hugeicons/core-free-icons";
import {
  Analytics01Icon,
  Contact02Icon,
  Home01Icon,
  Invoice01Icon,
  Notification01Icon,
  Preferences,
  SettingsIcon,
  ShoppingBasket01Icon,
  Wallet01Icon,
} from "@hugeicons/core-free-icons/index";

// Navigation and branding. Phase 7 replaces this with role-aware navigation.
const data = {
  teams: [
    {
      name: APP_NAME,
      logo: <HugeiconsIcon icon={LayoutBottomIcon} strokeWidth={2} />,
      plan: "Standalone",
    },
  ],
  navMain: [
    {
      title: "Dashboard",
      url: "/dashboard",
      icon: <HugeiconsIcon icon={Home01Icon} strokeWidth={2} />,
      items: [
        {
          title: "Reports",
          url: "/dashboard/reports",
        },
      ],
    },
    {
      title: "Inventory",
      url: "/inventory",
      icon: <HugeiconsIcon icon={ShoppingBasket01Icon} strokeWidth={2} />,
      items: [
        {
          title: "All Products",
          url: "/inventory/products",
        },
        {
          title: "Categories & Brands",
          url: "/inventory/categories",
        },
        {
          title: "Stock Adjustments",
          url: "/inventory/adjustments",
        },
        {
          title: "Print Barcodes/Labels",
          url: "/inventory/barcodes",
        },
      ],
    },
    {
      title: "Sales",
      url: "/sales",
      icon: <HugeiconsIcon icon={Invoice01Icon} strokeWidth={2} />,
      items: [
        {
          title: "Invoices",
          url: "/sales/invoices",
        },
        {
          title: "Quotations / Estimates",
          url: "/sales/quotations",
        },
        {
          title: "Sales Returns / Credit Notes",
          url: "/sales/returns",
        },
        {
          title: "Point of Sale (POS)",
          url: "/sales/pos",
        },
      ],
    },
    {
      title: "Purchases",
      url: "/purchases",
      icon: <HugeiconsIcon icon={Invoice01Icon} strokeWidth={2} />,
      items: [
        {
          title: "Purchase Orders",
          url: "/purchases/orders",
        },
        {
          title: "Supplier Bills",
          url: "/purchases/bills",
        },
        {
          title: "Purchase Returns",
          url: "/purchases/returns",
        },
      ],
    },
    {
      title: "Contacts (CRM)",
      url: "/contacts",
      icon: <HugeiconsIcon icon={Contact02Icon} strokeWidth={2} />,
      items: [
        {
          title: "Customers",
          url: "/contacts/customers",
        },
        {
          title: "Leads & Deals",
          url: "/contacts/leads",
        },
        {
          title: "Suppliers / Vendors",
          url: "/contacts/suppliers",
        },
      ],
    },
    {
      title: "Accounting",
      url: "/accounting",
      icon: <HugeiconsIcon icon={Wallet01Icon} strokeWidth={2} />,
      items: [
        {
          title: "Chart of Accounts",
          url: "/accounting/chart-of-accounts",
        },
        {
          title: "Journal Entries",
          url: "/accounting/journals",
        },
        {
          title: "Expenses",
          url: "/accounting/expenses",
        },
        {
          title: "Payments Received",
          url: "/accounting/payments-received",
        },
        {
          title: "Tax Rates & Rules",
          url: "/accounting/taxes",
        },
      ],
    },
    {
      title: "Reports",
      url: "/reports",
      icon: <HugeiconsIcon icon={Analytics01Icon} strokeWidth={2} />,
      items: [
        {
          title: "Profit & Loss",
          url: "/reports/profit-and-loss",
        },
        {
          title: "Sales Summary",
          url: "/reports/sales",
        },
        {
          title: "Stock Valuation",
          url: "/reports/stock-valuation",
        },
        {
          title: "Tax Summary",
          url: "/reports/tax",
        },
      ],
    },
    {
      title: "Settings",
      url: "/settings",
      icon: <HugeiconsIcon icon={SettingsIcon} strokeWidth={2} />,
      items: [
        {
          title: "Company Profile",
          url: "/settings/company",
        },
        {
          title: "Users & Roles",
          url: "/settings/users",
        },
        {
          title: "Preferences (Currency, Timezone)",
          url: "/settings/preferences",
        },
        {
          title: "Invoice Templates",
          url: "/settings/templates",
        },
        {
          title: "DB Maintenance",
          url: "/settings/maintenance",
        },
      ],
    },
  ],
  projects: [
    {
      name: "Notifications",
      url: "#",
      icon: <HugeiconsIcon icon={Notification01Icon} strokeWidth={2} />,
    },
    {
      name: "Preferences",
      url: "#",
      icon: <HugeiconsIcon icon={Preferences} strokeWidth={2} />,
    },
  ],
};

import { useStore } from "@/stores/app-store";

export function AppSidebar({ ...props }) {
  const user = useStore((state) => state.user);
  return (
    <Sidebar collapsible="icon" {...props}>
      <SidebarHeader>
        <TeamSwitcher teams={data.teams} />
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={data.navMain} />
        <NavProjects projects={data.projects} />
      </SidebarContent>
      <SidebarFooter>
        <NavUser user={user} />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
