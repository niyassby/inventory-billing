import type * as React from "react";
import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

/** The search box used above lists: an input with a magnifier, and an accessible name. */
export function SearchInput({
  className,
  ...props
}: Omit<React.ComponentProps<"input">, "type"> & { "aria-label": string }) {
  return (
    <div className={cn("relative w-full max-w-xs", className)}>
      <Search
        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input type="search" className="pl-8" {...props} />
    </div>
  );
}
