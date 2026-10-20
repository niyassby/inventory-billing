import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTheme } from "next-themes";
import { Moon, Package, Rows3, Sun } from "lucide-react";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { useDebounced } from "@/components/app/pickers";
import { useCommand } from "@/lib/queries";
import { useAppearance } from "@/lib/appearance";
import { useSession } from "@/stores/session";
import { visibleNav } from "./nav";

/** Ctrl+K: jump to any screen the user may open, find a product, or switch the display. */
export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [term, setTerm] = useState("");
  const navigate = useNavigate();
  const { resolvedTheme, setTheme } = useTheme();
  const density = useAppearance((s) => s.density);
  const setDensity = useAppearance((s) => s.setDensity);
  const permissions = useSession((s) => s.user?.permissions ?? []);
  const canSearch = permissions.includes("products.view");
  const debounced = useDebounced(term.trim(), 200);
  const products = useCommand(
    "products_search",
    { term: debounced, limit: 8 },
    { enabled: open && canSearch && debounced.length >= 2 },
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const go = (to: string) => {
    setOpen(false);
    setTerm("");
    void navigate(to);
  };

  return (
    <CommandDialog
      open={open}
      onOpenChange={(next: boolean) => {
        setOpen(next);
        if (!next) setTerm("");
      }}
      title="Search"
      description="Go to a screen, find a product or change the display"
    >
      <CommandInput
        placeholder="Go to a screen or find a product"
        value={term}
        onValueChange={setTerm}
      />
      <CommandList>
        <CommandEmpty>Nothing matches.</CommandEmpty>
        {visibleNav(permissions).map((group) => (
          <CommandGroup key={group.label} heading={group.label}>
            {group.items.map((item) => (
              <CommandItem
                key={item.to}
                value={`${group.label} ${item.title}`}
                onSelect={() => go(item.to)}
              >
                <item.icon aria-hidden />
                {item.title}
              </CommandItem>
            ))}
          </CommandGroup>
        ))}
        {products.data && products.data.length > 0 && (
          <CommandGroup heading="Products">
            {products.data.map((p) => (
              <CommandItem
                key={`${p.productId}-${p.sku}`}
                value={`product ${p.name} ${p.sku} ${p.barcode ?? ""}`}
                onSelect={() => go(`/inventory/products/${p.productId}`)}
              >
                <Package aria-hidden />
                {p.name}
                <span className="ml-auto text-xs text-muted-foreground">{p.sku}</span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        <CommandGroup heading="Display">
          <CommandItem
            value="display switch light dark theme"
            onSelect={() => {
              setTheme(resolvedTheme === "dark" ? "light" : "dark");
              setOpen(false);
            }}
          >
            {resolvedTheme === "dark" ? <Sun aria-hidden /> : <Moon aria-hidden />}
            Switch to {resolvedTheme === "dark" ? "light" : "dark"} theme
          </CommandItem>
          <CommandItem
            value="display density compact comfortable"
            onSelect={() => {
              setDensity(density === "compact" ? "comfortable" : "compact");
              setOpen(false);
            }}
          >
            <Rows3 aria-hidden />
            Use {density === "compact" ? "comfortable" : "compact"} spacing
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
