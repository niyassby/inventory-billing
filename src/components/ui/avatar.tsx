import * as React from "react";

import { cn } from "@/lib/utils";

// Decorative, so no green, amber or red (those mean paid, due, overdue elsewhere).
const TINTS = [
  "bg-primary-soft text-primary",
  "bg-info-soft text-info",
  "bg-accent text-accent-foreground",
  "bg-secondary text-secondary-foreground",
  "bg-muted text-muted-foreground",
] as const;

/** Up to two initials from a name or SKU. */
export function initials(text: string): string {
  const words = text
    .trim()
    .split(/[\s\-_.]+/)
    .filter(Boolean);
  const first = words[0]?.[0] ?? "?";
  const second = words.length > 1 ? (words[1]?.[0] ?? "") : (words[0]?.[1] ?? "");
  return (first + second).toUpperCase();
}

/** The same name always gets the same tint, so a list reads as a set of recognisable things. */
export function tintFor(text: string): string {
  let hash = 0;
  for (const ch of text) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return TINTS[hash % TINTS.length] ?? TINTS[4];
}

/** A letter avatar (products, people). Decorative: the name is next to it. */
function Avatar({
  name,
  size = "default",
  className,
  ...props
}: React.ComponentProps<"span"> & { name: string; size?: "sm" | "default" | "lg" }) {
  return (
    <span
      data-slot="avatar"
      aria-hidden
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-md font-semibold select-none",
        size === "sm" && "size-7 text-[0.6875rem]",
        size === "default" && "size-9 text-xs",
        size === "lg" && "size-12 text-sm",
        tintFor(name),
        className,
      )}
      {...props}
    >
      {initials(name)}
    </span>
  );
}

export { Avatar };
