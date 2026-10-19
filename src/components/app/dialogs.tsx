import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { create } from "zustand";
import { ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useApproval } from "@/stores/approval";

/**
 * A manager approves on the cashier's screen with their e-mail and PIN (or password). The
 * request is then sent again with the approval; nothing is stored as "pending".
 */
export function ApprovalDialog() {
  const pending = useApproval((s) => s.pending);
  const answer = useApproval((s) => s.answer);
  const [email, setEmail] = useState("");
  const [secret, setSecret] = useState("");
  const [usePassword, setUsePassword] = useState(false);
  useEffect(() => {
    if (pending) setSecret("");
  }, [pending]);
  const submit = () => {
    if (!email.trim() || !secret) return;
    answer(
      usePassword
        ? { email: email.trim(), password: secret }
        : { email: email.trim(), pin: secret },
    );
  };
  return (
    <Dialog open={pending !== null} onOpenChange={(open) => !open && answer(null)}>
      <DialogContent>
        <form
          className="flex flex-col gap-5"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldCheck className="size-5 text-primary" aria-hidden />
              Manager approval
            </DialogTitle>
            <DialogDescription>
              A manager must approve this: <strong>{pending?.reason}</strong>.
            </DialogDescription>
          </DialogHeader>
          <Field>
            <FieldLabel htmlFor="approver-email">Manager e-mail</FieldLabel>
            <Input
              id="approver-email"
              type="email"
              autoComplete="off"
              autoFocus
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </Field>
          <Field data-invalid={Boolean(pending?.error) || undefined}>
            <FieldLabel htmlFor="approver-secret">{usePassword ? "Password" : "PIN"}</FieldLabel>
            <Input
              id="approver-secret"
              type="password"
              inputMode={usePassword ? undefined : "numeric"}
              autoComplete="off"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
            {pending?.error && <FieldError>{pending.error}</FieldError>}
            <button
              type="button"
              className="w-fit text-xs text-muted-foreground underline underline-offset-2"
              onClick={() => setUsePassword((v) => !v)}
            >
              {usePassword ? "Use PIN instead" : "Use password instead"}
            </button>
          </Field>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => answer(null)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!email.trim() || !secret}>
              Approve
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------------------------

interface ConfirmRequest {
  title: string;
  description?: ReactNode;
  confirmLabel?: string;
  destructive?: boolean;
  /** Ask for a reason (voids, write-offs); the promise resolves with it. */
  reason?: { label: string; required?: boolean };
}

interface ConfirmState {
  request: (ConfirmRequest & { resolve: (value: string | false) => void }) | null;
  ask: (request: ConfirmRequest) => Promise<string | false>;
  close: (value: string | false) => void;
}

const useConfirmStore = create<ConfirmState>()((set, get) => ({
  request: null,
  ask(request) {
    get().request?.resolve(false);
    return new Promise((resolve) => set({ request: { ...request, resolve } }));
  },
  close(value) {
    const request = get().request;
    set({ request: null });
    request?.resolve(value);
  },
}));

/** `await confirm({...})` → the reason (or "" without one), or false when cancelled. */
export function confirm(request: ConfirmRequest): Promise<string | false> {
  return useConfirmStore.getState().ask(request);
}

export function ConfirmDialog() {
  const request = useConfirmStore((s) => s.request);
  const close = useConfirmStore((s) => s.close);
  const [reason, setReason] = useState("");
  useEffect(() => setReason(""), [request]);
  const needsReason = request?.reason?.required ?? false;
  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && close(false)}>
      <DialogContent>
        <form
          className="flex flex-col gap-5"
          onSubmit={(e) => {
            e.preventDefault();
            if (needsReason && !reason.trim()) return;
            close(reason.trim());
          }}
        >
          <DialogHeader>
            <DialogTitle>{request?.title}</DialogTitle>
            {request?.description && <DialogDescription>{request.description}</DialogDescription>}
          </DialogHeader>
          {request?.reason && (
            <Field>
              <FieldLabel htmlFor="confirm-reason">{request.reason.label}</FieldLabel>
              <Textarea
                id="confirm-reason"
                autoFocus
                value={reason}
                onChange={(e) => setReason(e.target.value)}
              />
            </Field>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => close(false)}>
              Cancel
            </Button>
            <Button
              type="submit"
              variant={request?.destructive ? "destructive" : "default"}
              disabled={needsReason && !reason.trim()}
              autoFocus={!request?.reason}
            >
              {request?.confirmLabel ?? "Confirm"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
