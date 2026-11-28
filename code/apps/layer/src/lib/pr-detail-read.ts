import { useEffect, useMemo, useState } from "react";
import { failureText } from "./api";

export type PrDetailRead<T> =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "failed"; message: string }
  | { status: "ready"; data: T };

/** A card owns one read; an identity change or explicit refresh supersedes it. */
export function usePrDetailRead<T>({
  enabled,
  identity,
  refresh,
  load,
}: {
  enabled: boolean;
  identity: string;
  refresh: number;
  load: () => Promise<T>;
}): PrDetailRead<T> {
  const request = useMemo(
    () => (enabled ? { identity, refresh, load } : null),
    [enabled, identity, refresh, load],
  );
  const [answer, setAnswer] = useState<{
    request: NonNullable<typeof request>;
    read: PrDetailRead<T>;
  } | null>(null);

  useEffect(() => {
    if (!request) return;
    let current = true;
    void (async () => {
      try {
        const data = await request.load();
        if (current) setAnswer({ request, read: { status: "ready", data } });
      } catch (e) {
        if (current)
          setAnswer({ request, read: { status: "failed", message: failureText(e ?? "") } });
      }
    })();
    return () => {
      current = false;
    };
  }, [request]);

  // The render can see a new disclosure or identity before the effect
  // cleans up the previous one. Never paint that old request's data.
  if (!request) return { status: "idle" };
  return answer?.request === request ? answer.read : { status: "loading" };
}
