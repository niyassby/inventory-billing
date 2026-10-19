import type { Api } from "@inventory/shared";

export type ConnectionTone = "ok" | "busy" | "warning" | "error" | "neutral";

export interface ConnectionView {
  tone: ConnectionTone;
  /** Short text for the top bar ("Hub connected", "Syncing (3)"). */
  label: string;
  /** Longer explanation for the tooltip. */
  detail: string;
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

/**
 * What the top bar shows about this PC's connection (Master Prompt §11: online / hub-connected /
 * offline / syncing, visible at all times).
 */
export function describeConnection(status: Api.DeviceStatus): ConnectionView {
  if (status.restartRequired) {
    return {
      tone: "warning",
      label: "Restart needed",
      detail: "This PC's role was changed. Restart the app to apply it.",
    };
  }
  if (status.runningMode === "standalone") {
    return { tone: "neutral", label: "Standalone", detail: "Working on this PC only." };
  }
  if (status.runningMode === "hub") {
    const hub = status.hub;
    if (!hub?.running) {
      return {
        tone: "error",
        label: "Hub not serving",
        detail: hub?.error ?? "Counters can't connect until the hub server starts.",
      };
    }
    return {
      tone: "ok",
      label: "Hub",
      detail: `Counters connect to ${hub.urls[0] ?? `port ${hub.port}`} (code ${hub.fingerprintShort}).`,
    };
  }
  const client = status.client;
  if (!client) {
    return { tone: "error", label: "Not paired", detail: "Pair this counter with the hub." };
  }
  const rejected =
    client.rejected > 0
      ? ` ${plural(client.rejected, "offline sale")} ${client.rejected === 1 ? "needs" : "need"} a manager's review.`
      : "";
  switch (client.link) {
    case "connected":
      return {
        tone: client.rejected > 0 ? "warning" : "ok",
        label: "Hub connected",
        detail: `Connected to ${client.businessName}.${rejected}`,
      };
    case "syncing":
      return {
        tone: "busy",
        label: `Syncing (${client.pending})`,
        detail: `Sending ${plural(client.pending, "offline sale")} to the hub.${rejected}`,
      };
    case "degraded":
      return {
        tone: "warning",
        label: "Hub slow",
        detail: "The hub didn't answer just now; still trying.",
      };
    case "hub_offline":
      return {
        tone: "error",
        label: client.pending > 0 ? `Hub offline (${client.pending})` : "Hub offline",
        detail:
          `Paid sales still work: ${plural(client.offlineNumbersLeft, "invoice number")} left. ` +
          `${plural(client.pending, "sale")} waiting to sync.${rejected}`,
      };
  }
}
