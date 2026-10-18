import { useEffect, useRef, useState } from "react";
import { dragGrip } from "../lib/drag";
import { read, write } from "../lib/persist";
import type { BrowserView } from "../lib/types";
import { BrowserPanel, useBrowserView } from "./BrowserPanel";
import { GlobeIcon } from "./icons";

/**
 * Whether a browser is shown beside a task's terminals or the chats (BRW-9),
 * remembered per task. `"chat"` is the chats' one browser (BRW-2).
 */
export function useBrowserSide(taskId: string) {
  const [open, setOpen] = useState(() => read<boolean>(`browserOpen.${taskId}`, false));
  useEffect(() => setOpen(read<boolean>(`browserOpen.${taskId}`, false)), [taskId]);
  const show = (on: boolean) => {
    setOpen(on);
    write(`browserOpen.${taskId}`, on);
  };
  const view = useBrowserView(taskId);
  // An agent asking for a site is waiting on the answer, which is given in
  // the panel (BRW-11): it opens when a request comes.
  const asking = view?.requests.length ?? 0;
  useEffect(() => {
    if (asking > 0) {
      setOpen(true);
      write(`browserOpen.${taskId}`, true);
    }
  }, [asking, taskId]);
  return { open, show, view };
}

/** The globe that shows and hides it; a dot says agents have used it. */
export function BrowserToggle({ open, view, whose, onToggle }: {
  open: boolean;
  view: BrowserView | null;
  /** "this task's", "the chats'". */
  whose: string;
  onToggle: () => void;
}) {
  return (
    <button
      className={`ui-button btn-add${open ? " active" : ""}`}
      title={open ? `Hide ${whose} browser` : view?.tab ? `Show ${whose} browser: agents have used it` : `Show ${whose} browser`}
      onClick={onToggle}
    >
      <GlobeIcon />
      {!open && view?.tab && <span className="ui-live-dot browser-dot" />}
    </button>
  );
}

/**
 * The browser, after a grip that resizes it against what is beside it: the
 * last children of a flex row.
 */
export function BrowserSide({ taskId, visible, onClose }: { taskId: string; visible: boolean; onClose: () => void }) {
  const [share, setShare] = useState(() => read<number>("browserShare", 0.5));
  const sideRef = useRef<HTMLDivElement>(null);
  return (
    <>
      <div
        className="diff-grip"
        title="Drag to resize"
        onMouseDown={(e) => {
          const box = e.currentTarget.parentElement?.getBoundingClientRect();
          if (!box) return;
          let next = share;
          // Only the side's style changes while dragging: a render per step
          // redrew the whole browser panel. Kept, and remembered, on the drop.
          dragGrip(e, (m) => {
            // Clamped so neither side can be dragged away: the agent's
            // terminal and the page it works on are both the point.
            next = Math.min(0.8, Math.max(0.2, (box.right - m.clientX) / box.width));
            if (sideRef.current) sideRef.current.style.flexBasis = `${next * 100}%`;
          }, () => { setShare(next); write("browserShare", next); });
        }}
      />
      <div className="browser-side" ref={sideRef} style={{ flexBasis: `${share * 100}%` }}>
        <BrowserPanel taskId={taskId} visible={visible} onClose={onClose} />
      </div>
    </>
  );
}
