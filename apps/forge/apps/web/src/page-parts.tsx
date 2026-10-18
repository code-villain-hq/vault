// Pieces every record page shares, so the same job looks the same everywhere
// (see the "Design principles" spec): a + that opens a New dialog, a ⋯ menu
// for rare actions, and properties as quiet chips under the title.
import { Dialog } from "@base-ui/react/dialog";
import { Menu } from "@base-ui/react/menu";
import type { User } from "@forge/types";
import { CalendarDays, Ellipsis, Plus } from "lucide-react";
import type { ReactNode } from "react";
import { statusStyle } from "./contrast";
import { formatDate } from "./data";
import { useDialogFocus } from "./dialog-focus";
import { Avatar } from "./icons";
import { Picker } from "./picker";

/** A property chip under a page's title: quiet until hovered. */
export const chip =
  "ui-chip flex h-7 max-w-64 items-center gap-1.5 px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover";

/**
 * A status's dot in its status ink: `status-<name>-ink` for the shared
 * colours, and for a chosen hue the ink `statusStyle` checks for it.
 * Inside a pill, `currentColor` takes the pill's ink.
 */
export function Dot({ color }: { color: string }) {
  const style =
    color === "currentColor"
      ? { background: color }
      : { ...statusStyle(color), background: "var(--status-ink)" };
  return <span className="size-2 shrink-0 rounded-full" style={style} aria-hidden="true" />;
}

/** A page's "+": a plain icon that opens its New dialog. */
export function NewButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="flex size-7 items-center justify-center rounded-md text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent"
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
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const finalFocus = useDialogFocus(open);
  return (
    <Dialog.Root open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="app-backdrop fixed inset-0 z-40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup
          finalFocus={finalFocus}
          className="app-dialog fixed top-[12vh] left-1/2 z-40 w-[min(40rem,calc(100vw-2rem))] -translate-x-1/2 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"
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
        className="flex size-7 items-center justify-center rounded-md text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover"
      >
        <Ellipsis size={15} strokeWidth={1.75} aria-hidden="true" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="end">
          <Menu.Popup className="app-menu min-w-44 p-1 outline-none">
            {items.map((item) => (
              <Menu.Item
                key={item.label}
                onClick={item.onSelect}
                className={`flex cursor-default items-center rounded-md px-2 py-1.5 outline-none select-none data-highlighted:bg-highlight ${item.danger ? "text-danger-ink" : ""}`}
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

/** A status as a chip under the title: its dot and name. */
export function StatusChip({
  statuses,
  value,
  onChange,
}: {
  statuses: { value: string; label: string; color: string }[];
  value: string;
  onChange: (value: string) => void;
}) {
  const current = statuses.find((s) => s.value === value);
  return (
    <Picker
      label="Status"
      options={statuses.map((s) => ({
        value: s.value,
        label: s.label,
        icon: <Dot color={s.color} />,
      }))}
      selected={[value]}
      onSelect={onChange}
      triggerClassName="ui-status flex h-7 max-w-64 items-center gap-1.5 px-2 outline-none focus-visible:ring-2 focus-visible:ring-accent"
      triggerStyle={statusStyle(current?.color)}
    >
      {current && <Dot color="currentColor" />}
      {current?.label}
    </Picker>
  );
}

/** A person as a chip, such as a project's lead: their avatar and name. */
export function PersonChip({
  label,
  users,
  value,
  onChange,
}: {
  label: string;
  users: User[];
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  const current = users.find((u) => u.id === value);
  return (
    <Picker
      label={label}
      options={[
        {
          value: "",
          label: `No ${label.toLowerCase()}`,
          icon: <Avatar user={undefined} size="size-4" />,
        },
        ...users.map((u) => ({
          value: u.id,
          label: u.name,
          icon: <Avatar user={u} size="size-4" />,
        })),
      ]}
      selected={[value ?? ""]}
      onSelect={(id) => onChange(id || null)}
      triggerClassName={chip}
    >
      <Avatar user={current} size="size-4" />
      {current?.name ?? label}
    </Picker>
  );
}

/** A date as a chip, such as a target date; clicking it opens the date picker. */
export function DateChip({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string | null;
  onChange: (date: string | null) => void;
}) {
  return (
    <label className={`${chip} relative`} title={label}>
      <CalendarDays size={12} strokeWidth={1.75} aria-hidden="true" />
      {value ? formatDate(value) : label}
      <input
        type="date"
        aria-label={label}
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value || null)}
        onClick={(e) => e.currentTarget.showPicker?.()}
        className="absolute inset-0 cursor-pointer opacity-0"
      />
    </label>
  );
}
