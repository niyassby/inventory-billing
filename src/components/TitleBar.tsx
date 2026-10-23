import { useEffect, useState } from "react";
import { Copy, Minus, Square, Store, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import { APP_NAME } from "@inventory/shared";
import { Button } from "@/components/ui/button";

/** Minimize, maximize/restore and close. The window has no frame of its own, so every screen needs these. */
function WindowControls({ size = "icon" }: { size?: "icon" | "icon-sm" }) {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;

    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let gone = false;

    const setup = async () => {
      setIsMaximized(await appWindow.isMaximized());

      const off = await appWindow.onResized(() => {
        void appWindow.isMaximized().then(setIsMaximized);
      });
      if (gone) off();
      else unlisten = off;
    };

    void setup();

    return () => {
      gone = true;
      unlisten?.();
    };
  }, []);

  // In a plain browser (development, tests) there is no window to control.
  if (!isTauri()) return null;

  const runWindowAction = async (action: "minimize" | "toggleMaximize" | "close") => {
    const appWindow = getCurrentWindow();

    if (action === "minimize") await appWindow.minimize();
    if (action === "toggleMaximize") {
      await appWindow.toggleMaximize();
      setIsMaximized(await appWindow.isMaximized());
    }
    if (action === "close") await appWindow.close();
  };

  return (
    <div className="flex items-center">
      <Button
        variant="ghost"
        size={size}
        aria-label="Minimize"
        onClick={() => void runWindowAction("minimize")}
      >
        <Minus className="size-4" />
      </Button>

      <Button
        variant="ghost"
        size={size}
        aria-label={isMaximized ? "Restore" : "Maximize"}
        onClick={() => void runWindowAction("toggleMaximize")}
      >
        {isMaximized ? <Copy className="size-3.5" /> : <Square className="size-3.5" />}
      </Button>

      <Button
        variant="ghost"
        size={size}
        aria-label="Close"
        className="hover:bg-danger hover:text-background"
        onClick={() => void runWindowAction("close")}
      >
        <X className="size-4" />
      </Button>
    </div>
  );
}

/** The controls at the end of the app header (the header itself is the drag area). */
export function TitleBar() {
  return (
    <div className="shrink-0 select-none">
      <WindowControls />
    </div>
  );
}

/**
 * A slim title strip for the screens that have no app header (sign-in, setup, the till, loading):
 * the app's name on the left, the window controls on the right, and the empty space drags the window.
 */
export function TitleStrip() {
  if (!isTauri()) return null;
  return (
    <div
      data-tauri-drag-region
      className="sticky top-0 z-50 flex h-8 shrink-0 items-center gap-2 border-b border-border bg-background pl-3 font-sans select-none"
    >
      <Store className="pointer-events-none size-3.5 text-primary" aria-hidden />
      <span className="pointer-events-none text-xs font-medium text-muted-foreground">
        {APP_NAME}
      </span>
      <div className="ml-auto">
        <WindowControls size="icon-sm" />
      </div>
    </div>
  );
}
