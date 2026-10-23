/// <reference types="node" />
import { expect, test } from "@playwright/test";
import { EMAIL, PASSWORD } from "./global-setup";

test("a journal is approved and posted, a statement imported, the audit trail exported", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (e) => pageErrors.push(e.message));

  await page.goto("/");
  await page.getByLabel("E-mail").fill(EMAIL);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("link", { name: "Point of sale" })).toBeVisible();

  // A journal: written, submitted, approved by the owner, posted.
  await page.goto("/accounting/journals");
  await page.getByRole("button", { name: "New journal" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Narration").fill("E2E bank fee correction");
  await dialog.getByLabel("Account, line 1").click();
  await page.getByRole("option", { name: /General Expenses/ }).click();
  await dialog.getByLabel("Debit, line 1").fill("150");
  await dialog.getByLabel("Account, line 2").click();
  await page.getByRole("option", { name: /Cash in Hand/ }).click();
  await dialog.getByLabel("Credit, line 2").fill("150");
  await expect(dialog.getByText("Balanced")).toBeVisible();
  await dialog.getByRole("button", { name: "Save draft" }).click();
  await expect(page.getByText(/JV\/.+ saved/).first()).toBeVisible();
  await dialog.getByRole("button", { name: "Submit for approval" }).click();
  await expect(page.getByText(/JV\/.+ submitted/).first()).toBeVisible();
  await dialog.getByRole("button", { name: "Approve" }).click();
  await expect(page.getByText(/JV\/.+ approved/).first()).toBeVisible();
  await dialog.getByRole("button", { name: "Post to the books" }).click();
  await expect(page.getByText(/JV\/.+ posted/).first()).toBeVisible();
  await expect(dialog.getByText("Posted", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "Close" }).last().click();

  // It is in the day book, tagged as a manual journal.
  await page.goto("/reports/day-book");
  await expect(page.getByRole("cell", { name: "manual journal" }).first()).toBeVisible();

  // A bank statement: columns chosen once, previewed, imported; matching runs.
  const today = new Date();
  const dd = String(today.getDate()).padStart(2, "0");
  const mm = String(today.getMonth() + 1).padStart(2, "0");
  await page.goto("/accounting/bank");
  await page.getByRole("button", { name: "Import statement" }).click();
  const importer = page.getByRole("dialog");
  await importer.getByLabel("Statement file").setInputFiles({
    name: "statement.csv",
    mimeType: "text/csv",
    buffer: Buffer.from(
      `Date,Narration,Amount\n${dd}/${mm}/${today.getFullYear()},E2E SMS CHARGES,-5.90\n`,
    ),
  });
  // The columns are guessed from the header (date, narration, amount).
  await expect(importer.getByText("1 new")).toBeVisible();
  await importer.getByRole("button", { name: "Import 1 line(s)" }).click();
  await expect(page.getByText(/1 line\(s\) imported/)).toBeVisible();
  await expect(page.getByText("E2E SMS CHARGES")).toBeVisible();
  await page.getByRole("button", { name: "Match automatically" }).click();
  await expect(page.getByText(/line\(s\) matched/)).toBeVisible();
  await expect(page.getByText("Reconciliation statement")).toBeVisible();

  // The audit trail, exported with the viewer's filters.
  await page.goto("/settings/audit");
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "CSV" }).click();
  expect((await download).suggestedFilename()).toMatch(/^audit-log-.*\.csv$/);

  // Year end and locked days have their screen.
  await page.goto("/accounting/periods");
  await expect(page.getByText("Financial years")).toBeVisible();

  expect(pageErrors).toEqual([]);
});
