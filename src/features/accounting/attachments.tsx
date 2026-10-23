import { useRef } from "react";
import type { Api } from "@inventory/shared";
import { Download, Paperclip, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { call } from "@/api/v2";
import type { CommandName } from "@/api/v2";
import { downloadBlob } from "@/lib/export";
import { fileToBase64 } from "@/lib/files";
import { showDateTime } from "@/lib/format";
import { errorText, useAction } from "@/lib/queries";

function sizeText(bytes: number): string {
  return bytes < 1024 * 1024
    ? `${Math.max(1, Math.round(bytes / 1024))} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Download an attachment (the file comes through the command API, from the hub on a counter). */
async function download(id: string): Promise<void> {
  try {
    const file = await call("attachments_get", { id });
    const bytes = Uint8Array.from(atob(file.data), (c) => c.charCodeAt(0));
    downloadBlob(file.fileName, new Blob([bytes], { type: file.contentType }));
  } catch (error) {
    toast.error(errorText(error));
  }
}

/** Files on a record: download each, and add or remove them while the record can change. */
export function Attachments({
  entityType,
  entityId,
  items,
  editable,
  invalidates,
}: {
  entityType: "manual_journal";
  entityId: string;
  items: Api.AttachmentInfo[];
  editable: boolean;
  invalidates: CommandName[];
}) {
  const input = useRef<HTMLInputElement>(null);
  const add = useAction("attachments_add", { invalidates, success: "File attached" });
  const remove = useAction("attachments_remove", { invalidates, success: "File removed" });
  return (
    <div className="flex flex-col gap-2">
      {items.length === 0 ? (
        <p className="text-sm text-muted-foreground">No files attached.</p>
      ) : (
        <ul className="flex flex-col gap-1" aria-label="Attached files">
          {items.map((a) => (
            <li key={a.id} className="flex items-center gap-2 text-sm">
              <Paperclip className="size-4 shrink-0 text-muted-foreground" aria-hidden />
              <span className="min-w-0 flex-1 truncate">
                {a.fileName}
                <span className="ml-2 text-xs text-muted-foreground">
                  {sizeText(a.sizeBytes)} · {showDateTime(a.createdAt)}
                </span>
              </span>
              <Button
                size="icon-sm"
                variant="ghost"
                aria-label={`Download ${a.fileName}`}
                onClick={() => void download(a.id)}
              >
                <Download />
              </Button>
              {editable && (
                <Button
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Remove ${a.fileName}`}
                  onClick={() => remove.mutate({ input: { id: a.id } })}
                >
                  <Trash2 />
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {editable && (
        <>
          <input
            ref={input}
            type="file"
            accept="application/pdf,image/png,image/jpeg,image/webp"
            className="hidden"
            aria-label="Choose a file to attach"
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (!file) return;
              if (file.size > 5 * 1024 * 1024) {
                toast.error("Files can be at most 5 MB.");
                return;
              }
              void fileToBase64(file).then((data) =>
                add.mutate({ input: { entityType, entityId, fileName: file.name, data } }),
              );
            }}
          />
          <div>
            <Button
              size="sm"
              variant="outline"
              disabled={add.isPending}
              onClick={() => input.current?.click()}
            >
              <Paperclip /> {add.isPending ? "Attaching…" : "Attach a file"}
            </Button>
            <span className="ml-2 text-xs text-muted-foreground">PDF or picture, up to 5 MB</span>
          </div>
        </>
      )}
    </div>
  );
}
