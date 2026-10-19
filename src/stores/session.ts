import { create } from "zustand";
import type { Api } from "@inventory/shared";
import { call, setAuthErrorHandler, setTokenSource } from "@/api/v2";

const STORAGE_KEY = "v2.session";

/**
 * The session token lives in sessionStorage: it survives a window reload but not closing the
 * app (audit S-3: no long-lived token in localStorage). The backend enforces expiry and idle
 * timeout anyway.
 */
function readToken(): string | null {
  try {
    return sessionStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function writeToken(token: string | null): void {
  try {
    if (token) sessionStorage.setItem(STORAGE_KEY, token);
    else sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    // Storage blocked: the session simply won't survive a reload.
  }
}

export type SessionStatus = "checking" | "signedOut" | "signedIn";

interface SessionState {
  status: SessionStatus;
  token: string | null;
  user: Api.SessionUser | null;
  mustChangePassword: boolean;
  /** Why the last session ended, shown on the sign-in screen. */
  notice: string | null;
  signIn: (session: Api.AuthSession) => void;
  signOut: () => Promise<void>;
  /** The backend rejected the token (expired, idle, revoked). */
  expire: (message?: string) => void;
  passwordChanged: () => void;
  requirePasswordChange: () => void;
  /** Checks a token kept from before a reload. */
  restore: () => Promise<void>;
}

export const useSession = create<SessionState>()((set, get) => ({
  status: readToken() ? "checking" : "signedOut",
  token: readToken(),
  user: null,
  mustChangePassword: false,
  notice: null,

  signIn(session) {
    writeToken(session.token);
    set({
      status: "signedIn",
      token: session.token,
      user: session.user,
      mustChangePassword: session.mustChangePassword,
      notice: null,
    });
  },

  async signOut() {
    const { token } = get();
    writeToken(null);
    set({ status: "signedOut", token: null, user: null, mustChangePassword: false, notice: null });
    if (token) {
      // Best effort: the session is gone locally either way.
      await call("auth_logout", {}).catch(() => undefined);
    }
  },

  expire(message) {
    writeToken(null);
    set({
      status: "signedOut",
      token: null,
      user: null,
      mustChangePassword: false,
      notice: message ?? "Your session ended. Please sign in again.",
    });
  },

  passwordChanged() {
    set({ mustChangePassword: false });
  },

  requirePasswordChange() {
    set({ mustChangePassword: true });
  },

  async restore() {
    if (!get().token) {
      set({ status: "signedOut" });
      return;
    }
    try {
      const user = await call("auth_me", {});
      set({ status: "signedIn", user });
    } catch {
      // expire() already ran for UNAUTHORIZED; anything else (backend down) also signs out.
      if (get().status !== "signedOut") get().expire();
    }
  },
}));

setTokenSource(() => useSession.getState().token);
setAuthErrorHandler((error) => {
  const session = useSession.getState();
  if (error.code === "PASSWORD_CHANGE_REQUIRED") session.requirePasswordChange();
  else if (session.status === "signedIn") session.expire();
});

/** Permission check for showing or hiding actions. The backend decides; this only hides. */
export function useCan(permission: string): boolean {
  return useSession((s) => s.user?.permissions.includes(permission) ?? false);
}
