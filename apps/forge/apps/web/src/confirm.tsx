import { AlertDialog } from "@base-ui/react/alert-dialog";
import { type ReactNode, useState } from "react";
import { Button } from "./ui";

/** Something that can't be undone, waiting for a yes. */
export interface Confirmation {
  title: string;
  /** What will happen, in a sentence or two. */
  body: ReactNode;
  /** The button that does it, e.g. "Remove agent". */
  action: string;
  onConfirm: () => void;
}

/**
 * Asks before doing something destructive. Use instead of `window.confirm()`,
 * which embedded webviews (the desktop app among them) silently refuse.
 * Open while `confirmation` is set; Cancel has focus, so Enter is safe.
 */
export function ConfirmDialog({
  confirmation,
  onClose,
}: {
  confirmation: Confirmation | null;
  onClose: () => void;
}) {
  // Keeps showing what was asked while the dialog fades out.
  const [last, setLast] = useState(confirmation);
  if (confirmation && confirmation !== last) setLast(confirmation);
  const shown = confirmation ?? last;

  return (
    <AlertDialog.Root open={confirmation !== null} onOpenChange={(open) => !open && onClose()}>
      <AlertDialog.Portal>
        <AlertDialog.Backdrop className="app-backdrop fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <AlertDialog.Popup className="app-dialog fixed top-[20vh] left-1/2 z-40 w-[min(28rem,calc(100vw-2rem))] -translate-x-1/2 p-5 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
          <AlertDialog.Title className="mb-2 font-medium">{shown?.title}</AlertDialog.Title>
          <AlertDialog.Description className="text-muted">{shown?.body}</AlertDialog.Description>
          <div className="mt-5 flex justify-end gap-2">
            <AlertDialog.Close render={<Button variant="secondary" />}>Cancel</AlertDialog.Close>
            <Button
              variant="danger"
              onClick={() => {
                shown?.onConfirm();
                onClose();
              }}
            >
              {shown?.action}
            </Button>
          </div>
        </AlertDialog.Popup>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
