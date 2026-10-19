import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import type { Api } from "@inventory/shared";
import { createV2Client, tauriTransport } from "@/api/client";

/** Local device commands need no session. */
const device = createV2Client(tauriTransport, () => null);

const KEY = ["device-status"] as const;

/**
 * This PC's role and connection, kept fresh by the backend's "hub-status" events (counters) and
 * a slow poll (hub and standalone).
 */
export function useConnection() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: KEY,
    queryFn: () => device.call("device_status", {}),
    refetchInterval: 15_000,
  });
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void listen<Api.ClientStatus>("hub-status", (event) => {
      queryClient.setQueryData<Api.DeviceStatus>(KEY, (old) =>
        old ? { ...old, client: event.payload } : old,
      );
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        // Outside Tauri (plain browser preview) there are no events.
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [queryClient]);
  return query;
}
