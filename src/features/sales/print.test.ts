import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { defaultPaper, documentHtml, salesDocHtml } from "./print";

const settings = {
  business: {
    name: "Ravi <Stores>",
    address: "MG Road",
    phone: "98",
    gstin: "29AAGCB7383J1Z4",
    currency: "INR",
    currencyMinorDigits: 2,
    locale: "en-IN",
  },
  tax: {},
  invoice: { footerText: "Visit again", terms: "No returns after 7 days", paper: "thermal_58" },
  inventory: { negativeStockPolicy: "block" },
  offlineSalesPolicy: "allow_reserved_numbers",
} as Api.PublicSettings;

const invoice = {
  id: "i1",
  docNo: "INV/26-27/00001",
  businessDate: "2026-10-17",
  status: "finalized",
  paymentStatus: "paid",
  customerName: "Walk-in Customer",
  isInterState: false,
  pricesIncludeTax: false,
  gross: 20_000,
  lineDiscount: 1_000,
  billDiscount: 0,
  taxable: 19_000,
  taxTotal: 3_420,
  roundOff: -20,
  total: 22_400,
  amountPaid: 22_400,
  balanceDue: 0,
  snapshotSource: "live",
  origin: "offline",
  lines: [
    {
      id: "l1",
      lineNo: 1,
      variantId: "v",
      itemName: "Rice & Dal",
      sku: "R1",
      hsnSac: "1006",
      unitName: "kg",
      qty: 2000,
      unitPrice: 10_000,
      lineDiscount: 1_000,
      billDiscountShare: 0,
      taxable: 19_000,
      taxRate: 180_000,
      cgst: 1_710,
      sgst: 1_710,
      igst: 0,
      otherTax: 0,
      cess: 0,
      taxTotal: 3_420,
      lineTotal: 22_420,
      returnedQty: 0,
    },
  ],
  payments: [
    {
      id: "p",
      docNo: "",
      businessDate: "2026-10-17",
      mode: "cash",
      amount: 22_400,
      tendered: 25_000,
      changeGiven: 2_600,
      status: "posted",
    },
  ],
  taxSummary: [
    { taxRate: 180_000, taxable: 19_000, cgst: 1_710, sgst: 1_710, igst: 0, cess: 0, otherTax: 0 },
  ],
  warnings: [],
} as unknown as Api.InvoiceDetail;

describe("print", () => {
  it("makes a thermal receipt with escaped text, tax split and change", () => {
    const html = documentHtml(invoice, settings, "thermal_58");
    expect(html).toContain("size: 58mm auto");
    expect(html).toContain("Ravi &lt;Stores&gt;");
    expect(html).toContain("Rice &amp; Dal");
    expect(html).toContain("CGST ₹17.10");
    expect(html).toContain("Change");
    expect(html).toContain("Billed offline");
    expect(html).toContain("Visit again");
    expect(documentHtml(invoice, settings, "thermal_80")).toContain("size: 80mm auto");
  });

  it("makes an A4 tax invoice", () => {
    const html = documentHtml({ ...invoice, status: "voided", balanceDue: 100 }, settings, "a4");
    expect(html).toContain("TAX INVOICE");
    expect(html).toContain("VOIDED");
    expect(html).toContain("HSN 1006");
    expect(html).toContain("No returns after 7 days");
    expect(html).toContain("Balance due");
  });

  it("chooses the configured paper", () => {
    expect(defaultPaper(settings)).toBe("thermal_58");
    expect(defaultPaper(undefined)).toBe("thermal_80");
  });

  it("makes an A4 quotation", () => {
    const html = salesDocHtml(
      {
        id: "q",
        docNo: "QT/26-27/00001",
        docType: "quotation",
        businessDate: "2026-10-17",
        validUntil: "2026-10-31",
        status: "open",
        customerName: "Asha & Co",
        partyId: null,
        pricesIncludeTax: false,
        taxable: 19_000,
        taxTotal: 3_420,
        discount: 1_000,
        roundOff: -20,
        total: 22_400,
        notes: "Delivery in 2 days",
        terms: "50% advance",
        updatedAt: "",
        lines: [
          {
            variantId: "v",
            itemName: "Rice",
            qty: 2000,
            unitId: "u",
            unitPrice: 10_000,
            discount: 1_000,
            taxable: 19_000,
            taxTotal: 3_420,
            lineTotal: 22_420,
          },
        ],
      },
      settings,
    );
    expect(html).toContain("QUOTATION");
    expect(html).toContain("Valid until");
    expect(html).toContain("Asha &amp; Co");
    expect(html).toContain("50% advance");
    expect(html).toContain("Round off");
  });
});
