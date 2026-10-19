import type { Transport } from "./client";
import { tauriTransport } from "./client";

/** True inside the Tauri window (IPC available). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** A failure from the HTTP bridge, carrying the backend's `{ code, message, details }`. */
class TransportError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly details?: unknown,
  ) {
    super(message);
  }
}

function asTransportError(body: unknown, status: number): TransportError {
  if (
    typeof body === "object" &&
    body !== null &&
    "code" in body &&
    typeof body.code === "string"
  ) {
    const b = body as { code: string; message?: unknown; details?: unknown };
    return new TransportError(
      b.code,
      typeof b.message === "string" ? b.message : b.code,
      b.details,
    );
  }
  return new TransportError("INTERNAL", `HTTP ${status}`);
}

/**
 * Development and end-to-end tests in a plain browser: the same commands over HTTP to the dev
 * bridge (`cargo run --bin dev-server`). Errors keep the backend's `{ code, message, details }`.
 */
export function httpTransport(baseUrl: string, fetcher: typeof fetch = fetch): Transport {
  const base = baseUrl.replace(/\/+$/, "");
  return {
    async call(command, args) {
      let response: Response;
      try {
        response = await fetcher(`${base}/v1/call/${encodeURIComponent(command)}`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(args),
        });
      } catch {
        throw new TransportError(
          "HUB_OFFLINE",
          `The development server at ${base} is not running (npm run dev:bridge).`,
        );
      }
      const body: unknown = await response.json().catch(() => null);
      if (!response.ok) throw asTransportError(body, response.status);
      return body;
    },
  };
}

const DEV_BRIDGE =
  (import.meta.env.VITE_DEV_BRIDGE as string | undefined) ?? "http://127.0.0.1:1430";

/** IPC in the app; the dev bridge when the screens run in an ordinary browser. */
export function defaultTransport(): Transport {
  return isTauri() ? tauriTransport : httpTransport(DEV_BRIDGE);
}
