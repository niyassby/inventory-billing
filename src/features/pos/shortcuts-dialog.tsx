import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ShortcutsEditor } from "./shortcuts-editor";

/** The shortcut editor, opened from the till without leaving the bill. */
export function ShortcutsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Till shortcuts</DialogTitle>
          <DialogDescription>
            Choose the keys you like best. Changes apply at once.
          </DialogDescription>
        </DialogHeader>
        <ShortcutsEditor />
      </DialogContent>
    </Dialog>
  );
}
