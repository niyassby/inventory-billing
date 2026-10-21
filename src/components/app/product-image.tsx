import { Package } from "lucide-react";
import { Avatar } from "@/components/ui/avatar";
import { useCommand } from "@/lib/queries";
import { cn } from "@/lib/utils";

const BOX = {
  sm: "size-7",
  md: "size-10",
  lg: "size-16",
  xl: "size-40",
} as const;

export type ProductImageSize = keyof typeof BOX;

/**
 * A product's picture, or a quiet placeholder when it has none (or the hub cannot be reached).
 * The picture is fetched through the app's own command, so it works from any counter; the
 * `imageKey` is part of the cache key, so a new picture replaces the old one on every screen.
 * It is decorative: the product's name is always next to it.
 */
export function ProductImage({
  productId,
  imageKey,
  size = "md",
  name,
  className,
}: {
  productId: string;
  imageKey: string | null | undefined;
  size?: ProductImageSize;
  /** With a name, the placeholder is the product's letter avatar instead of a plain icon. */
  name?: string;
  className?: string;
}) {
  const picture = useCommand(
    "products_image_get",
    {
      productId,
      size: size === "lg" || size === "xl" ? "full" : "thumb",
      version: imageKey ?? null,
    },
    { enabled: Boolean(imageKey), staleTime: Infinity, retry: false },
  );
  const frame = cn(
    "inline-flex shrink-0 items-center justify-center overflow-hidden rounded-md border border-border bg-muted text-muted-foreground",
    BOX[size],
    className,
  );
  if (picture.data) {
    return (
      <span className={frame} aria-hidden>
        <img
          src={`data:${picture.data.contentType};base64,${picture.data.data}`}
          alt=""
          className="size-full object-cover"
          loading="lazy"
          draggable={false}
        />
      </span>
    );
  }
  if (name && (size === "sm" || size === "md")) {
    return (
      <Avatar
        name={name}
        size={size === "sm" ? "sm" : "default"}
        className={cn(imageKey && picture.isPending && "animate-pulse", className)}
      />
    );
  }
  return (
    <span className={cn(frame, imageKey && picture.isPending && "animate-pulse")} aria-hidden>
      <Package className={size === "sm" ? "size-3.5" : size === "md" ? "size-4" : "size-6"} />
    </span>
  );
}
