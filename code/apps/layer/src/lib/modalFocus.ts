import type { RefObject } from "react";
import { useLayoutEffect, useRef, useState } from "react";

const STOPS = "a[href], button, input, select, textarea, [tabindex], [contenteditable]";

/** What Tab stops on inside `box`, in order. */
function stops(box: HTMLElement): HTMLElement[] {
  return [...box.querySelectorAll<HTMLElement>(STOPS)].filter(
    (el) => el.tabIndex >= 0 && !el.matches(":disabled") && el.getClientRects().length > 0,
  );
}

/**
 * Where a dialog's focus starts: its first field, or else its first button.
 * The ✕ in its head is the last resort: the dialog's own buttons say what it
 * is for.
 */
function firstStop(box: HTMLElement): HTMLElement | undefined {
  const all = stops(box);
  return (
    all.find((el) => el.matches("input, select, textarea")) ??
    all.find((el) => !el.closest(".modal-head")) ??
    all[0]
  );
}

/**
 * Where Tab goes from stop `at` of `count` (-1 when the focus is on none of
 * them), backwards with ⇧: round from either end, so it never leaves.
 */
export function nextStop(count: number, at: number, back: boolean): number {
  if (at < 0) return back ? count - 1 : 0;
  return (at + (back ? count - 1 : 1)) % count;
}

/** Only the dialog on top keeps the keys: a Confirm opened over a Modal. */
function onTop(box: HTMLElement): boolean {
  const open = document.querySelectorAll('[aria-modal="true"]');
  return open[open.length - 1] === box;
}

/**
 * Keeps the keyboard in a modal dialog while it is open (WIN-6). Before, the
 * focus stayed on the row or button behind it, and Tab walked the page
 * underneath.
 *
 * The focus moves in when it opens, unless a field's `autoFocus` has already
 * put it there. Tab and ⇧Tab go round the dialog's controls. Every control
 * is a stop, buttons too: WebKit leaves buttons out of Tab unless macOS's
 * keyboard navigation is on, and then a dialog's Cancel and its action
 * could not be reached at all. Closing it gives the focus back to what
 * opened it after keys (Esc, or Enter on a button), so they go on from
 * there. After a click only something you type in gets it back, a field or
 * a terminal: WebKit rings what script focuses, and a row or button behind
 * the dialog would keep a ring nobody asked for (`studio/design/behaviour.md`,
 * "Keys").
 *
 * The web kit's `useDialogFocus` does this for Base UI's dialogs, which are
 * mounted while closed. A Modal mounts as it opens, so what opened it is read
 * as it first renders, before the commit runs a field's `autoFocus`.
 */
export function useModalFocus(box: RefObject<HTMLElement | null>) {
  const [opener] = useState(() =>
    typeof document === "undefined" ? null : document.activeElement,
  );
  // Where the focus was in the dialog when its effect was last undone:
  // StrictMode undoes and redoes it on mount in development, and the redo
  // must not move the focus off a field's autoFocus.
  const held = useRef<HTMLElement | null>(null);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    if (held.current && el.contains(held.current)) held.current.focus();
    else if (!el.contains(document.activeElement)) firstStop(el)?.focus();

    let byKeys = false;
    const onKey = (e: KeyboardEvent) => {
      byKeys = true;
      if (e.key !== "Tab" || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      if (!onTop(el)) return;
      const all = stops(el);
      if (all.length === 0) return;
      e.preventDefault();
      const at = all.indexOf(document.activeElement as HTMLElement);
      all[nextStop(all.length, at, e.shiftKey)]?.focus();
    };
    const onPointer = () => {
      byKeys = false;
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onPointer, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onPointer, true);
      // This runs before React takes the dialog out of the page, so the focus
      // is still in it: unless what closed it put the focus somewhere else.
      const active = document.activeElement;
      const inside = active instanceof HTMLElement && el.contains(active);
      held.current = inside ? active : null;
      if (!inside && active && active !== document.body) return;
      if (!(opener instanceof HTMLElement) || !opener.isConnected) return;
      const typing = opener.matches("input, select, textarea") || opener.isContentEditable;
      if (byKeys || typing) opener.focus();
    };
  }, [box, opener]);
}
