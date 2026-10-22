import { useEffect, useState } from "react";
import { refusals, useSyncState } from "../../lib/workspace-sync";

/**
 * Whether the shared setup is saved, waiting or refused, at the foot of the
 * sidebar (PROF-19). Drawn with Layer's own CSS, but word for word and dot
 * for dot the kit's `SyncIndicator` (`code/kit/web/src/sync-indicator.tsx`),
 * which every other app shows: the kit's is drawn with Tailwind, which this
 * window has not.
 */
export function SyncStatus() {
  const status = useSyncState();
  const refused = useLatestRefusal();
  // Changes usually reach the server in a blink; saying "Syncing" for that
  // only makes the foot flicker.
  const slow = useLasting(status.pending > 0, 1000);

  if (refused) return <Pill tone="danger" label={`Change refused: ${refused}`} />;
  if (status.connection === "offline") {
    const waiting = status.pending > 0 ? ` · ${status.pending} waiting` : "";
    return <Pill tone="warn" label={`Offline${waiting}`} />;
  }
  if (status.connection === "connecting" && !status.ready) return <Pill tone="muted" label="Connecting…" />;
  if (status.pending > 0 && slow) return <Pill tone="muted" label={`Syncing ${status.pending}`} />;
  return <Pill tone="ok" label="Synced" />;
}

function Pill({ tone, label }: { tone: "ok" | "warn" | "danger" | "muted"; label: string }) {
  return (
    <span className="sync-indicator" role="status" title={label}>
      <span className={`sync-dot ${tone}`} />
      <span className="sync-label">{label}</span>
    </span>
  );
}

function useLatestRefusal(): string | null {
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const onRefused = (event: Event) => {
      setMessage((event as CustomEvent<string>).detail);
      clearTimeout(timer);
      timer = setTimeout(() => setMessage(null), 6000);
    };
    refusals.addEventListener("refused", onRefused);
    return () => {
      refusals.removeEventListener("refused", onRefused);
      clearTimeout(timer);
    };
  }, []);
  return message;
}

/** Whether `on` has stayed true for `ms`; false again as soon as it isn't. */
function useLasting(on: boolean, ms: number): boolean {
  const [lasting, setLasting] = useState(false);
  useEffect(() => {
    if (!on) {
      setLasting(false);
      return;
    }
    const timer = setTimeout(() => setLasting(true), ms);
    return () => clearTimeout(timer);
  }, [on, ms]);
  return lasting;
}
