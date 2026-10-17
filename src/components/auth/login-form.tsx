import type { ComponentProps, SyntheticEvent } from "react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { loginSchema } from "@inventory/shared";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { useStore } from "@/stores/app-store";

export function LoginForm({ className, ...props }: ComponentProps<"form">) {
  // Known issue (audit S-1): the seeded demo account is prefilled until first-run onboarding
  // replaces it in Phase 3.
  const [formData, setFormData] = useState({
    email: "admin@example.com",
    password: "password123",
  });
  const [errorMsg, setErrorMsg] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const login = useStore((state) => state.login);
  const navigate = useNavigate();

  const onSubmit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    setErrorMsg("");
    const parsed = loginSchema.safeParse(formData);
    if (!parsed.success) {
      setErrorMsg(parsed.error.issues[0]?.message ?? "Check the form and try again.");
      return;
    }
    setSubmitting(true);
    const res = await login(parsed.data.email, parsed.data.password);
    setSubmitting(false);
    if (res.success) {
      void navigate("/dashboard");
    } else {
      setErrorMsg(res.error || "Invalid credentials. Please try again.");
    }
  };

  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className={cn("flex flex-col gap-6", className)}
      noValidate
      {...props}
    >
      <FieldGroup>
        <div className="flex flex-col items-center gap-1 text-center">
          <h1 className="text-2xl font-bold">Sign In</h1>
          <p className="text-sm text-balance text-muted-foreground">
            Enter your credentials to access your offline dashboard
          </p>
        </div>

        {errorMsg && (
          <div
            role="alert"
            className="rounded-md border border-destructive/25 bg-destructive/15 p-3 text-center text-sm text-destructive"
          >
            {errorMsg}
          </div>
        )}

        <Field>
          <FieldLabel htmlFor="email">Email Address</FieldLabel>
          <Input
            value={formData.email}
            onChange={(e) => {
              setFormData({ ...formData, email: e.target.value });
            }}
            id="email"
            type="email"
            autoComplete="username"
            placeholder="you@business.com"
            required
          />
        </Field>

        <Field>
          <FieldLabel htmlFor="password">Password</FieldLabel>
          <Input
            value={formData.password}
            onChange={(e) => {
              setFormData({ ...formData, password: e.target.value });
            }}
            id="password"
            type="password"
            autoComplete="current-password"
            placeholder="••••••••"
            required
          />
        </Field>

        <Field>
          <Button type="submit" className="w-full" disabled={submitting}>
            {submitting ? "Signing in…" : "Sign In"}
          </Button>
        </Field>

        <div className="mt-2 border-t border-border/40 pt-4 text-center text-xs text-muted-foreground">
          <p className="mb-1 font-semibold text-foreground">Demo Credentials:</p>
          <p>
            Email: <span className="font-mono text-foreground">admin@example.com</span>
          </p>
          <p>
            Password: <span className="font-mono text-foreground">password123</span>
          </p>
        </div>
      </FieldGroup>
    </form>
  );
}
