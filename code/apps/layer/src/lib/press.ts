import type { KeyboardEvent } from "react";

/**
 * Enter or Space on something that acts as a button but can't be a
 * `<button>`, because it holds controls of its own (a row with a ✕, a card
 * with a link): the same as a click. A key pressed on one of those controls
 * is that control's. `act` gets the key, for a click that reads its modifiers.
 */
export function pressKeys(act: (e: KeyboardEvent) => void) {
  return (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
    e.preventDefault();
    act(e);
  };
}

/**
 * ⇧F10 or the context-menu key on something with a right-click menu: that
 * menu, at its foot, as macOS opens one for what has the focus. It arrives as
 * a contextmenu event, as a right-click's does, so the element's
 * `onContextMenu` serves both. Any other key goes on to `next`, usually
 * `pressKeys`.
 */
export function menuKeys(next?: (e: KeyboardEvent) => void) {
  return (e: KeyboardEvent) => {
    const asked = e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey);
    if (!asked || e.target !== e.currentTarget) {
      next?.(e);
      return;
    }
    e.preventDefault();
    const at = e.currentTarget.getBoundingClientRect();
    e.currentTarget.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: at.left,
        clientY: at.bottom + 4,
      }),
    );
  };
}

// Whether the last thing done was a key rather than the pointer: a menu
// opened from the keyboard chooses its first item, one opened with the mouse
// nothing (ContextMenu). The focus ring can't tell them apart. A row that had
// the keys keeps its ring when it is then right-clicked, and WebKit doesn't
// focus a button that is clicked, so the ring is still on whatever had it.
let keyed = false;
if (typeof window !== "undefined") {
  window.addEventListener(
    "keydown",
    () => {
      keyed = true;
    },
    true,
  );
  window.addEventListener(
    "pointerdown",
    () => {
      keyed = false;
    },
    true,
  );
}

/** Whether the last thing done was a key, not the pointer. */
export function byKeys() {
  return keyed;
}
