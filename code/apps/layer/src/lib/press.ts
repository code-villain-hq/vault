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
