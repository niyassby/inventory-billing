import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import type { Transport } from "./client";
import { createApiClient, createV2Client, tauriTransport } from "./client";
import { ApiError, toApiError } from "./errors";

describe("toApiError", () => {
  it.each([
    [
      "UNAUTHORIZED: Invalid or expired session token",
      "UNAUTHORIZED",
      "Invalid or expired session token",
    ],
    ["Session invalid", "UNAUTHORIZED", "Session invalid"],
    ["Login failed: Invalid credentials", "UNAUTHORIZED", "Invalid credentials"],
    [
      "FORBIDDEN: Role 'staff' does not have permission for this action",
      "FORBIDDEN",
      "Role 'staff' does not have permission for this action",
    ],
    [
      "UNIQUE constraint failed: products.sku",
      "CONFLICT",
      expect.stringContaining("duplicate SKU"),
    ],
    ["Query returned no rows", "NOT_FOUND", expect.stringContaining("not found")],
    ["disk I/O error", "INTERNAL", "disk I/O error"],
  ])("maps %j", (raw, code, message) => {
    const error = toApiError(raw);
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe(code);
    expect(error.message).toEqual(message);
  });

  it("keeps the raw legacy message in details", () => {
    expect(toApiError("disk I/O error").details).toEqual({ raw: "disk I/O error" });
  });

  it("accepts structured errors from the future backend", () => {
    const error = toApiError({
      code: "INSUFFICIENT_STOCK",
      message: "Only 2 left",
      details: { left: 2 },
    });
    expect(error.code).toBe("INSUFFICIENT_STOCK");
    expect(error.message).toBe("Only 2 left");
    expect(error.details).toEqual({ left: 2 });
    expect(toApiError({ code: "VALIDATION" }).message).toBe("VALIDATION");
  });

  it("handles Error objects, ApiErrors and unknown values", () => {
    const existing = new ApiError("FORBIDDEN", "no");
    expect(toApiError(existing)).toBe(existing);
    expect(toApiError(new Error("FORBIDDEN: nope")).code).toBe("FORBIDDEN");
    expect(toApiError({ code: "NOT_A_CODE" }).code).toBe("INTERNAL");
    expect(toApiError(42).code).toBe("INTERNAL");
    expect(toApiError("").message).toBe("Something went wrong. Please try again.");
  });
});

describe("createApiClient", () => {
  it("passes the command and arguments to the transport", async () => {
    const call = vi.fn().mockResolvedValue([{ id: "1" }]);
    const transport: Transport = { call };
    const client = createApiClient(transport);
    const result = await client.call("list_units_cmd", { token: "t" });
    expect(call).toHaveBeenCalledWith("list_units_cmd", { token: "t" });
    expect(result).toEqual([{ id: "1" }]);
  });

  it("normalises transport failures", async () => {
    const transport: Transport = {
      call: vi.fn().mockRejectedValue("FORBIDDEN: Role 'staff' does not have permission"),
    };
    await expect(
      createApiClient(transport).call("list_users_cmd", { token: "t" }),
    ).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
  });

  it("uses Tauri IPC by default", async () => {
    vi.mocked(invoke).mockResolvedValue("ok");
    await expect(tauriTransport.call("get_profile", { token: "t" })).resolves.toBe("ok");
    expect(invoke).toHaveBeenCalledWith("get_profile", { token: "t" });
  });
});

describe("createV2Client", () => {
  it("sends token, input and approval in one envelope", async () => {
    const call = vi.fn().mockResolvedValue({ items: [], total: 0, page: 1, pageSize: 50 });
    const client = createV2Client({ call }, () => "tok");
    await client.call("products_list", { page: 1 });
    expect(call).toHaveBeenCalledWith("products_list", {
      token: "tok",
      input: { page: 1 },
      approval: null,
    });
  });

  it("sends an empty token when signed out and passes approvals through", async () => {
    const call = vi.fn().mockResolvedValue(null);
    const client = createV2Client({ call }, () => null);
    const approval = { email: "owner@shop.in", pin: "1234" };
    await client.call("auth_logout", {}, approval);
    expect(call).toHaveBeenCalledWith("auth_logout", { token: "", input: {}, approval });
  });

  it("turns structured backend errors into ApiErrors", async () => {
    const call = vi
      .fn()
      .mockRejectedValue({ code: "APPROVAL_REQUIRED", message: "Needs a manager" });
    const client = createV2Client({ call }, () => "tok");
    await expect(client.call("auth_me", {})).rejects.toMatchObject({
      name: "ApiError",
      code: "APPROVAL_REQUIRED",
    });
  });
});
