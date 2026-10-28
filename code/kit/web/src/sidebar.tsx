// The sidebar's own buttons, at the same spot in every app: search beside
// the app's name, shortcuts at the foot; and the edge that sets its width
// (layouts/app-window.md).
import { CircleHelp, Search } from "lucide-react";
import { type CSSProperties, useCallback, useRef, useState } from "react";
import { openCommandMenu } from "./command-menu";
import { appConfig } from "./places";
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

/** How wide the sidebar starts, and how narrow and wide its edge sets it, in pixels. */
export const SIDEBAR_WIDTH = { start: 224, min: 180, max: 360 };

const widthKey = () => `${appConfig().key}:sidebar-width`;
const within = (width: number) =>
  Math.round(Math.min(SIDEBAR_WIDTH.max, Math.max(SIDEBAR_WIDTH.min, width)));

function keptWidth(): number {
  try {
    const kept = Number(localStorage.getItem(widthKey()));
    return kept ? within(kept) : SIDEBAR_WIDTH.start;
  } catch {
    return SIDEBAR_WIDTH.start;
  }
}

/**
 * The sidebar's width as this device keeps it, for every workspace, and the
 * style that hands it to the frame: the sidebar takes `--sidebar-width`.
 */
export function useSidebarWidth() {
  const [width, setWidth] = useState(keptWidth);
  const change = useCallback((next: number) => {
    const kept = within(next);
    setWidth(kept);
    try {
      localStorage.setItem(widthKey(), String(kept));
    } catch {
      // Storage can be off, in a private window: the width lasts until reload.
    }
  }, []);
  const style = { "--sidebar-width": `${width}px` } as CSSProperties;
  return { width, setWidth: change, style };
}

/**
 * The edge where the sidebar meets the content, between them in the frame:
 * dragging it sets the sidebar's width, double-clicking puts it back, and
 * ← and → move it while it has focus. Under the pointer it shows as a line.
 */
export function SidebarEdge({
  width,
  onChange,
}: {
  width: number;
  onChange: (width: number) => void;
}) {
  const from = useRef<{ x: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const stop = () => {
    from.current = null;
    setDragging(false);
    document.documentElement.style.removeProperty("cursor");
    document.documentElement.style.removeProperty("user-select");
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable separator has no element of its own; <hr> takes no focus.
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      aria-valuenow={width}
      aria-valuemin={SIDEBAR_WIDTH.min}
      aria-valuemax={SIDEBAR_WIDTH.max}
      tabIndex={0}
      data-dragging={dragging || undefined}
      className="sidebar-edge group relative z-20 w-0 shrink-0 cursor-col-resize outline-none max-[560px]:hidden"
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        from.current = { x: event.clientX, width };
        setDragging(true);
        // The pointer leaves the edge as it drags; the cursor stays.
        document.documentElement.style.cursor = "col-resize";
        document.documentElement.style.userSelect = "none";
      }}
      onPointerMove={(event) => {
        if (from.current) onChange(from.current.width + event.clientX - from.current.x);
      }}
      onPointerUp={stop}
      onPointerCancel={stop}
      onDoubleClick={() => onChange(SIDEBAR_WIDTH.start)}
      onKeyDown={(event) => {
        const step = event.key === "ArrowLeft" ? -16 : event.key === "ArrowRight" ? 16 : 0;
        if (!step) return;
        event.preventDefault();
        onChange(width + step);
      }}
    >
      <span aria-hidden="true" className="absolute inset-y-0 -left-1 w-2" />
      <span
        aria-hidden="true"
        className="absolute inset-y-0 -left-px w-0.5 bg-faint opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100 group-data-dragging:opacity-100"
      />
    </div>
  );
}
