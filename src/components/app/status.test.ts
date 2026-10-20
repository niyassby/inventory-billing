import { describe, expect, it } from "vitest";
import { initials, tintFor } from "@/components/ui/avatar";
import { STATUS_TONES, stockTone } from "./status";

describe("status tones", () => {
  it("keeps green for settled things and amber for what is still owed", () => {
    expect(STATUS_TONES["paid"]).toBe("success");
    expect(STATUS_TONES["partial"]).toBe("warning");
    expect(STATUS_TONES["unpaid"]).toBe("warning");
    expect(STATUS_TONES["voided"]).toBe("danger");
  });

  it("flags out of stock and low stock, and nothing else", () => {
    expect(stockTone(0, 5000)).toBe("out");
    expect(stockTone(-2000, 0)).toBe("out");
    expect(stockTone(3000, 5000)).toBe("low");
    expect(stockTone(5000, 5000)).toBe("low");
    expect(stockTone(5001, 5000)).toBe("ok");
    expect(stockTone(1000, 0)).toBe("ok"); // no reorder level set
  });
});

describe("avatar helpers", () => {
  it("takes two initials from a name or a single word", () => {
    expect(initials("Basmati Rice 5kg")).toBe("BR");
    expect(initials("steel-tumbler")).toBe("ST");
    expect(initials("Tea")).toBe("TE");
    expect(initials("")).toBe("?");
  });

  it("gives the same tint to the same name", () => {
    expect(tintFor("Notebook A5")).toBe(tintFor("Notebook A5"));
  });
});
