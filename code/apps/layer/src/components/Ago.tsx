import { ago } from "../lib/time";
import { useNow } from "../store";

/**
 * "3m ago", on a clock of its own, so that the label is all the clock
 * redraws rather than the card or row around it.
 */
export function Ago({ since, everyMs = 5_000 }: { since: string; everyMs?: number }) {
  const now = useNow(everyMs);
  return <>{ago(since, now)}</>;
}
