import { useRef, useState } from "react";
import type { Api } from "@inventory/shared";
import { toast } from "sonner";
import { ImagePlus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { confirm } from "@/components/app/dialogs";
import { ProductImage } from "@/components/app/product-image";
import { Section } from "@/components/app/page";
import { pictureForUpload, pictureProblem, PICTURE_TYPES } from "@/lib/images";
import { useAction } from "@/lib/queries";
import { run } from "@/lib/run";
import { cn } from "@/lib/utils";

const REFRESH = ["products_get", "products_list", "products_search", "products_image_get"] as const;

/**
 * The picture of a product on its form: preview, choose or drop a file, remove. The file is
 * checked and shrunk here, sent through the hub like any other change, and checked again by the
 * server. A new product has to be saved first (a picture belongs to a saved product).
 */
export function ProductPicture({
  product,
  canManage,
}: {
  product: Api.ProductDetail | null;
  canManage: boolean;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const save = useAction("products_image_set", {
    invalidates: [...REFRESH],
    success: "Picture saved",
  });
  const remove = useAction("products_image_remove", {
    invalidates: [...REFRESH],
    success: "Picture removed",
  });

  const send = async (file: File | undefined) => {
    if (!file || !product) return;
    const problem = pictureProblem(file);
    if (problem) {
      toast.error(problem);
      return;
    }
    setBusy(true);
    try {
      const data = await pictureForUpload(file);
      await save.mutateAsync({ input: { productId: product.id, data } });
    } catch {
      // The failure is already shown as a toast by the action.
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };

  return (
    <Section title="Picture">
      {product === null ? (
        <p className="text-sm text-muted-foreground">
          Save the product first, then add a picture to it.
        </p>
      ) : (
        <div className="flex flex-wrap items-center gap-5">
          <div
            onDragOver={(e) => {
              if (!canManage) return;
              e.preventDefault();
              setDragging(true);
            }}
            onDragLeave={() => setDragging(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragging(false);
              if (canManage) void send(e.dataTransfer.files[0]);
            }}
            className={cn("rounded-lg p-1 transition-colors", dragging && "bg-primary-soft")}
          >
            <ProductImage productId={product.id} imageKey={product.imageKey} size="xl" />
          </div>
          <div className="flex flex-col gap-2">
            {canManage && (
              <div className="flex flex-wrap gap-2">
                <input
                  ref={input}
                  type="file"
                  accept={PICTURE_TYPES.join(",")}
                  className="sr-only"
                  aria-label="Choose a picture file"
                  tabIndex={-1}
                  onChange={(e) => void send(e.target.files?.[0])}
                />
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => input.current?.click()}
                >
                  <ImagePlus aria-hidden />
                  {busy ? "Uploading…" : product.imageKey ? "Replace picture" : "Add a picture"}
                </Button>
                {product.imageKey && (
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={busy || remove.isPending}
                    onClick={run(async () => {
                      const ok = await confirm({
                        title: "Remove the picture?",
                        description: "The product keeps everything else.",
                        confirmLabel: "Remove",
                        destructive: true,
                      });
                      if (ok !== false) remove.mutate({ input: { productId: product.id } });
                    })}
                  >
                    <Trash2 aria-hidden />
                    Remove
                  </Button>
                )}
              </div>
            )}
            <p className="text-caption">
              PNG, JPEG, WebP or GIF, up to 5 MB. It is scaled down for you, and you can drop a file
              onto the picture.
            </p>
          </div>
        </div>
      )}
    </Section>
  );
}
