/// <reference types="node" />
import net from "node:net";
import { expect, test } from "@playwright/test";
import { EMAIL, PASSWORD } from "./global-setup";

/** A network printer on 127.0.0.1: every connection is one print job. */
async function fakePrinter(): Promise<{ port: number; jobs: Buffer[]; close: () => void }> {
  const jobs: Buffer[] = [];
  const server = net.createServer((socket) => {
    const parts: Buffer[] = [];
    socket.on("data", (d: Buffer) => parts.push(d));
    socket.on("end", () => jobs.push(Buffer.concat(parts)));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("no port");
  return { port: address.port, jobs, close: () => server.close() };
}

test("a sale prints on a network receipt printer and opens the drawer; labels go to the label printer", async ({
  page,
}) => {
  const receipts = await fakePrinter();
  const labels = await fakePrinter();
  const pageErrors: string[] = [];
  page.on("pageerror", (e) => pageErrors.push(e.message));

  await page.goto("/");
  await page.getByLabel("E-mail").fill(EMAIL);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("link", { name: "Point of sale" })).toBeVisible();

  // Settings → Printers: a receipt printer with a drawer, and a label printer.
  await page.goto("/settings/printers");
  await page.getByRole("button", { name: "Add printer" }).click();
  let dialog = page.getByRole("dialog");
  await dialog.getByLabel("Name").fill("Front counter");
  await dialog.getByLabel("IP address").fill("127.0.0.1");
  await dialog.getByLabel("Port").fill(String(receipts.port));
  await dialog.getByLabel("Rupee sign").click();
  await page.getByRole("option", { name: /Rs\./ }).click();
  await dialog.getByText("A cash drawer is plugged into this printer").click();
  await dialog.getByRole("button", { name: "Print a test page" }).click();
  await expect.poll(() => receipts.jobs.length).toBe(1);
  expect(receipts.jobs[0]?.toString("latin1")).toContain("Printer test");
  await dialog.getByRole("button", { name: "Keep" }).click();

  await page.getByRole("button", { name: "Add printer" }).click();
  dialog = page.getByRole("dialog");
  await dialog.getByLabel("Name").fill("Labels");
  await dialog.getByLabel("Kind").click();
  await page.getByRole("option", { name: /Label printer/ }).click();
  await dialog.getByLabel("IP address").fill("127.0.0.1");
  await dialog.getByLabel("Port").fill(String(labels.port));
  await dialog.getByRole("button", { name: "Keep" }).click();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByText("Printer settings saved")).toBeVisible();

  await page.getByLabel("Till receipts").click();
  await page.getByRole("option", { name: "Front counter" }).click();
  await page.getByLabel("Barcode labels").click();
  await page.getByRole("option", { name: "Labels" }).click();
  await page.getByText("Print the receipt as soon as a sale is saved").click();
  await page.getByLabel("Cash drawer").click();
  await page.getByRole("option", { name: "On Front counter" }).click();
  await page.getByText("Open the drawer for cash sales").click();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByText("Printer settings saved").first()).toBeVisible();

  // A cash sale prints by itself, drawer pulse first, with the total the screen showed.
  await page.goto("/pos");
  const find = page.getByLabel("Find an item");
  await find.fill("8900000000017");
  await find.press("Enter");
  await expect(page.getByRole("row", { name: /E2E Basmati Rice/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Pay/ })).toBeEnabled();
  await page.keyboard.press("F10");
  await expect(page.getByRole("button", { name: /Complete sale/ })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: /Sale complete/ })).toBeVisible();
  await expect.poll(() => receipts.jobs.length).toBe(2);
  const receipt = receipts.jobs[1]?.toString("latin1") ?? "";
  expect(receipt.startsWith("\x1b@\x1bt\x00\x1bp\x00")).toBe(true);
  expect(receipt).toContain("E2E Basmati Rice");
  expect(receipt).toContain("Rs.125.00");
  await page.keyboard.press("Enter");

  // Labels for the item, straight to the label printer (no print dialog).
  await page.goto("/inventory/labels");
  await page.getByLabel("Find an item").fill("8900000000017");
  await page.getByLabel("Find an item").press("Enter");
  await expect(page.getByRole("cell", { name: "E2E Basmati Rice", exact: true })).toBeVisible();
  await page.getByLabel("Labels for E2E Basmati Rice").fill("3");
  await expect(page.getByRole("img", { name: /Label for E2E Basmati Rice/ })).toBeVisible();
  await page.getByRole("button", { name: "Print 3 labels" }).click();
  await expect.poll(() => labels.jobs.length).toBe(1);
  const job = labels.jobs[0]?.toString("utf8") ?? "";
  expect(job.match(/\^XA/g)).toHaveLength(3);
  // 8900000000017 isn't a valid EAN-13, so the EAN template prints it as Code 128.
  expect(job).toContain("^BCN");

  // Back to the print dialog for the next tests.
  await page.goto("/settings/printers");
  await page.getByLabel("Till receipts").click();
  await page.getByRole("option", { name: "System print dialog" }).click();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByText("Printer settings saved").first()).toBeVisible();

  receipts.close();
  labels.close();
  expect(pageErrors).toEqual([]);
});
