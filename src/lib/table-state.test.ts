import { describe, expect, it } from "vitest";
import { nextSort, readTableState, writeTableState } from "./table-state";
import type { TableDefaults } from "./table-state";

const defaults: TableDefaults = {
  size: 50,
  sort: "date",
  descending: true,
  filters: { q: "", status: "" },
};

describe("table state in the URL", () => {
  it("falls back to the defaults for an empty or invalid query", () => {
    expect(readTableState(new URLSearchParams(""), defaults)).toEqual({
      page: 1,
      size: 50,
      sort: "date",
      descending: true,
      filters: { q: "", status: "" },
    });
    const bad = readTableState(new URLSearchParams("page=-3&size=7&dir=up"), defaults);
    expect(bad.page).toBe(1);
    expect(bad.size).toBe(50);
  });

  it("reads page, size, sort and filters", () => {
    const s = readTableState(
      new URLSearchParams("page=3&size=100&sort=total&dir=asc&q=rice&status=paid"),
      defaults,
    );
    expect(s).toEqual({
      page: 3,
      size: 100,
      sort: "total",
      descending: false,
      filters: { q: "rice", status: "paid" },
    });
  });

  it("writes only what differs from the defaults, and reads it back", () => {
    expect(
      writeTableState(
        { page: 1, size: 50, sort: "date", descending: true, filters: { q: "", status: "" } },
        defaults,
      ).toString(),
    ).toBe("");
    const state = {
      page: 2,
      size: 25,
      sort: "customer",
      descending: false,
      filters: { q: "ravi", status: "" },
    };
    const text = writeTableState(state, defaults).toString();
    expect(text).toBe("page=2&size=25&sort=customer&dir=asc&q=ravi");
    expect(readTableState(new URLSearchParams(text), defaults)).toEqual(state);
  });

  it("cycles a column: ascending, descending, then the default order", () => {
    const a = nextSort({ sort: "date", descending: true }, "total", defaults);
    expect(a).toEqual({ sort: "total", descending: false });
    const b = nextSort(a, "total", defaults);
    expect(b).toEqual({ sort: "total", descending: true });
    expect(nextSort(b, "total", defaults)).toEqual({ sort: "date", descending: true });
  });
});
