import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { createV2Client } from "./client";
import { ApiError } from "./errors";
import { httpTransport } from "./transport";
import { createCaller, requestId } from "./v2";

function setup(responses: unknown[]) {
  const sent: unknown[] = [];
  const transport = {
    call: vi.fn((_command: string, args: Record<string, unknown>) => {
      sent.push(args);
      const next = responses.shift();
      return next instanceof Error || (typeof next === "object" && next && "code" in next)
        ? // Tauri rejects with the backend's plain { code, message } object, not an Error.
          // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
          Promise.reject(next)
        : Promise.resolve(next);
    }),
  };
  const requestApproval = vi.fn();
  const onAuthError = vi.fn();
  const call = createCaller({
    client: createV2Client(transport, () => "tok"),
    requestApproval,
    onAuthError,
  });
  return { call, sent, requestApproval, onAuthError };
}

const approvalNeeded = {
  code: "APPROVAL_REQUIRED",
  message: "Manager approval needed: give a discount",
  details: { permission: "sales.discount", reason: "give a discount" },
};
const manager = { email: "manager@shop.in", pin: "1234" };

describe("createCaller", () => {
  it("passes results through", async () => {
    const { call } = setup([{ ok: true }]);
    await expect(call("auth_me", {})).resolves.toEqual({ ok: true });
  });

  it("asks for a manager and retries the same request with the approval", async () => {
    const { call, sent, requestApproval } = setup([approvalNeeded, { id: "inv" }]);
    requestApproval.mockResolvedValue(manager);
    await expect(call("auth_me", {})).resolves.toEqual({ id: "inv" });
    expect(requestApproval).toHaveBeenCalledWith({ reason: "give a discount" });
    expect(sent[1]).toEqual({ token: "tok", input: {}, approval: manager });
  });

  it("asks again after a wrong PIN, then gives up after three tries", async () => {
    const wrong = { code: "FORBIDDEN", message: "Approval credentials are incorrect." };
    const { call, requestApproval } = setup([approvalNeeded, wrong, wrong, wrong]);
    requestApproval.mockResolvedValue(manager);
    await expect(call("auth_me", {})).rejects.toMatchObject({ code: "FORBIDDEN" });
    expect(requestApproval).toHaveBeenCalledTimes(3);
    expect(requestApproval).toHaveBeenLastCalledWith({
      reason: "give a discount",
      error: "Approval credentials are incorrect.",
    });
  });

  it("stops when the approval is cancelled", async () => {
    const { call, requestApproval } = setup([approvalNeeded]);
    requestApproval.mockResolvedValue(null);
    await expect(call("auth_me", {})).rejects.toMatchObject({ code: "APPROVAL_REQUIRED" });
  });

  it("does not treat a plain FORBIDDEN as a wrong PIN", async () => {
    const { call, requestApproval } = setup([{ code: "FORBIDDEN", message: "No" }]);
    await expect(call("auth_me", {}, { approval: manager })).rejects.toBeInstanceOf(ApiError);
    expect(requestApproval).not.toHaveBeenCalled();
  });

  it("reports session problems", async () => {
    const { call, onAuthError } = setup([{ code: "UNAUTHORIZED", message: "Session expired" }]);
    await expect(call("auth_me", {})).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(onAuthError).toHaveBeenCalledWith(expect.objectContaining({ code: "UNAUTHORIZED" }));
  });

  it("makes request ids the backend accepts", () => {
    expect(requestId()).toMatch(/^[0-9a-f-]{36}$/);
    expect(requestId()).not.toBe(requestId());
  });
});

describe("httpTransport", () => {
  it("posts the envelope and returns the body", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify({ a: 1 }), { status: 200 }));
    const transport = httpTransport("http://127.0.0.1:1430/", fetcher);
    await expect(transport.call("auth_me", { token: "t" })).resolves.toEqual({ a: 1 });
    expect(fetcher).toHaveBeenCalledWith(
      "http://127.0.0.1:1430/v1/call/auth_me",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ token: "t" }) }),
    );
  });

  it("passes backend errors on and reports a missing server", async () => {
    const error = { code: "VALIDATION", message: "Bad" };
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(error), { status: 422 }))
      .mockResolvedValueOnce(new Response("oops", { status: 500 }))
      .mockRejectedValueOnce(new TypeError("Failed to fetch"));
    const transport = httpTransport("http://x", fetcher);
    await expect(transport.call("a", {})).rejects.toMatchObject(error);
    await expect(transport.call("a", {})).rejects.toMatchObject({ code: "INTERNAL" });
    await expect(transport.call("a", {})).rejects.toMatchObject({ code: "HUB_OFFLINE" });
  });
});
