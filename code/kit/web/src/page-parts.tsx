// Pieces every page shares, so the same job looks the same everywhere: a +
// that opens a New dialog, a ⋯ menu for rare actions, and quiet chips.
import { Dialog } from "@base-ui/react/dialog";
import { Menu } from "@base-ui/react/menu";
import { Ellipsis, Plus } from "lucide-react";
import type { ReactNode } from "react";
import { useDialogFocus } from "./dialog-focus";

/** A property chip: quiet until hovered. */
export const chip =
  "ui-chip flex h-7 max-w-64 items-center gap-1.5 px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover";

/** A page's "+": a plain icon that opens its New dialog. */
export function NewButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="ui-button ui-icon-button"
    >
      <Plus size={15} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

/** The window things are made in, like New issue; its form renders only while open. */
export function CreateDialog({
  open,
  onClose,
  children,
  width = "40rem",
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  width?: string;
}) {
  const finalFocus = useDialogFocus(open);
  return (
    <Dialog.Root open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="ui-backdrop fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup
          finalFocus={finalFocus}
          style={{ width: `min(${width}, calc(100vw - 2rem))` }}
          className="ui-dialog fixed top-[12vh] left-1/2 z-40 -translate-x-1/2 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"
        >
          {open && children}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** A "⋯" button with a short menu of a page's rarer actions. */
export function MoreMenu({
  label,
  items,
}: {
  label: string;
  items: { label: string; danger?: boolean; onSelect: () => void }[];
}) {
  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label={label}
        title={label}
        className="ui-button ui-icon-button data-popup-open:bg-hover"
      >
        <Ellipsis size={15} strokeWidth={1.75} aria-hidden="true" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="end">
          <Menu.Popup className="ui-menu min-w-44 p-1 outline-none">
            {items.map((item) => (
              <Menu.Item
                key={item.label}
                onClick={item.onSelect}
                className={`ui-option flex cursor-default items-center px-2 py-1.5 outline-none select-none ${item.danger ? "text-danger-ink" : ""}`}
              >
                {item.label}
              </Menu.Item>
            ))}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

/** A small icon button for a row's actions, such as copy or delete. */
export function RowAction({
  label,
  onClick,
  children,
  disabled,
  className = "",
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      className={`ui-button ui-icon-button ${className}`}
    >
      {children}
    </button>
  );
}
