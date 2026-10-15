import type { MouseEvent as ReactMouseEvent } from "react";

/**
 * Dragging a grip that resizes part of the layout: the sidebar's edge
 * (TASK-16), the browser beside the terminals (BRW-9), the diff's file list.
 *
 * Every step of a drag resizes whatever is beside the grip, and a terminal
 * that is resized tells its process (`pty_resize`), which redraws its whole
 * screen for each new width. Terminals wait out a drag (`afterDrag`) and fit
 * once, to where it ended.
 */
let dragging = false;
const waiting = new Set<() => void>();

/** Whether a grip is being dragged right now. */
export function isDragging(): boolean {
  return dragging;
}

/** Run `fn` once the drag ends; asking twice for the same `fn` runs it once. */
export function afterDrag(fn: () => void): void {
  waiting.add(fn);
}

/**
 * Follow the mouse from a grip's mousedown until it is let go: `move` for
 * each step, then `done`.
 *
 * `body.dragging` takes pointer events from everything else while it lasts,
 * so the drag selects no text, lights no hover, and an agent that reads the
 * mouse (`xterm` reports motion to it) is not sent a stroke it never saw
 * start. False when it is not a drag (not the main button).
 */
export function dragGrip(e: ReactMouseEvent, move: (m: MouseEvent) => void, done: () => void): boolean {
  if (e.button !== 0) return false;
  e.preventDefault();
  dragging = true;
  document.body.classList.add("dragging");
  // A mouseup the window never saw (let go over another app, ⌘-Tab away
  // mid-drag) would leave every click in the app swallowed by
  // `body.dragging`: a move with no button down, or losing focus, ends it.
  const step = (m: MouseEvent) => (m.buttons === 0 ? up() : move(m));
  const up = () => {
    window.removeEventListener("mousemove", step);
    window.removeEventListener("mouseup", up);
    window.removeEventListener("blur", up);
    document.body.classList.remove("dragging");
    dragging = false;
    done();
    const fns = [...waiting];
    waiting.clear();
    for (const fn of fns) fn();
  };
  window.addEventListener("mousemove", step);
  window.addEventListener("mouseup", up);
  window.addEventListener("blur", up);
  return true;
}

export const SIDEBAR_DEFAULT = 260;
/** The Chat view's list of chats, which is resized the same way (CHAT-4). */
export const CHAT_LIST_DEFAULT = 230;
/** Narrower and the sidebar's own controls stop fitting on one line. */
export const SIDEBAR_MIN = 200;
const SIDEBAR_MAX = 560;

/**
 * The widest the sidebar may be: 560px, and never more than half the window,
 * so the terminals keep at least half of the smallest window (900px).
 * `viewport` is `innerWidth`; `scale` is the UI scale (`.shell`'s zoom), by
 * which the sidebar's own pixels are larger than the window's.
 */
export function sidebarMost(viewport: number, scale: number): number {
  return Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, Math.floor(viewport / 2 / scale)));
}

/** `width` held to what the sidebar may be; anything not a number is `usual`. */
export function clampSidebar(width: unknown, viewport: number, scale: number, usual = SIDEBAR_DEFAULT): number {
  const w = typeof width === "number" && Number.isFinite(width) ? width : usual;
  return Math.round(Math.min(sidebarMost(viewport, scale), Math.max(SIDEBAR_MIN, w)));
}
