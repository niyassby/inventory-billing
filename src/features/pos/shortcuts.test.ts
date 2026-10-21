import { describe, expect, it } from "vitest";
import {
  DEFAULT_BINDINGS,
  bindingFromEvent,
  formatBinding,
  matchesEvent,
  mergeSaved,
  normalizeBinding,
  overridesOf,
  validateBinding,
} from "./shortcuts";

const press = (
  key: string,
  mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {},
) => ({
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("writing a key press down", () => {
  it("spells modifiers in a fixed order and capitalises letters", () => {
    expect(bindingFromEvent(press("F10"))).toBe("F10");
    expect(bindingFromEvent(press("Delete", { ctrlKey: true }))).toBe("Ctrl+Delete");
    expect(bindingFromEvent(press("k", { shiftKey: true, ctrlKey: true, altKey: true }))).toBe(
      "Ctrl+Alt+Shift+K",
    );
  });

  it("says nothing while only a modifier is held", () => {
    expect(bindingFromEvent(press("Control", { ctrlKey: true }))).toBeNull();
    expect(bindingFromEvent(press("Shift", { shiftKey: true }))).toBeNull();
  });

  it("matches a press against a binding exactly", () => {
    expect(matchesEvent("F8", press("F8"))).toBe(true);
    expect(matchesEvent("F8", press("F8", { shiftKey: true }))).toBe(false);
    expect(matchesEvent("Ctrl+Delete", press("Delete", { ctrlKey: true }))).toBe(true);
  });

  it("normalises typed text and rejects nonsense", () => {
    expect(normalizeBinding("ctrl + delete")).toBe("Ctrl+Delete");
    expect(normalizeBinding("shift+ctrl+x")).toBe("Ctrl+Shift+X");
    expect(normalizeBinding("")).toBeNull();
    expect(normalizeBinding("Hyper+X")).toBeNull();
    expect(normalizeBinding("Ctrl")).toBeNull();
    expect(formatBinding("Ctrl+Delete")).toBe("Ctrl + Delete");
  });
});

describe("which bindings are allowed", () => {
  it("accepts function keys and Ctrl or Alt combinations", () => {
    expect(validateBinding("F6", "hold", DEFAULT_BINDINGS)).toBeNull();
    expect(validateBinding("Ctrl+H", "hold", DEFAULT_BINDINGS)).toBeNull();
    expect(validateBinding("Alt+1", "hold", DEFAULT_BINDINGS)).toBeNull();
  });

  it("keeps the keys the app, the system and the till itself use", () => {
    for (const key of [
      "F5",
      "F11",
      "F12",
      "F2",
      "Ctrl+K",
      "Ctrl+W",
      "Alt+F4",
      "Enter",
      "Escape",
      "Ctrl+C",
      "Delete",
    ]) {
      expect(validateBinding(key, "hold", DEFAULT_BINDINGS)?.kind, key).toBe("reserved");
    }
  });

  it("refuses keys that would type into the search box", () => {
    for (const key of ["A", "5", "Shift+A", "+"]) {
      expect(validateBinding(key, "hold", DEFAULT_BINDINGS)?.kind, key).not.toBeNull();
    }
    expect(validateBinding("A", "hold", DEFAULT_BINDINGS)?.kind).toBe("plain");
  });

  it("names the action that already has the key", () => {
    const clash = validateBinding("F10", "hold", DEFAULT_BINDINGS);
    expect(clash?.kind).toBe("conflict");
    expect(clash?.with).toBe("pay");
    // An action may keep its own key.
    expect(validateBinding("F10", "pay", DEFAULT_BINDINGS)).toBeNull();
  });
});

describe("saved shortcuts", () => {
  it("uses the defaults when nothing was saved", () => {
    expect(mergeSaved(null)).toEqual(DEFAULT_BINDINGS);
    expect(mergeSaved({})).toEqual(DEFAULT_BINDINGS);
    expect(mergeSaved("junk")).toEqual(DEFAULT_BINDINGS);
  });

  it("applies valid changes and drops the rest", () => {
    const merged = mergeSaved({
      customer: "F6",
      hold: "F6", // clashes with the customer binding given first: back to the default
      pay: "F5", // reserved: back to the default
      search: "x", // plain key: back to the default
      nonsense: "F9",
    });
    expect(merged.customer).toBe("F6");
    expect(merged.pay).toBe("F10");
    expect(merged.search).toBe("F1");
    expect(merged.hold).toBe("F8");
  });

  it("saves only what differs from the defaults", () => {
    expect(overridesOf(DEFAULT_BINDINGS)).toEqual({});
    expect(overridesOf({ ...DEFAULT_BINDINGS, hold: "F6" })).toEqual({ hold: "F6" });
  });
});
