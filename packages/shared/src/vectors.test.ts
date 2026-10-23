import { describe, expect, it } from "vitest";
import vectors from "../test-vectors/money.json";
import pricing from "../test-vectors/pricing.json";
import priceLists from "../test-vectors/price-lists.json";
import type { ListPriceQuery } from "./price-lists";
import { resolveListPrice } from "./price-lists";
import { fromLegacyFloat, money } from "./money";
import { allocateProportionally, divRound } from "./rounding";
import type { DocumentInput } from "./tax";
import { priceDocument } from "./tax";

// The Rust domain::money module runs the same file; both must agree (docs/ASSUMPTIONS.md A-28).
describe("shared money vectors", () => {
  it.each(vectors.divRound)("divRound($n / $d)", (v) => {
    const n = BigInt(v.n);
    const d = BigInt(v.d);
    expect(divRound(n, d, "half_up")).toBe(BigInt(v.half_up));
    expect(divRound(n, d, "half_even")).toBe(BigInt(v.half_even));
    expect(divRound(n, d, "down")).toBe(BigInt(v.down));
    expect(divRound(n, d, "up")).toBe(BigInt(v.up));
  });

  it.each(vectors.allocate)("allocate($total over $weights)", (v) => {
    expect(allocateProportionally(v.total, v.weights)).toEqual(v.expected);
  });

  it.each(vectors.legacyFloat)("fromLegacyFloat($major)", (v) => {
    const result = fromLegacyFloat(v.major, { code: "TEST", minorDigits: v.minorDigits });
    expect(result).toEqual({ amount: money(v.amount), lossy: v.lossy });
  });
});

describe("shared price-list vectors", () => {
  it.each(priceLists.cases)("$name", (c) => {
    expect(resolveListPrice(c.query as ListPriceQuery)).toEqual(c.expected);
  });
});

describe("shared pricing vectors", () => {
  it.each(pricing.cases)("$name", (c) => {
    expect(priceDocument(c.input as unknown as DocumentInput)).toEqual(c.expected);
  });

  it.each(pricing.errors)("rejects: $name", (c) => {
    expect(() => priceDocument(c.input as unknown as DocumentInput)).toThrow();
  });
});
