import { useState } from "react";
import type { ReactNode } from "react";
import { APP_NAME } from "@inventory/shared";
import { ShieldCheck, Store, Users, WifiOff } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ConnectionBadge } from "@/components/connection-badge";
import { call } from "@/api/v2";
import { errorText } from "@/lib/queries";
import { useSession } from "@/stores/session";

const POINTS = [
  [WifiOff, "Works offline", "Keep selling when the network is down; everything syncs later."],
  [
    Users,
    "Many counters, one hub",
    "Connect every till in the shop to one PC over your own network.",
  ],
  [
    ShieldCheck,
    "Careful with money",
    "Every bill and payment is kept, signed and never silently edited.",
  ],
] as const;

/** Sign-in, first-run setup and the forced password change share this frame. */
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
    <div className="grid min-h-screen bg-background font-sans lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]">
      <aside className="relative hidden flex-col justify-between overflow-hidden bg-primary p-10 text-primary-foreground lg:flex">
        <div
          aria-hidden
          className="pointer-events-none absolute -top-24 -right-24 size-96 rounded-full bg-primary-foreground/10 blur-3xl"
        />
        <div className="relative flex items-center gap-3">
          <span className="flex size-10 items-center justify-center rounded-lg bg-primary-foreground/15">
            <Store className="size-5" aria-hidden />
          </span>
          <span className="text-lg font-semibold tracking-tight">{APP_NAME}</span>
        </div>
        <div className="relative flex flex-col gap-8">
          <h2 className="max-w-sm text-3xl leading-tight font-semibold tracking-tight">
            Stock, billing and books for your shop.
          </h2>
          <ul className="flex max-w-sm flex-col gap-5">
            {POINTS.map(([Icon, heading, text]) => (
              <li key={heading} className="flex gap-3">
                <Icon className="mt-0.5 size-5 shrink-0 opacity-90" aria-hidden />
                <div>
                  <p className="font-medium">{heading}</p>
                  <p className="text-sm text-primary-foreground/90">{text}</p>
                </div>
              </li>
            ))}
          </ul>
        </div>
        <p className="relative text-xs text-primary-foreground/85">
          Your data stays on your own computers.
        </p>
      </aside>
      <main className="flex items-center justify-center p-6">
        <div className="w-full max-w-md animate-fade-in">
          <div className="mb-6 flex items-start justify-between gap-3">
            <div>
              <p className="text-caption font-medium tracking-wide uppercase lg:hidden">
                {APP_NAME}
              </p>
              <h1 className="font-heading text-page-title">{title}</h1>
              {subtitle && <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>}
            </div>
            <ConnectionBadge />
          </div>
          <div className="rounded-xl border border-border bg-card p-6 shadow-sm">{children}</div>
        </div>
      </main>
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
