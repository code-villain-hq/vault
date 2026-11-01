// Pieces every page shares, so the same job looks the same everywhere: a +
// that opens a New dialog, a ⋯ menu for rare actions, and quiet chips.
import { Dialog } from "@base-ui/react/dialog";
import { Menu } from "@base-ui/react/menu";
import { ChevronRight, Ellipsis, Plus } from "lucide-react";
import type { ReactNode } from "react";
import { focusBackAfterKeys, useDialogFocus } from "./dialog-focus";

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

/** An action in a ⋯ menu, with its icon when the menu shows icons. */
export type MoreMenuAction = {
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  onSelect: () => void;
};

/**
 * A row of a ⋯ menu: an action; a choice of one of a few, side by side
 * (Width ¼ ⅓ ½), which changes as it's picked and leaves the menu open; or
 * a menu of its own beside it (Add below ›), its actions in groups with a
 * line between them.
 */
export type MoreMenuItem =
  | MoreMenuAction
  | {
      label: string;
      choices: { value: string; label: string; title: string }[];
      value: string;
      onChoose: (value: string) => void;
    }
  | { label: string; groups: MoreMenuAction[][] };

/** A "⋯" button with a short menu of a page's rarer actions. */
export function MoreMenu({ label, items }: { label: string; items: MoreMenuItem[] }) {
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
          <Menu.Popup finalFocus={focusBackAfterKeys} className="ui-menu min-w-44 p-1 outline-none">
            {items.map((item) =>
              "choices" in item ? (
                <Menu.Group key={item.label} className="flex items-center gap-4 px-2 py-1">
                  <Menu.GroupLabel>{item.label}</Menu.GroupLabel>
                  <Menu.RadioGroup
                    value={item.value}
                    onValueChange={(value: string) => item.onChoose(value)}
                    className="ui-segmented ml-auto"
                  >
                    {item.choices.map((choice) => (
                      <Menu.RadioItem
                        key={choice.value}
                        value={choice.value}
                        aria-label={choice.title}
                        title={choice.title}
                        className="ui-segment cursor-default outline-none select-none data-checked:bg-[color-mix(in_srgb,var(--fg)_var(--button-hover-tint),transparent)] data-checked:font-bold data-checked:text-fg data-highlighted:text-fg"
                      >
                        {choice.label}
                      </Menu.RadioItem>
                    ))}
                  </Menu.RadioGroup>
                </Menu.Group>
              ) : "groups" in item ? (
                <Menu.SubmenuRoot key={item.label}>
                  <Menu.SubmenuTrigger className={`${option} justify-between gap-4`}>
                    {item.label}
                    <ChevronRight
                      size={14}
                      strokeWidth={1.75}
                      aria-hidden="true"
                      className="text-faint"
                    />
                  </Menu.SubmenuTrigger>
                  <Menu.Portal>
                    <Menu.Positioner className="z-50 outline-none" sideOffset={4} alignOffset={-5}>
                      <Menu.Popup className="ui-menu min-w-40 p-1 outline-none">
                        {item.groups.map((group, n) => (
                          <Menu.Group key={group[0]?.label ?? n}>
                            {n > 0 && <div role="separator" className="mx-1 my-1 h-px bg-line" />}
                            {group.map((action) => (
                              <MoreMenuOption key={action.label} action={action} />
                            ))}
                          </Menu.Group>
                        ))}
                      </Menu.Popup>
                    </Menu.Positioner>
                  </Menu.Portal>
                </Menu.SubmenuRoot>
              ) : (
                <MoreMenuOption key={item.label} action={item} />
              ),
            )}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

/** How a ⋯ menu's row looks. */
const option = "ui-option flex cursor-default items-center px-2 py-1.5 outline-none select-none";

function MoreMenuOption({ action }: { action: MoreMenuAction }) {
  return (
    <Menu.Item
      onClick={action.onSelect}
      className={`${option} gap-2.5 ${action.danger ? "text-danger-ink" : ""}`}
    >
      {action.icon && <span className="flex shrink-0 text-muted">{action.icon}</span>}
      {action.label}
    </Menu.Item>
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
