/** True while the key goes to a field, so letters are typed rather than shortcuts. */
export function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

/** True while a dialog or menu is open: shortcuts wait until it closes. */
export function overlayOpen(): boolean {
  return document.querySelector("[role=dialog],[role=alertdialog],[role=menu]") !== null;
}

/** Whether a plain key press is free for a shortcut: no modifier, no field, no dialog. */
export function shortcutFree(event: KeyboardEvent): boolean {
  return !(
    event.metaKey ||
    event.ctrlKey ||
    event.altKey ||
    isTyping(event.target) ||
    overlayOpen()
  );
}
