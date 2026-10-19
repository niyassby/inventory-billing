import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiError } from "@/api/errors";

/** Only network-ish failures are worth a retry; a refused request stays refused. */
function shouldRetry(failures: number, error: unknown): boolean {
  if (error instanceof ApiError && error.code !== "INTERNAL" && error.code !== "HUB_OFFLINE") {
    return false;
  }
  return failures < 1;
}

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Several counters change the same data: keep lists fresh.
      staleTime: 1000 * 30,
      retry: shouldRetry,
      refetchOnWindowFocus: false,
    },
    mutations: { retry: false },
  },
});

export default function QueryProvider({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
