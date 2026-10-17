import { z } from "zod";
import { isValidGstin } from "./gstin";
import type { Currency } from "./money";
import { DEFAULT_CURRENCY, MoneyError, parseMoney } from "./money";
import { QTY_DECIMALS, QtyError, parseQty } from "./qty";
import { PAYMENT_MODES, ROLES } from "./constants";
import { percentToRate, TaxError } from "./tax";

/**
 * Form schemas. Inputs are what the user types (strings); outputs are domain values (Money, Qty,
 * rates in ppm). The backend validates again; these exist for instant, friendly inline errors.
 */

const trimmed = z.string().trim();

export const requiredText = (label: string, max = 200) =>
  trimmed.min(1, `${label} is required`).max(max, `${label} must be at most ${max} characters`);

export const optionalText = (max = 500) =>
  trimmed.max(max, `Must be at most ${max} characters`).transform((v) => (v === "" ? null : v));

export const optionalEmail = trimmed
  .transform((v) => (v === "" ? null : v))
  .pipe(z.email("Enter a valid email address").nullable());

export const optionalPhone = trimmed
  .transform((v) => (v === "" ? null : v.replace(/[\s()-]/g, "")))
  .pipe(
    z
      .string()
      .regex(/^\+?[0-9]{6,15}$/, "Enter a valid phone number")
      .nullable(),
  );

export const optionalGstin = trimmed
  .transform((v) => (v === "" ? null : v.toUpperCase()))
  .refine((v) => v === null || isValidGstin(v), "Enter a valid 15-character GSTIN");

/** A money amount typed by the user, e.g. "1,250.50" → Money(125050). */
export const moneyInput = (
  currency: Currency = DEFAULT_CURRENCY,
  opts: { allowNegative?: boolean } = {},
) =>
  trimmed.min(1, "Amount is required").transform((value, ctx) => {
    try {
      const amount = parseMoney(value, currency);
      if (!opts.allowNegative && amount < 0) {
        ctx.addIssue({ code: "custom", message: "Amount cannot be negative" });
        return z.NEVER;
      }
      return amount;
    } catch (e) {
      ctx.addIssue({
        code: "custom",
        message: e instanceof MoneyError ? e.message : "Invalid amount",
      });
      return z.NEVER;
    }
  });

/** A quantity typed by the user, e.g. "1.5" → Qty(1500). */
export const qtyInput = (opts: { maxDecimals?: number; allowZero?: boolean } = {}) =>
  trimmed.min(1, "Quantity is required").transform((value, ctx) => {
    try {
      const q = parseQty(value, opts.maxDecimals ?? QTY_DECIMALS);
      if (q < 0 || (q === 0 && !opts.allowZero)) {
        ctx.addIssue({ code: "custom", message: "Quantity must be greater than zero" });
        return z.NEVER;
      }
      return q;
    } catch (e) {
      ctx.addIssue({
        code: "custom",
        message: e instanceof QtyError ? e.message : "Invalid quantity",
      });
      return z.NEVER;
    }
  });

/** A percentage typed by the user, e.g. "18" or "0.25" → rate in ppm. */
export const percentInput = trimmed.min(1, "Rate is required").transform((value, ctx) => {
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0 || n > 100) {
    ctx.addIssue({ code: "custom", message: "Enter a percentage between 0 and 100" });
    return z.NEVER;
  }
  try {
    return percentToRate(n);
  } catch (e) {
    ctx.addIssue({ code: "custom", message: e instanceof TaxError ? e.message : "Invalid rate" });
    return z.NEVER;
  }
});

export const loginSchema = z.object({
  email: z.email("Enter a valid email address"),
  password: z.string().min(1, "Password is required"),
});
export type LoginInput = z.input<typeof loginSchema>;

/** Default password policy (configurable in Settings → Security in Phase 6). */
export const passwordSchema = z
  .string()
  .min(10, "Use at least 10 characters")
  .max(128, "Use at most 128 characters")
  .regex(/[A-Za-z]/, "Include at least one letter")
  .regex(/[0-9]/, "Include at least one number");

export const pinSchema = z.string().regex(/^[0-9]{4,6}$/, "PIN must be 4 to 6 digits");

export const roleSchema = z.enum(ROLES);
export const paymentModeSchema = z.enum(PAYMENT_MODES);

export const partyInputSchema = z.object({
  name: requiredText("Name"),
  phone: optionalPhone,
  email: optionalEmail,
  address: optionalText(500),
  gstin: optionalGstin,
});
export type PartyFormInput = z.input<typeof partyInputSchema>;
export type PartyInput = z.output<typeof partyInputSchema>;

export const productInputSchema = (currency: Currency = DEFAULT_CURRENCY) =>
  z.object({
    name: requiredText("Product name"),
    sku: requiredText("SKU", 64).regex(
      /^[A-Za-z0-9._/-]+$/,
      "SKU may use letters, digits and . _ / -",
    ),
    barcode: trimmed
      .transform((v) => (v === "" ? null : v))
      .pipe(
        z
          .string()
          .regex(/^[A-Za-z0-9-]{4,48}$/, "Barcode must be 4–48 letters or digits")
          .nullable(),
      ),
    categoryId: z.string().nullable(),
    brandId: z.string().nullable(),
    unitId: z.string().nullable(),
    taxRateId: z.string().nullable(),
    salePrice: moneyInput(currency),
    costPrice: moneyInput(currency),
    reorderLevel: qtyInput({ allowZero: true }),
    description: optionalText(2000),
  });
export type ProductFormInput = z.input<ReturnType<typeof productInputSchema>>;
export type ProductInput = z.output<ReturnType<typeof productInputSchema>>;
