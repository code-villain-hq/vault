import { useEffect, useState } from "react";
import { useSyncStatus, useWorkspace } from "./workspace";

/** Shows whether changes are saved, waiting, or refused. */
export function SyncIndicator() {
  const status = useSyncStatus();
  const refused = useLatestRefusal();
  // Changes usually reach the server in a blink; saying "Syncing" for that
  // only makes the header flicker.
  const slow = useLasting(status.pending > 0, 1000);

  if (refused) {
    return <Pill tone="danger" label={`Change refused: ${refused}`} />;
  }
  if (status.connection === "offline") {
    const waiting = status.pending > 0 ? ` · ${status.pending} waiting` : "";
    return <Pill tone="warn" label={`Offline${waiting}`} />;
  }
  if (status.connection === "connecting" && !status.ready) {
    return <Pill tone="muted" label="Connecting…" />;
  }
  if (status.pending > 0 && slow) {
    return <Pill tone="muted" label={`Syncing ${status.pending}`} />;
  }
  return <Pill tone="ok" label="Synced" />;
}

function Pill({ tone, label }: { tone: "ok" | "warn" | "danger" | "muted"; label: string }) {
  const dot = {
    ok: "bg-accent",
    warn: "bg-warn",
    danger: "bg-danger",
    muted: "bg-faint animate-pulse",
  }[tone];
  return (
    <span
      className="sync-indicator flex min-w-0 items-start gap-2 text-xs text-muted"
      role="status"
      title={label}
    >
      <span className={`mt-1.5 size-1.5 shrink-0 rounded-full ${dot}`} />
      <span className="sync-label">{label}</span>
    </span>
  );
}

function useLatestRefusal(): string | null {
  const { refusals } = useWorkspace();
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
  }, [refusals]);
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
