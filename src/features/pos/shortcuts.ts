/**
 * The till's keyboard shortcuts as data: which actions can be bound, their defaults, how a key
 * press is written down ("Ctrl+Delete", "F10"), and which bindings are allowed. No React here, so
 * the rules are unit tested.
 */

export type ShortcutAction =
  "search" | "customer" | "editLine" | "discount" | "hold" | "recall" | "pay" | "clear";

export interface ShortcutDef {
  id: ShortcutAction;
  /** In the settings list. */
  label: string;
  /** Under the bill ("Search", "Hold"). */
  hint: string;
  default: string;
}

export const SHORTCUT_ACTIONS: readonly ShortcutDef[] = [
  { id: "search", label: "Find an item", hint: "Search", default: "F1" },
  { id: "customer", label: "Choose the customer", hint: "Customer", default: "F3" },
  { id: "editLine", label: "Edit the selected line", hint: "Edit line", default: "F4" },
  { id: "discount", label: "Discount on the whole bill", hint: "Bill discount", default: "F7" },
  { id: "hold", label: "Hold the bill and start a new one", hint: "Hold", default: "F8" },
  { id: "recall", label: "Held bills", hint: "Recall", default: "F9" },
  { id: "pay", label: "Take payment", hint: "Pay", default: "F10" },
  { id: "clear", label: "Clear the bill", hint: "Clear", default: "Ctrl+Delete" },
];

export type Bindings = Record<ShortcutAction, string>;

export const DEFAULT_BINDINGS: Bindings = Object.fromEntries(
  SHORTCUT_ACTIONS.map((a) => [a.id, a.default]),
) as Bindings;

/** Keys the till always uses itself: they can't be given to an action. */
export const FIXED_KEYS: readonly { keys: string; does: string }[] = [
  { keys: "↑ ↓", does: "Choose a line" },
  { keys: "+ −", does: "Change the quantity" },
  { keys: "Delete", does: "Remove the selected line" },
  { keys: "Ctrl+Enter", does: "Take payment (always works)" },
  { keys: "Enter / Esc", does: "Complete the payment / go back" },
];

const MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;

/** Keys that belong to the app, the window or the browser, or that the till itself uses. */
const RESERVED = new Set([
  // The till's own
  "Enter",
  "Escape",
  "Tab",
  "Space",
  "Backspace",
  "Delete",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Ctrl+Enter",
  // The app's: open the till (F2), search (Ctrl+K), the sidebar (Ctrl+B)
  "F2",
  "Ctrl+K",
  "Ctrl+B",
  // The window and the web view
  "F5",
  "Ctrl+F5",
  "F11",
  "F12",
  "Ctrl+R",
  "Ctrl+Shift+R",
  "Ctrl+W",
  "Ctrl+T",
  "Ctrl+N",
  "Ctrl+Q",
  "Ctrl+P",
  "Ctrl+F",
  "Ctrl+Shift+I",
  "Alt+F4",
  "Alt+Left",
  "Alt+Right",
  "Alt+Tab",
  // Editing a text box
  "Ctrl+A",
  "Ctrl+C",
  "Ctrl+V",
  "Ctrl+X",
  "Ctrl+Z",
  "Ctrl+Y",
  "Ctrl+Backspace",
  "Ctrl+S",
  // Zoom
  "Ctrl++",
  "Ctrl+-",
  "Ctrl+=",
  "Ctrl+0",
]);

const NAMED_KEYS: Record<string, string> = {
  " ": "Space",
  Esc: "Escape",
  Del: "Delete",
  Up: "ArrowUp",
  Down: "ArrowDown",
  Left: "ArrowLeft",
  Right: "ArrowRight",
  Return: "Enter",
};

const CANONICAL_KEYS = [
  "Enter",
  "Escape",
  "Tab",
  "Space",
  "Backspace",
  "Delete",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
];

/** One key as written in a binding: letters upper case, named keys as the browser names them. */
function keyName(key: string): string {
  const named = NAMED_KEYS[key] ?? key;
  if (named.length === 1) return named.toUpperCase();
  const known = CANONICAL_KEYS.find((k) => k.toLowerCase() === named.toLowerCase());
  if (known) return known;
  return /^f\d{1,2}$/i.test(named) ? named.toUpperCase() : named;
}

/** "ctrl + delete" → "Ctrl+Delete" (modifiers in a fixed order); null if it is not a binding. */
export function normalizeBinding(text: string): string | null {
  const parts = text
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean);
  if (text.trim().endsWith("+") && parts.length > 0) parts.push("+");
  if (parts.length === 0) return null;
  const keyPart = parts[parts.length - 1] ?? "";
  const mods = new Set<string>();
  for (const part of parts.slice(0, -1)) {
    const m = MODIFIERS.find((x) => x.toLowerCase() === part.toLowerCase());
    if (!m) return null;
    mods.add(m);
  }
  if (MODIFIERS.some((m) => m.toLowerCase() === keyPart.toLowerCase())) return null;
  const key = keyName(keyPart);
  return [...MODIFIERS.filter((m) => mods.has(m)), key].join("+");
}

export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** The binding a key press spells, or null while only a modifier is held. */
export function bindingFromEvent(e: KeyLike): string | null {
  if (["Control", "Alt", "Shift", "Meta", "AltGraph", "Dead", "Unidentified"].includes(e.key)) {
    return null;
  }
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Meta"];
  return [...mods.filter(Boolean), keyName(e.key)].join("+");
}

export function matchesEvent(binding: string, e: KeyLike): boolean {
  return bindingFromEvent(e) === binding;
}

/** For the screen: "Ctrl+Delete" → "Ctrl + Delete". */
export function formatBinding(binding: string): string {
  return binding.replaceAll("+", " + ").replace(/ \+ $/, " +");
}

export type ProblemKind = "invalid" | "reserved" | "plain" | "conflict";

export interface BindingProblem {
  kind: ProblemKind;
  message: string;
  /** The action that already uses the binding. */
  with?: ShortcutAction;
}

const FUNCTION_KEY = /^F([1-9]|1[0-2])$/;

function labelOf(id: ShortcutAction): string {
  return SHORTCUT_ACTIONS.find((a) => a.id === id)?.label ?? id;
}

/**
 * Whether `binding` may be given to `action`, with the other actions' current bindings in
 * `current`. A shortcut must work while the search box has the focus (a scanner types into it), so
 * a plain letter, digit or Shift+letter is not allowed: use a function key, or Ctrl or Alt with
 * another key.
 */
export function validateBinding(
  binding: string,
  action: ShortcutAction,
  current: Bindings,
): BindingProblem | null {
  const normal = normalizeBinding(binding);
  if (!normal) return { kind: "invalid", message: "That is not a key combination." };
  if (RESERVED.has(normal)) {
    return {
      kind: "reserved",
      message: `${formatBinding(normal)} is used by the app or the system. Choose another.`,
    };
  }
  const parts = normal.split("+");
  const key = parts[parts.length - 1] ?? "";
  const hasCtrlOrAlt = parts.includes("Ctrl") || parts.includes("Alt");
  if (!FUNCTION_KEY.test(key) && !hasCtrlOrAlt) {
    return {
      kind: "plain",
      message: "Use a function key (F1 to F12), or hold Ctrl or Alt with another key.",
    };
  }
  const other = SHORTCUT_ACTIONS.find((a) => a.id !== action && current[a.id] === normal);
  if (other) {
    return {
      kind: "conflict",
      message: `${formatBinding(normal)} already does “${labelOf(other.id)}”.`,
      with: other.id,
    };
  }
  return null;
}

/**
 * What was saved for a user, mended: unknown actions, bindings that are no longer allowed and
 * clashes are dropped and the default is used instead, so a damaged or outdated save never leaves
 * the till without a key.
 */
export function mergeSaved(saved: unknown): Bindings {
  const result: Bindings = { ...DEFAULT_BINDINGS };
  if (typeof saved !== "object" || saved === null) return result;
  const record = saved as Record<string, unknown>;
  for (const def of SHORTCUT_ACTIONS) {
    const value = record[def.id];
    if (typeof value !== "string") continue;
    const normal = normalizeBinding(value);
    if (!normal) continue;
    if (validateBinding(normal, def.id, result) === null) result[def.id] = normal;
  }
  return result;
}

/** Only what differs from the defaults is saved. */
export function overridesOf(bindings: Bindings): Partial<Bindings> {
  const out: Partial<Bindings> = {};
  for (const def of SHORTCUT_ACTIONS) {
    if (bindings[def.id] !== def.default) out[def.id] = bindings[def.id];
  }
  return out;
}
