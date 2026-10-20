import { create } from "zustand";

export type Density = "comfortable" | "compact";

const KEY = "density";

function read(): Density {
  try {
    return localStorage.getItem(KEY) === "compact" ? "compact" : "comfortable";
  } catch {
    return "comfortable";
  }
}

export function applyDensity(density: Density): void {
  if (typeof document === "undefined") return;
  if (density === "compact") document.documentElement.dataset.density = "compact";
  else delete document.documentElement.dataset.density;
}

interface AppearanceState {
  density: Density;
  setDensity: (density: Density) => void;
}

/** Per-PC display preferences (theme itself is handled by next-themes). */
export const useAppearance = create<AppearanceState>((set) => ({
  density: read(),
  setDensity: (density) => {
    try {
      localStorage.setItem(KEY, density);
    } catch {
      // Not persisted in private mode; it still applies until the window closes.
    }
    applyDensity(density);
    set({ density });
  },
}));

applyDensity(useAppearance.getState().density);
