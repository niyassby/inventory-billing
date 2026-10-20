import { describe, expect, it } from "vitest";
import { activeItem, breadcrumbs, visibleNav } from "./nav";

describe("navigation", () => {
  it("shows only what the role may open", () => {
    const cashier = visibleNav(["sales.create", "sales.view", "products.view"]);
    const titles = cashier.flatMap((g) => g.items.map((i) => i.title));
    expect(titles).toContain("Point of sale");
    expect(titles).toContain("Products");
    expect(titles).not.toContain("Purchases");
    expect(cashier.find((g) => g.label === "Money")).toBeUndefined();
  });

  it("finds the active item by the longest prefix", () => {
    expect(activeItem("/")?.title).toBe("Dashboard");
    expect(activeItem("/sales/invoices/abc")?.title).toBe("Invoices");
    expect(activeItem("/inventory/products/new")?.title).toBe("Products");
    expect(activeItem("/nowhere")).toBeUndefined();
  });

  it("builds breadcrumbs: group, screen, then the detail", () => {
    expect(breadcrumbs("/")).toEqual([{ label: "Dashboard" }]);
    expect(breadcrumbs("/sales/invoices")).toEqual([{ label: "Sell" }, { label: "Invoices" }]);
    expect(breadcrumbs("/inventory/products/new")).toEqual([
      { label: "Stock" },
      { label: "Products", to: "/inventory/products" },
      { label: "New" },
    ]);
    expect(breadcrumbs("/sales/invoices/01a15848-9bc8-7841")).toEqual([
      { label: "Sell" },
      { label: "Invoices", to: "/sales/invoices" },
      { label: "Details" },
    ]);
    expect(breadcrumbs("/reports/sales-summary").at(-1)).toEqual({ label: "Sales summary" });
    expect(breadcrumbs("/nowhere")).toEqual([]);
  });
});
