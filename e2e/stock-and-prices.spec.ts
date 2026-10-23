import { expect, test } from "@playwright/test";
import { EMAIL, PASSWORD, call } from "./global-setup";

test("a till's price list prices the sale; an expired batch is written off", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (e) => pageErrors.push(e.message));

  // An expired batch of a batch-tracked item, set up through the API the screens use.
  const session = await call<{ token: string }>("auth_login", "", {
    email: EMAIL,
    password: PASSWORD,
  });
  const units = await call<{ id: string }[]>("units_list", session.token, {});
  await call("products_save", session.token, {
    name: "E2E Cough Syrup",
    baseUnitId: units[0]?.id,
    trackBatches: true,
    trackExpiry: true,
    variants: [
      {
        sku: "E2E-SYRUP",
        barcode: "8900000000024",
        salePrice: 9_000,
        costPrice: 6_000,
        openingStock: 4_000,
        openingBatchNo: "E2E-OLD",
        openingExpiryDate: "2025-01-31",
      },
    ],
  });

  await page.goto("/");
  await page.getByLabel("E-mail").fill(EMAIL);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("link", { name: "Point of sale" })).toBeVisible();

  // A price list with the rice at ₹110, given to the till.
  await page.goto("/inventory/price-lists");
  await page.getByRole("button", { name: "New price list" }).click();
  await page.getByRole("dialog").getByLabel("Name").fill("E2E Counter prices");
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Price list saved")).toBeVisible();
  const find = page.getByLabel("Find an item");
  await find.fill("8900000000017");
  await find.press("Enter");
  await expect(page.getByRole("dialog").getByText("E2E Basmati Rice")).toBeVisible();
  await page.getByRole("dialog").getByLabel("Price", { exact: true }).fill("110");
  await page.getByRole("button", { name: "Save price" }).click();
  await expect(page.getByRole("cell", { name: "₹110.00" })).toBeVisible();
  const till = page.locator('[id^="till-"]').first();
  await till.click();
  await page.getByRole("option", { name: "E2E Counter prices" }).click();
  await expect(page.getByText("Till's price list changed")).toBeVisible();

  // The open till sells at the list price.
  await page.goto("/pos");
  const tillButton = page.getByRole("button", { name: /Open till|Till open/ });
  await expect(tillButton).toBeVisible();
  if ((await tillButton.textContent())?.includes("Open till")) {
    await tillButton.click();
    await page.getByLabel("Opening cash").fill("500");
    await page.getByRole("dialog").getByRole("button", { name: "Open till" }).click();
    await expect(page.getByRole("button", { name: /Till open/ })).toBeVisible();
  }
  const scan = page.getByLabel("Find an item");
  await scan.fill("8900000000017");
  await scan.press("Enter");
  await expect(page.getByRole("row", { name: /E2E Basmati Rice/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Pay/ })).toContainText("110.00");
  await page.keyboard.press("F10");
  await expect(page.getByRole("button", { name: /Complete sale/ })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: /Sale complete/ })).toBeVisible();
  await expect(page.getByText(/110\.00/).first()).toBeVisible();
  await page.keyboard.press("Enter");

  // The till goes back to product prices for the other tests.
  await page.goto("/inventory/price-lists");
  await page.locator('[id^="till-"]').first().click();
  await page.getByRole("option", { name: "Default list / product prices" }).click();
  await expect(page.getByText("Till's price list changed")).toBeVisible();

  // The expired batch: on the dashboard, on the expiry screen, then written off.
  await page.goto("/");
  await expect(page.getByText("Expiry", { exact: true }).first()).toBeVisible();
  await page.goto("/inventory/expiry");
  const row = page.getByRole("row", { name: /E2E Cough Syrup/ });
  await expect(row).toContainText("E2E-OLD");
  await expect(row).toContainText("Expired");
  await row.getByRole("button", { name: "Write off" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Write off" }).click();
  await expect(page.getByText(/Written off as/)).toBeVisible();
  await expect(page.getByRole("row", { name: /E2E Cough Syrup/ })).toHaveCount(0);

  expect(pageErrors).toEqual([]);
});
