import { useEffect, useState } from "react";
import { RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { useSession } from "@/stores/session";
import { useShortcuts } from "./shortcut-store";
import {
  FIXED_KEYS,
  SHORTCUT_ACTIONS,
  bindingFromEvent,
  formatBinding,
  type ShortcutAction,
} from "./shortcuts";

/** A binding as key caps: "Ctrl+Delete" → [Ctrl] [Delete]. */
export function KeyCaps({ binding }: { binding: string }) {
  const parts = binding.endsWith("++")
    ? [...binding.slice(0, -2).split("+"), "+"]
    : binding.split("+");
  return (
    <span className="inline-flex items-center gap-1" aria-label={formatBinding(binding)}>
      {parts.map((part, i) => (
        <Kbd key={`${part}-${i}`}>{part}</Kbd>
      ))}
    </span>
  );
}

/**
 * Shows each till shortcut, lets the person press the keys they want instead, refuses keys that are
 * taken or reserved (and says why), and puts one or all back to the defaults.
 */
export function ShortcutsEditor() {
  const user = useSession((s) => s.user);
  const { bindings, set, reset, resetAll } = useShortcuts();
  const [capturing, setCapturing] = useState<ShortcutAction | null>(null);
  const [problem, setProblem] = useState<{ action: ShortcutAction; message: string } | null>(null);

  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      // While choosing keys, nothing else on the screen may react to them.
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturing(null);
        setProblem(null);
        return;
      }
      const binding = bindingFromEvent(e);
      if (!binding) return; // only a modifier so far
      const refused = set(capturing, binding);
      if (refused) setProblem({ action: capturing, message: refused.message });
      else {
        setCapturing(null);
        setProblem(null);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, set]);

  const changed = SHORTCUT_ACTIONS.some((a) => bindings[a.id] !== a.default);
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        These keys work on the till screen{user ? ` for ${user.name}` : ""}. Choose{" "}
        <strong className="font-medium text-foreground">Change</strong>, then press the keys you
        want. They are saved on this PC.
      </p>
      <ul className="divide-y divide-border/60 rounded-lg border border-border">
        {SHORTCUT_ACTIONS.map((a) => {
          const active = capturing === a.id;
          const isDefault = bindings[a.id] === a.default;
          return (
            <li key={a.id} className="flex flex-wrap items-center gap-3 px-3 py-2.5">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">{a.label}</p>
                {problem?.action === a.id && (
                  <p role="alert" className="mt-0.5 text-sm text-danger">
                    {problem.message}
                  </p>
                )}
              </div>
              <div className="flex min-w-44 items-center justify-end gap-2">
                {active ? (
                  <span className="animate-pulse text-sm text-primary" aria-live="polite">
                    Press the new keys… (Esc cancels)
                  </span>
                ) : (
                  <KeyCaps binding={bindings[a.id]} />
                )}
              </div>
              <div className="flex items-center gap-1">
                <Button
                  type="button"
                  variant={active ? "secondary" : "outline"}
                  size="sm"
                  onClick={() => {
                    setProblem(null);
                    setCapturing(active ? null : a.id);
                  }}
                >
                  {active ? "Cancel" : "Change"}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Put “${a.label}” back to ${formatBinding(a.default)}`}
                  title={isDefault ? "Already the default" : `Back to ${formatBinding(a.default)}`}
                  disabled={isDefault}
                  onClick={() => {
                    setProblem(null);
                    setCapturing(null);
                    reset(a.id);
                  }}
                >
                  <RotateCcw />
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-caption">
          Always the same:{" "}
          {FIXED_KEYS.map((k, i) => (
            <span key={k.keys}>
              {i > 0 && " · "}
              <span className="font-medium text-foreground">{k.keys}</span> {k.does.toLowerCase()}
            </span>
          ))}
          .
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!changed}
          onClick={() => {
            setProblem(null);
            setCapturing(null);
            resetAll();
          }}
        >
          Reset all to defaults
        </Button>
      </div>
    </div>
  );
}
