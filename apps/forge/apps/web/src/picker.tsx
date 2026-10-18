import { Menu } from "@base-ui/react/menu";
import type { CSSProperties, KeyboardEvent, ReactNode } from "react";
import { CheckIcon } from "./icons";

export interface PickerOption {
  value: string;
  label: string;
  icon?: ReactNode;
}

const itemClass =
  "flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 outline-none select-none data-highlighted:bg-highlight";

/** Enter after typing a search picks the first match, as in a command menu. */
function chooseFirstMatch(event: KeyboardEvent<HTMLInputElement>) {
  if (event.key !== "Enter") return;
  const menu = event.currentTarget.closest("[role=dialog], [role=menu]");
  const items = "[role=menuitem], [role=menuitemcheckbox]";
  if (!menu || menu.querySelector(`:is(${items})[data-highlighted]`)) return;
  const first = menu.querySelector<HTMLElement>(items);
  if (!first) return;
  event.preventDefault();
  first.click();
}

/**
 * A searchable menu for choosing one value, or several with `multiple`. The
 * trigger is whatever `children` renders.
 */
export function Picker({
  label,
  options,
  selected,
  onSelect,
  multiple = false,
  children,
  triggerClassName = "",
  triggerStyle,
  triggerId,
  defaultOpen,
  onOpenChange,
}: {
  label: string;
  options: PickerOption[];
  /** The chosen value(s). */
  selected: string[];
  /** Single: the chosen value. Multiple: the value toggled. */
  onSelect: (value: string) => void;
  multiple?: boolean;
  children: ReactNode;
  triggerClassName?: string;
  triggerStyle?: CSSProperties;
  /** For a shortcut that opens the menu by clicking its trigger. */
  triggerId?: string;
  /** Open as soon as it appears, e.g. when another menu asked for it. */
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <Menu.FilterProvider>
      <Menu.Root defaultOpen={defaultOpen} onOpenChange={onOpenChange}>
        <Menu.Trigger
          id={triggerId}
          aria-label={label}
          style={triggerStyle}
          className={`flex items-center gap-1.5 outline-none focus-visible:ring-2 focus-visible:ring-accent ${triggerClassName.includes("ui-status") ? "" : "hover:bg-hover data-popup-open:bg-hover"} ${triggerClassName.includes("ui-chip") || triggerClassName.includes("ui-status") ? "" : "rounded-md"} ${triggerClassName}`}
          onClick={(event) => event.stopPropagation()}
        >
          {children}
        </Menu.Trigger>
        <Menu.Portal>
          {/* Layers: sticky headers 10, floating bars 30, dialogs 40, menus 50 (menus open from dialogs too). */}
          <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start">
            <Menu.Popup
              className="app-menu w-60 origin-(--transform-origin) outline-none transition-[scale,opacity] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"
              onClick={(event) => event.stopPropagation()}
            >
              <Menu.Input
                aria-label={`Search ${label.toLowerCase()}`}
                placeholder={`${label}…`}
                onKeyDown={chooseFirstMatch}
                className="h-9 w-full bg-transparent px-3 outline-none placeholder:text-faint"
              />
              <Menu.Empty className="px-3 py-2 text-muted">No matches</Menu.Empty>
              <Menu.List className="max-h-72 overflow-y-auto p-1">
                {options.map((option) =>
                  multiple ? (
                    <Menu.CheckboxItem
                      key={option.value}
                      className={itemClass}
                      checked={selected.includes(option.value)}
                      onCheckedChange={() => onSelect(option.value)}
                      closeOnClick={false}
                    >
                      {option.icon}
                      <span className="truncate">{option.label}</span>
                      <Menu.CheckboxItemIndicator className="ml-auto text-muted">
                        <CheckIcon />
                      </Menu.CheckboxItemIndicator>
                    </Menu.CheckboxItem>
                  ) : (
                    <Menu.Item
                      key={option.value}
                      className={itemClass}
                      onClick={() => onSelect(option.value)}
                    >
                      {option.icon}
                      <span className="truncate">{option.label}</span>
                      {selected.includes(option.value) && (
                        <span className="ml-auto text-muted">
                          <CheckIcon />
                        </span>
                      )}
                    </Menu.Item>
                  ),
                )}
              </Menu.List>
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>
    </Menu.FilterProvider>
  );
}
