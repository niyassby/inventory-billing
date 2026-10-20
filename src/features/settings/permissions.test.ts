import { describe, expect, it } from "vitest";
import { permissionMatrix } from "./permissions";

const p = (code: string) => ({ code, description: `Can ${code}` });

describe("permissionMatrix", () => {
  it("groups codes into areas and orders the common actions first", () => {
    const m = permissionMatrix([
      p("sales.void"),
      p("sales.view"),
      p("products.manage"),
      p("products.view"),
      p("audit.view"),
      p("approvals.grant"),
    ]);
    expect(m.actions).toEqual(["view", "manage", "void", "grant"]);
    expect(m.rows.map((r) => r.title)).toEqual(["Approvals", "Audit", "Products", "Sales"]);
    const sales = m.rows.find((r) => r.group === "sales");
    expect(sales?.cells["view"]?.code).toBe("sales.view");
    expect(sales?.cells["manage"]).toBeUndefined();
  });

  it("copes with codes that have no action part", () => {
    const m = permissionMatrix([p("backup")]);
    expect(m.actions).toEqual(["use"]);
    expect(m.rows[0]?.cells["use"]?.code).toBe("backup");
  });

  it("returns an empty grid for no permissions", () => {
    expect(permissionMatrix([])).toEqual({ actions: [], rows: [] });
  });
});
