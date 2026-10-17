import type { ReactNode } from "react";
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useShallow } from "zustand/react/shallow";
import { useStore } from "@/stores/app-store";

export function FullScreenSpinner({ label }: { label: string }) {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-background font-sans text-foreground">
      <div className="flex flex-col items-center gap-4" role="status" aria-live="polite">
        <div className="h-12 w-12 animate-spin rounded-full border-4 border-primary border-t-transparent" />
        <p className="animate-pulse text-sm font-semibold text-muted-foreground">{label}</p>
      </div>
    </div>
  );
}

/**
 * Validates the stored session once, shows a spinner meanwhile, and redirects to /auth when there
 * is no valid session. Wraps both the main layout and the full-screen POS (which previously had no
 * guard at all).
 */
export function RequireAuth({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const { isAuthenticated, authLoading, checkAuth } = useStore(
    useShallow((s) => ({
      isAuthenticated: s.isAuthenticated,
      authLoading: s.authLoading,
      checkAuth: s.checkAuth,
    })),
  );

  useEffect(() => {
    if (!isAuthenticated) void checkAuth();
    // Only on mount: login() sets isAuthenticated itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!authLoading && !isAuthenticated) {
      void navigate("/auth", { replace: true });
    }
  }, [authLoading, isAuthenticated, navigate]);

  if (authLoading) return <FullScreenSpinner label="Connecting to database…" />;
  if (!isAuthenticated) return null;
  return <>{children}</>;
}
