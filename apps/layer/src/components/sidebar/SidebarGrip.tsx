import { useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { CHAT_LIST_DEFAULT, clampSidebar, dragGrip, SIDEBAR_DEFAULT, SIDEBAR_MIN, sidebarMost } from "../../lib/drag";
import { read, write } from "../../lib/persist";
import { useStore } from "../../store";

/** What each grip resizes: where its width is kept, the property it sets, its usual width. */
const SIDES = {
  tasks: { key: "sidebarWidth", prop: "--sidebar-w", usual: SIDEBAR_DEFAULT, label: "Resize the task list" },
  chats: { key: "chatListWidth", prop: "--chat-list-w", usual: CHAT_LIST_DEFAULT, label: "Resize the chat list" },
} as const;

/** How far an arrow key moves the edge; with ⇧, four times as far. */
const STEP = 16;

const scale = () => useStore.getState().settings?.ui.scale ?? 1;

/**
 * A sidebar's right edge, dragged to make it wider or narrower: the task
 * list's (TASK-16), or the Chat view's list of chats (CHAT-4). Long names
 * were cut short at a width nobody could change.
 *
 * It sits at zero width between the list and what is beside it (`.app`'s
 * middle column, or between `.chat`'s two), and sets its width property on
 * that parent. While dragging, only that property changes, with no React
 * render per step; the width is kept, and remembered, on the drop.
 */
export function SidebarGrip({ side = "tasks" }: { side?: keyof typeof SIDES }) {
  const { key, prop, usual, label } = SIDES[side];
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(() => clampSidebar(read<unknown>(key, usual), innerWidth, scale(), usual));

  const apply = (w: number) => {
    const el = ref.current;
    el?.parentElement?.style.setProperty(prop, `${w}px`);
    el?.setAttribute("aria-valuenow", String(w));
  };
  useLayoutEffect(() => apply(width), [width]);

  const keep = (w: number) => {
    const next = clampSidebar(w, innerWidth, scale(), usual);
    apply(next);
    setWidth(next);
    write(key, next);
  };

  const onKey = (e: KeyboardEvent) => {
    const step = e.shiftKey ? STEP * 4 : STEP;
    if (e.key === "ArrowLeft") keep(width - step);
    else if (e.key === "ArrowRight") keep(width + step);
    else if (e.key === "Home") keep(SIDEBAR_MIN);
    else if (e.key === "End") keep(sidebarMost(innerWidth, scale()));
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={ref}
      className="sidebar-grip"
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={SIDEBAR_MIN}
      aria-valuemax={sidebarMost(innerWidth, scale())}
      tabIndex={0}
      title="Drag to resize; double-click for the usual width"
      onMouseDown={(e) => {
        // Not onDoubleClick: the drag the first press starts gives the
        // release to `body`, so the dblclick event never reaches the grip.
        if (e.detail === 2) {
          e.preventDefault();
          keep(usual);
          return;
        }
        const startX = e.clientX;
        // The mouse moves in the window's pixels, the sidebar in the UI's,
        // which `.shell`'s zoom makes larger.
        const s = scale();
        let next = width;
        const grip = e.currentTarget;
        const held = dragGrip(e, (m) => {
          next = clampSidebar(width + (m.clientX - startX) / s, innerWidth, s, usual);
          apply(next);
        }, () => {
          grip.classList.remove("held");
          keep(next);
        });
        if (held) grip.classList.add("held");
      }}
      onKeyDown={onKey}
    />
  );
}
