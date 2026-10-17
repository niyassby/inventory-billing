import { describe, expect, it } from "vitest";
import { CURRENCIES } from "./money";
import {
  loginSchema,
  moneyInput,
  optionalText,
  partyInputSchema,
  passwordSchema,
  percentInput,
  pinSchema,
  productInputSchema,
  qtyInput,
  requiredText,
  roleSchema,
} from "./schemas";

function firstError(result: { success: boolean; error?: { issues: { message: string }[] } }) {
  return result.error?.issues[0]?.message;
}

describe("primitive schemas", () => {
  it("trims required text and enforces limits", () => {
    expect(requiredText("Name").parse("  Rice  ")).toBe("Rice");
    expect(firstError(requiredText("Name").safeParse("   "))).toBe("Name is required");
    expect(requiredText("Code", 3).safeParse("ABCD").success).toBe(false);
  });

  it("turns empty optional text into null", () => {
    expect(optionalText().parse("  ")).toBeNull();
    expect(optionalText().parse(" note ")).toBe("note");
  });

  it("parses money input", () => {
    expect(moneyInput().parse("1,250.50")).toBe(125050);
    expect(moneyInput(CURRENCIES.KWD).parse("1.250")).toBe(1250);
    expect(firstError(moneyInput().safeParse("-5"))).toBe("Amount cannot be negative");
    expect(moneyInput(CURRENCIES.INR, { allowNegative: true }).parse("-5")).toBe(-500);
    expect(firstError(moneyInput().safeParse("1.005"))).toMatch(/decimal places/);
    expect(firstError(moneyInput().safeParse(""))).toBe("Amount is required");
  });

  it("parses quantity input", () => {
    expect(qtyInput().parse("1.5")).toBe(1500);
    expect(firstError(qtyInput().safeParse("0"))).toMatch(/greater than zero/);
    expect(qtyInput({ allowZero: true }).parse("0")).toBe(0);
    expect(firstError(qtyInput({ maxDecimals: 0 }).safeParse("1.5"))).toMatch(/decimal/);
    expect(firstError(qtyInput().safeParse("-1"))).toMatch(/greater than zero/);
  });

  it("parses percentages into ppm", () => {
    expect(percentInput.parse("18")).toBe(180000);
    expect(percentInput.parse("0.25")).toBe(2500);
    expect(percentInput.safeParse("101").success).toBe(false);
    expect(percentInput.safeParse("abc").success).toBe(false);
    expect(firstError(percentInput.safeParse("0.123456"))).toMatch(/4 decimal places/);
  });
});

describe("auth schemas", () => {
  it("validates login", () => {
    expect(loginSchema.safeParse({ email: "owner@shop.in", password: "x" }).success).toBe(true);
    expect(loginSchema.safeParse({ email: "nope", password: "x" }).success).toBe(false);
    expect(loginSchema.safeParse({ email: "owner@shop.in", password: "" }).success).toBe(false);
  });

  it("enforces the password policy", () => {
    expect(passwordSchema.safeParse("correct horse 9").success).toBe(true);
    expect(passwordSchema.safeParse("short1").success).toBe(false);
    expect(passwordSchema.safeParse("onlyletterslong").success).toBe(false);
    expect(passwordSchema.safeParse("1234567890").success).toBe(false);
  });

  it("validates PINs and roles", () => {
    expect(pinSchema.safeParse("1234").success).toBe(true);
    expect(pinSchema.safeParse("12").success).toBe(false);
    expect(pinSchema.safeParse("12ab").success).toBe(false);
    expect(roleSchema.safeParse("cashier").success).toBe(true);
    expect(roleSchema.safeParse("admin").success).toBe(false);
  });
});

describe("partyInputSchema", () => {
  it("normalises optional fields", () => {
    const result = partyInputSchema.parse({
      name: " Ravi Traders ",
      phone: "+91 98765 43210",
      email: "",
      address: "",
      gstin: "27aapfu0939f1zv",
    });
    expect(result).toEqual({
      name: "Ravi Traders",
      phone: "+919876543210",
      email: null,
      address: null,
      gstin: "27AAPFU0939F1ZV",
    });
  });

  it("reports invalid contact details", () => {
    const base = { name: "X", phone: "", email: "", address: "", gstin: "" };
    expect(partyInputSchema.safeParse({ ...base, email: "bad" }).success).toBe(false);
    expect(partyInputSchema.safeParse({ ...base, phone: "12" }).success).toBe(false);
    expect(partyInputSchema.safeParse({ ...base, gstin: "27AAPFU0939F1ZX" }).success).toBe(false);
  });
});

describe("productInputSchema", () => {
  const valid = {
    name: "Basmati Rice 5kg",
    sku: "RICE-5KG",
    barcode: "8901234567890",
    categoryId: null,
    brandId: null,
    unitId: null,
    taxRateId: null,
    salePrice: "499.00",
    costPrice: "420",
    reorderLevel: "10",
    description: "",
  };

  it("converts form strings into domain values", () => {
    const p = productInputSchema().parse(valid);
    expect(p.salePrice).toBe(49900);
    expect(p.costPrice).toBe(42000);
    expect(p.reorderLevel).toBe(10000);
    expect(p.description).toBeNull();
  });

  it("allows an empty barcode and rejects a malformed SKU", () => {
    expect(productInputSchema().parse({ ...valid, barcode: "" }).barcode).toBeNull();
    expect(productInputSchema().safeParse({ ...valid, sku: "has space" }).success).toBe(false);
    expect(productInputSchema().safeParse({ ...valid, barcode: "12" }).success).toBe(false);
  });
});
