import type { ReactNode, RefObject } from "react";
import { useEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  /** Renders a divider above this item. */
  separated?: boolean;
}

/**
 * Something that floats over the window at a point: a menu, a panel.
 *
 * Shifts itself back on screen near an edge, since the sidebar is close to the
 * left and tasks near the bottom would otherwise open a menu below the window.
 * A press outside it or Escape closes it.
 */
export function Floating({
  x,
  y,
  align = "start",
  above = false,
  onClose,
  ignore,
  measureKey,
  className,
  children,
}: {
  x: number;
  y: number;
  /** Whether `x` is where its left edge goes, or its right edge. */
  align?: "start" | "end";
  /** `y` is where its bottom edge goes, for a button at the window's foot. */
  above?: boolean;
  onClose: () => void;
  /**
   * An element whose clicks must not dismiss it, so the button that opened
   * it can close it again instead of closing and reopening.
   */
  ignore?: RefObject<HTMLElement | null>;
  /** Changes when the contents may have changed size, to place it again. */
  measureKey?: unknown;
  className: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    // The app chrome is scaled with CSS `zoom`, which scales fixed positioning
    // too: a rect comes back in viewport pixels while `left`/`top` are set in
    // the element's own units. Rather than assume the factor — engines differ
    // on what they report — measure it: move the menu a known distance and see
    // how far it actually went.
    el.style.left = "0px";
    el.style.top = "0px";
    const origin = el.getBoundingClientRect();
    el.style.left = "100px";
    el.style.top = "100px";
    const moved = el.getBoundingClientRect();
    const sx = (moved.left - origin.left) / 100 || 1;
    const sy = (moved.top - origin.top) / 100 || 1;

    const want = align === "end" ? x - origin.width : x;
    const left = Math.max(8, Math.min(want, window.innerWidth - origin.width - 8));
    const top = Math.max(
      8,
      Math.min(above ? y - origin.height : y, window.innerHeight - origin.height - 8),
    );
    const placed = { left: (left - origin.left) / sx, top: (top - origin.top) / sy };
    // Onto the element as well as into state: measuring moved it, and when
    // the place comes out the same as last time React sees nothing to change
    // and leaves it where the measuring put it, 100px from the corner.
    el.style.left = `${placed.left}px`;
    el.style.top = `${placed.top}px`;
    setPos(placed);
  }, [x, y, align, above, measureKey]);

  useEffect(() => {
    const close = (e: MouseEvent) => {
      // A press inside must not dismiss it: closing on mousedown detaches
      // the item before the browser can deliver its click, and the item then
      // does nothing at all.
      if (ref.current?.contains(e.target as Node)) return;
      if (ignore?.current?.contains(e.target as Node)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    // Capture so a click anywhere dismisses before it does anything else.
    window.addEventListener("mousedown", close, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose, ignore]);

  return (
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions -- not a control: a press inside must not reach the window, which closes it
    <div
      ref={ref}
      className={className}
      // Hidden until measured, so it never flashes at the unadjusted spot.
      style={{ ...pos, visibility: pos ? "visible" : "hidden" }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {children}
    </div>
  );
}

/** A right-click menu anchored at the cursor. */
export function ContextMenu({
  x,
  y,
  items,
  onClose,
  ignore,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
  ignore?: RefObject<HTMLElement | null>;
}) {
  return (
    <Floating
      x={x}
      y={y}
      onClose={onClose}
      ignore={ignore}
      className="ui-menu ctx-menu"
      // Items can arrive after the menu opens — resumable agents are fetched —
      // and a taller menu needs placing again or it runs off the bottom.
      measureKey={items.length}
    >
      {items.map((item, i) => (
        <button
          // oxlint-disable-next-line react/no-array-index-key -- a menu's items, in order; a label can repeat
          key={i}
          className={`ctx-item ui-option${item.danger ? " danger" : ""}${
            item.separated ? " separated" : ""
          }`}
          disabled={item.disabled}
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          {item.label}
        </button>
      ))}
    </Floating>
  );
}
