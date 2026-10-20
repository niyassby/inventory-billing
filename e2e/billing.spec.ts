import { expect, test } from "@playwright/test";
import { EMAIL, PASSWORD } from "./global-setup";

test("sign in, sell an item for cash, and see it in the invoices and reports", async ({ page }) => {
  const policyErrors: string[] = [];
  page.on("console", (m) => {
    if (/content security policy/i.test(m.text())) policyErrors.push(m.text());
  });
  const pageErrors: string[] = [];
  page.on("pageerror", (e) => {
    pageErrors.push(e.message);
    console.log("[pageerror]", e.message);
  });
  page.on("console", (m) => m.type() === "error" && console.log("[console]", m.text()));

  await page.goto("/");
  await page.getByLabel("E-mail").fill(EMAIL);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("link", { name: "Point of sale" })).toBeVisible();

  // The till: keyboard-first, scan by barcode and pay.
  await page.goto("/pos");
  const find = page.getByLabel("Find an item");
  await find.fill("8900000000017");
  await find.press("Enter");
  await expect(page.getByRole("row", { name: /E2E Basmati Rice/ })).toBeVisible();
  // Keyboard only: F10 opens the payment, Enter completes it.
  await expect(page.getByRole("button", { name: /^Pay/ })).toBeEnabled();
  await page.keyboard.press("F10");
  await expect(page.getByRole("button", { name: /Complete sale/ })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: /Sale complete/ })).toBeVisible();
  await expect(page.getByText(/125\.00/).first()).toBeVisible();

  // The same sale in the invoice list and the sales report.
  await page.goto("/sales/invoices");
  await expect(page.getByText(/125\.00/).first()).toBeVisible();
  await page.goto("/reports/sales-summary");
  await expect(page.getByRole("heading", { name: "Sales summary" })).toBeVisible();
  await expect(page.getByText(/125\.00/).first()).toBeVisible();

  // The command palette finds the item.
  await page.keyboard.press("Control+k");
  await page.getByPlaceholder(/Go to a screen/).fill("E2E Basmati");
  await expect(page.getByRole("option", { name: /E2E Basmati Rice/ })).toBeVisible();

  expect(policyErrors, "the Content-Security-Policy blocked something").toEqual([]);
  expect(pageErrors).toEqual([]);
});
