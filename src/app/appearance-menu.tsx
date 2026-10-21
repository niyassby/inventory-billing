import { useTheme } from "next-themes";
import { Monitor, Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { getCurrentWindow } from "@tauri-apps/api/window";

const ORDER = ["system", "light", "dark"] as const;
type Choice = (typeof ORDER)[number];
const LABEL: Record<Choice, string> = { system: "System", light: "Light", dark: "Dark" };

/** One button that cycles system, light and dark; the label says what is on now. */
export function ThemeButton() {
  const { theme = "system", setTheme } = useTheme();
  const current: Choice = ORDER.includes(theme as Choice) ? (theme as Choice) : "system";
  const next = ORDER[(ORDER.indexOf(current) + 1) % ORDER.length] ?? "system";
  const Icon = current === "dark" ? Moon : current === "light" ? Sun : Monitor;

  const changeTheme = async () => {
    setTheme(next);
    try {
      await getCurrentWindow().setTheme(next === "system" ? null : next);
    } catch (error) {
      console.error("Failed to update native window theme:", error);
    }
  };
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Theme: ${LABEL[current]}. Switch to ${LABEL[next]}`}
            onClick={() => void changeTheme()}
          />
        }
      >
        <Icon aria-hidden />
      </TooltipTrigger>
      <TooltipContent>Theme: {LABEL[current]}</TooltipContent>
    </Tooltip>
  );
}
