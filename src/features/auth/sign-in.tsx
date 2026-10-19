import { useState } from "react";
import type { ReactNode } from "react";
import { APP_NAME } from "@inventory/shared";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ConnectionBadge } from "@/components/connection-badge";
import { call } from "@/api/v2";
import { errorText } from "@/lib/queries";
import { useSession } from "@/stores/session";

export function AuthFrame({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-muted/30 p-4 font-sans">
      <div className="w-full max-w-md rounded-3xl border border-border/60 bg-background p-8 shadow-sm">
        <div className="mb-6 flex items-start justify-between gap-3">
          <div>
            <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
              {APP_NAME}
            </p>
            <h1 className="mt-1 font-heading text-2xl font-semibold">{title}</h1>
            {subtitle && <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>}
          </div>
          <ConnectionBadge />
        </div>
        {children}
      </div>
    </div>
  );
}

export function SignIn() {
  const notice = useSession((s) => s.notice);
  const signIn = useSession((s) => s.signIn);
  const [mode, setMode] = useState<"password" | "pin">("password");
  const [email, setEmail] = useState(() => localStorage.getItem("v2.lastEmail") ?? "");
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const session =
        mode === "password"
          ? await call("auth_login", { email: email.trim(), password: secret })
          : await call("auth_pin_login", { email: email.trim(), pin: secret });
      try {
        localStorage.setItem("v2.lastEmail", email.trim());
      } catch {
        // Only a convenience.
      }
      signIn(session);
    } catch (e) {
      setError(errorText(e));
      setSecret("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthFrame title="Sign in" subtitle={notice ?? "Welcome back."}>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Field>
          <FieldLabel htmlFor="email">E-mail</FieldLabel>
          <Input
            id="email"
            type="email"
            autoComplete="username"
            autoFocus={!email}
            required
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <Field data-invalid={Boolean(error) || undefined}>
          <FieldLabel htmlFor="secret">{mode === "password" ? "Password" : "PIN"}</FieldLabel>
          <Input
            id="secret"
            type="password"
            inputMode={mode === "pin" ? "numeric" : undefined}
            autoComplete={mode === "password" ? "current-password" : "off"}
            autoFocus={Boolean(email)}
            required
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
          {error && <FieldError>{error}</FieldError>}
        </Field>
        <Button type="submit" size="lg" disabled={busy || !email.trim() || !secret}>
          {busy ? "Signing in…" : "Sign in"}
        </Button>
        <button
          type="button"
          className="text-sm text-muted-foreground underline underline-offset-2"
          onClick={() => {
            setMode((m) => (m === "password" ? "pin" : "password"));
            setSecret("");
            setError(null);
          }}
        >
          {mode === "password" ? "Quick sign-in with PIN" : "Sign in with password"}
        </button>
      </form>
    </AuthFrame>
  );
}

export function ChangePassword() {
  const user = useSession((s) => s.user);
  const passwordChanged = useSession((s) => s.passwordChanged);
  const signOut = useSession((s) => s.signOut);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mismatch = again.length > 0 && next !== again;

  return (
    <AuthFrame
      title="Choose a new password"
      subtitle={`${user?.name ?? "You"}, please replace the temporary password before continuing.`}
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          setBusy(true);
          setError(null);
          call("auth_change_password", { currentPassword: current, newPassword: next })
            .then(passwordChanged)
            .catch((err: unknown) => setError(errorText(err)))
            .finally(() => setBusy(false));
        }}
      >
        <Field>
          <FieldLabel htmlFor="current">Current (temporary) password</FieldLabel>
          <Input
            id="current"
            type="password"
            autoComplete="current-password"
            required
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="next">New password</FieldLabel>
          <Input
            id="next"
            type="password"
            autoComplete="new-password"
            required
            value={next}
            onChange={(e) => setNext(e.target.value)}
          />
        </Field>
        <Field data-invalid={mismatch || Boolean(error) || undefined}>
          <FieldLabel htmlFor="again">New password again</FieldLabel>
          <Input
            id="again"
            type="password"
            autoComplete="new-password"
            required
            value={again}
            onChange={(e) => setAgain(e.target.value)}
          />
          {mismatch && <FieldError>The passwords don't match.</FieldError>}
          {error && <FieldError>{error}</FieldError>}
        </Field>
        <Button type="submit" size="lg" disabled={busy || !current || !next || mismatch}>
          Save and continue
        </Button>
        <Button type="button" variant="ghost" onClick={() => void signOut()}>
          Sign out
        </Button>
      </form>
    </AuthFrame>
  );
}
