import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { labelSheetHtml, mergeItems, sheetGrid, totalLabels } from "./labels";

const item = (id: string, copies: number, batchNo: string | null = null): Api.LabelItem => ({
  variantId: id,
  name: `Item ${id}`,
  sku: id,
  barcode: null,
  price: 100,
  mrp: null,
  batchNo,
  expiryDate: null,
  copies,
});

describe("label sheets", () => {
  it("fits labels on A4", () => {
    expect(sheetGrid({ widthMm: 50, heightMm: 25 })).toEqual({ columns: 3, rows: 10 });
    expect(sheetGrid({ widthMm: 38, heightMm: 21 })).toEqual({ columns: 5, rows: 12 });
    expect(sheetGrid({ widthMm: 150, heightMm: 150 })).toEqual({ columns: 1, rows: 1 });
  });

  it("repeats each label for its copies", () => {
    const html = labelSheetHtml({ name: "Shelf <50>", widthMm: 50, heightMm: 25 }, [
      { svg: "<svg>a</svg>", copies: 2 },
      { svg: "<svg>b</svg>", copies: 0 },
      { svg: "<svg>c</svg>", copies: 1 },
    ]);
    expect(html.match(/<img /g)).toHaveLength(3);
    expect(html).toContain("grid-template-columns: repeat(3, 50mm)");
    expect(html).toContain("<title>Shelf 50</title>");
    expect(html).toContain(encodeURIComponent("<svg>a</svg>"));
  });

  it("merges the same product and batch, keeps other batches apart", () => {
    const list = mergeItems([item("v1", 2)], [item("v1", 3), item("v2", 1), item("v1", 1, "B1")]);
    expect(list.map((i) => [i.variantId, i.batchNo, i.copies])).toEqual([
      ["v1", null, 5],
      ["v2", null, 1],
      ["v1", "B1", 1],
    ]);
    expect(totalLabels(list)).toBe(7);
    expect(totalLabels([item("v1", -1)])).toBe(0);
  });
});
