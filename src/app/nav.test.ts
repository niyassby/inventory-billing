import { describe, expect, it } from "vitest";
import { activeItem, visibleNav } from "./nav";

describe("navigation", () => {
  it("shows only what the role may open", () => {
    const cashier = visibleNav(["sales.create", "sales.view", "products.view"]);
    const titles = cashier.flatMap((g) => g.items.map((i) => i.title));
    expect(titles).toContain("Point of sale");
    expect(titles).toContain("Products");
    expect(titles).not.toContain("Purchases");
    expect(cashier.find((g) => g.label === "Buy")).toBeUndefined();
  });

  it("finds the active item by the longest prefix", () => {
    expect(activeItem("/")?.title).toBe("Dashboard");
    expect(activeItem("/sales/invoices/abc")?.title).toBe("Invoices");
    expect(activeItem("/inventory/products/new")?.title).toBe("Products");
    expect(activeItem("/nowhere")).toBeUndefined();
  });
});
