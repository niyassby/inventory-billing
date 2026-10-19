import { describe, expect, it } from "vitest";
import type { Api } from "@inventory/shared";
import { describeConnection } from "./connection";

const client: Api.ClientStatus = {
  link: "connected",
  hubUrl: "https://192.168.1.20:47900",
  businessName: "Ravi Stores",
  pending: 0,
  rejected: 0,
  offlineNumbersLeft: 50,
  catalogueAt: null,
  lastContactAt: null,
  clockSkewSeconds: null,
  lastError: null,
};

function status(overrides: Partial<Api.DeviceStatus>): Api.DeviceStatus {
  return {
    mode: "client",
    runningMode: "client",
    deviceName: "Counter 1",
    hubLink: null,
    client,
    hub: null,
    restartRequired: false,
    ...overrides,
  };
}

describe("describeConnection", () => {
  it("shows standalone and restart states", () => {
    expect(
      describeConnection(status({ mode: "standalone", runningMode: "standalone" })).label,
    ).toBe("Standalone");
    expect(describeConnection(status({ mode: "hub", restartRequired: true })).tone).toBe("warning");
  });

  it("shows the hub server", () => {
    const running = describeConnection(
      status({
        runningMode: "hub",
        hub: {
          running: true,
          port: 47900,
          fingerprint: "ab",
          fingerprintShort: "AB12-CD34",
          urls: ["https://192.168.1.20:47900"],
          error: null,
        },
      }),
    );
    expect(running).toMatchObject({ tone: "ok", label: "Hub" });
    expect(running.detail).toContain("AB12-CD34");
    const failed = describeConnection(
      status({
        runningMode: "hub",
        hub: {
          running: false,
          port: 47900,
          fingerprint: "",
          fingerprintShort: "",
          urls: [],
          error: "port in use",
        },
      }),
    );
    expect(failed).toMatchObject({ tone: "error", detail: "port in use" });
    expect(describeConnection(status({ runningMode: "hub", hub: null })).tone).toBe("error");
  });

  it("follows the counter's link to the hub", () => {
    expect(describeConnection(status({})).label).toBe("Hub connected");
    expect(describeConnection(status({ client: { ...client, rejected: 1 } })).tone).toBe("warning");
    expect(
      describeConnection(status({ client: { ...client, link: "syncing", pending: 3 } })),
    ).toMatchObject({
      tone: "busy",
      label: "Syncing (3)",
    });
    expect(describeConnection(status({ client: { ...client, link: "degraded" } })).tone).toBe(
      "warning",
    );
    const offline = describeConnection(
      status({
        client: { ...client, link: "hub_offline", pending: 1, offlineNumbersLeft: 49, rejected: 2 },
      }),
    );
    expect(offline).toMatchObject({ tone: "error", label: "Hub offline (1)" });
    expect(offline.detail).toContain("49 invoice numbers left");
    expect(offline.detail).toContain("1 sale waiting");
    expect(offline.detail).toContain("2 offline sales need");
    expect(describeConnection(status({ client: { ...client, link: "hub_offline" } })).label).toBe(
      "Hub offline",
    );
    expect(describeConnection(status({ client: null })).label).toBe("Not paired");
  });
});
