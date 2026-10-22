// The sidebar's own buttons, at the same spot in every app: search beside
// the app's name, shortcuts at the foot (layouts/app-window.md).
import { CircleHelp, Search } from "lucide-react";
import { openCommandMenu } from "./command-menu";
import { openShortcuts } from "./shortcuts";

const iconButton = "ui-button ui-icon-button";

/** Opens the command menu, as ⌘K does. */
export function SearchButton() {
  return (
    <button
      type="button"
      aria-label="Search and commands"
      title="Search and commands (⌘K)"
      onClick={openCommandMenu}
      className={iconButton}
    >
      <Search size={15} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

/** Opens the list of shortcuts, as "?" does. */
export function ShortcutsButton() {
  return (
    <button
      type="button"
      aria-label="Keyboard shortcuts"
      title="Keyboard shortcuts (?)"
      onClick={openShortcuts}
      className={`${iconButton} ml-auto`}
    >
      <CircleHelp size={15} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}
