import { useStore } from "../store";

/**
 * What failed in the background, in the top bar left of the app's tools
 * (WIN-2): the latest failure for six seconds, else the task and pane polls
 * while they keep failing, else nothing. Forge's sync status, where Layer
 * has no sidebar foot to put it.
 */
export function FrameStatus() {
  const failure = useStore((s) => s.failure);
  const watchFailing = useStore((s) => s.watchFailing);
  const text = failure?.text ?? (watchFailing ? "Can't refresh tasks. Retrying…" : null);
  if (!text) return null;
  return (
    <span className={`frame-status${failure ? " danger" : ""}`} role="status" title={text}>
      <span className="frame-status-text">{text}</span>
    </span>
  );
}
