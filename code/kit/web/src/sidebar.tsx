// The sidebar's own buttons, at the same spot in every app: search beside
// the app's name, shortcuts at the foot; and the edge that sets its width
// (layouts/app-window.md). Other panels take the same edge.
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

/** How wide a panel starts, and how narrow and wide its edge sets it, in pixels. */
export type Widths = { start: number; min: number; max: number };

/** How wide the sidebar starts, and how narrow and wide its edge sets it, in pixels. */
export const SIDEBAR_WIDTH: Widths = { start: 224, min: 180, max: 360 };

const within = (widths: Widths, width: number) =>
  Math.round(Math.min(widths.max, Math.max(widths.min, width)));

function keptWidth(key: string, widths: Widths): number {
  try {
    const kept = Number(localStorage.getItem(key));
    return kept ? within(widths, kept) : widths.start;
  } catch {
    return widths.start;
  }
}

/**
 * A panel's width as this device keeps it, for every workspace, under
 * `<app>:<name>-width`.
 */
export function useKeptWidth(name: string, widths: Widths) {
  const key = `${appConfig().key}:${name}-width`;
  const [width, setWidth] = useState(() => keptWidth(key, widths));
  const change = useCallback(
    (next: number) => {
      const kept = within(widths, next);
      setWidth(kept);
      try {
        localStorage.setItem(key, String(kept));
      } catch {
        // Storage can be off, in a private window: the width lasts until reload.
      }
    },
    [key, widths],
  );
  return { width, setWidth: change };
}

/**
 * The sidebar's width as this device keeps it, and the style that hands it
 * to the frame: the sidebar takes `--sidebar-width`.
 */
export function useSidebarWidth() {
  const { width, setWidth } = useKeptWidth("sidebar", SIDEBAR_WIDTH);
  const style = { "--sidebar-width": `${width}px` } as CSSProperties;
  return { width, setWidth, style };
}

/** The edge where the sidebar meets the content, between them in the frame. */
export function SidebarEdge(props: { width: number; onChange: (width: number) => void }) {
  return <WidthEdge {...props} widths={SIDEBAR_WIDTH} label="Sidebar width" />;
}

/**
 * The edge of a panel beside the content: dragging it sets the panel's
 * width, double-clicking puts it back, and ← and → move it while it has
 * focus. Under the pointer it shows as a line. A panel on the right
 * (`side="end"`) grows as the edge moves left.
 */
export function WidthEdge({
  width,
  onChange,
  widths,
  label,
  side = "start",
}: {
  width: number;
  onChange: (width: number) => void;
  widths: Widths;
  label: string;
  side?: "start" | "end";
}) {
  const from = useRef<{ x: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const grows = side === "start" ? 1 : -1;
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
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={widths.min}
      aria-valuemax={widths.max}
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
        if (from.current) onChange(from.current.width + grows * (event.clientX - from.current.x));
      }}
      onPointerUp={stop}
      onPointerCancel={stop}
      onDoubleClick={() => onChange(widths.start)}
      onKeyDown={(event) => {
        const step = event.key === "ArrowLeft" ? -16 : event.key === "ArrowRight" ? 16 : 0;
        if (!step) return;
        event.preventDefault();
        onChange(width + grows * step);
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
