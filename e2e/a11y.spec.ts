import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { EMAIL, PASSWORD } from "./global-setup";

// axe injects its own script, so this file runs without the packaged CSP (billing.spec.ts keeps it).
test.use({ bypassCSP: true, viewport: { width: 1366, height: 768 } });

const SCREENS = [
  "/",
  "/pos",
  "/sales/invoices",
  "/inventory/products",
  "/inventory/products/new",
  "/purchases",
  "/parties/customers",
  "/reports",
  "/reports/sales-summary",
  "/accounting/expenses",
  "/settings/business",
  "/settings/users",
  "/settings/printers",
  "/inventory/labels",
  "/inventory/price-lists",
  "/inventory/expiry",
];

for (const theme of ["light", "dark"] as const) {
  test(`no WCAG A/AA violations in the ${theme} theme`, async ({ page }) => {
    await page.addInitScript((t) => localStorage.setItem("theme", t), theme);
    const check = async (name: string) => {
      await page.waitForLoadState("networkidle");
      const results = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
        .analyze();
      const summary = results.violations.map(
        (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(" | ")}`,
      );
      expect(summary, `${name} (${theme})`).toEqual([]);
    };

    await page.goto("/");
    await check("sign-in");
    await page.getByLabel("E-mail").fill(EMAIL);
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("link", { name: "Point of sale" })).toBeVisible();
    for (const route of SCREENS) {
      await page.goto(route);
      await expect(page.locator("main, #root").first()).toBeVisible();
      await check(route);
    }
  });
}
