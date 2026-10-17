import { describe, expect, it } from "vitest";
import {
  DEFAULT_ROLE_PERMISSIONS,
  LEGACY_ROLE_MAP,
  PERMISSIONS,
  ROLES,
  isErrorCode,
  roleHasPermission,
} from "./constants";
import { gstinCheckChar, gstinStateCode, isValidGstin } from "./gstin";

describe("roles and permissions", () => {
  it("gives the owner every permission", () => {
    expect(DEFAULT_ROLE_PERMISSIONS.owner).toEqual(PERMISSIONS);
  });

  it("keeps user and backup management with the owner only", () => {
    for (const role of ROLES.filter((r) => r !== "owner")) {
      expect(roleHasPermission(role, "users.manage")).toBe(false);
      expect(roleHasPermission(role, "backup.manage")).toBe(false);
    }
  });

  it("lets cashiers sell but not void or override prices", () => {
    expect(roleHasPermission("cashier", "sales.create")).toBe(true);
    expect(roleHasPermission("cashier", "sales.void")).toBe(false);
    expect(roleHasPermission("cashier", "sales.price_override")).toBe(false);
  });

  it("only references known permissions", () => {
    for (const perms of Object.values(DEFAULT_ROLE_PERMISSIONS)) {
      for (const p of perms) expect(PERMISSIONS).toContain(p);
    }
  });

  it("maps every legacy role", () => {
    expect(LEGACY_ROLE_MAP).toEqual({
      admin: "owner",
      manager: "manager",
      accountant: "accountant",
      staff: "cashier",
    });
  });

  it("recognises error codes", () => {
    expect(isErrorCode("FORBIDDEN")).toBe(true);
    expect(isErrorCode("Forbidden")).toBe(false);
  });
});

describe("GSTIN", () => {
  it("accepts valid GSTINs (case-insensitive)", () => {
    expect(isValidGstin("27AAPFU0939F1ZV")).toBe(true);
    expect(isValidGstin(" 29aagcb7383j1z4 ")).toBe(true);
    expect(gstinStateCode("33AAACH7409R1Z8")).toBe("33");
  });

  it("rejects bad format or checksum", () => {
    expect(isValidGstin("27AAPFU0939F1ZW")).toBe(false);
    expect(isValidGstin("27AAPFU0939F1Z")).toBe(false);
    expect(gstinStateCode("nope")).toBeNull();
  });

  it("computes the check character", () => {
    expect(gstinCheckChar("27AAPFU0939F1Z")).toBe("V");
    expect(() => gstinCheckChar("27aapfu0939f1z")).toThrow(RangeError);
  });
});
