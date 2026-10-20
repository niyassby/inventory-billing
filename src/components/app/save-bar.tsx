import type { MouseEventHandler } from "react";
import { Button } from "@/components/ui/button";

/**
 * Appears at the bottom of a long form as soon as something has changed, and stays in view while
 * you scroll: "Unsaved changes", Discard and Save. Nothing is shown while the form is clean.
 */
export function SaveBar({
  dirty,
  saving,
  onDiscard,
  onSave,
  saveLabel = "Save changes",
}: {
  dirty: boolean;
  saving: boolean;
  onDiscard: () => void;
  /** Omit inside a <form>: the button then submits the form. */
  onSave?: MouseEventHandler<HTMLButtonElement>;
  saveLabel?: string;
}) {
  if (!dirty) return null;
  return (
    <div
      role="region"
      aria-label="Unsaved changes"
      className="sticky bottom-4 z-30 mx-auto flex w-full max-w-xl animate-fade-in items-center justify-between gap-3 rounded-xl border border-border bg-raised px-4 py-2.5 shadow-lg"
    >
      <span className="text-sm text-muted-foreground">You have unsaved changes</span>
      <span className="flex items-center gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onDiscard} disabled={saving}>
          Discard
        </Button>
        <Button
          type={onSave ? "button" : "submit"}
          size="sm"
          disabled={saving}
          {...(onSave ? { onClick: onSave } : {})}
        >
          {saving ? "Saving…" : saveLabel}
        </Button>
      </span>
    </div>
  );
}
