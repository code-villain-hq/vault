import { useEffect, useMemo } from "react";
import { create } from "zustand";
import { isEpicType, typeMap } from "../components/IssueType";
import { useStore } from "../store";
import { api } from "./api";
import { epicsOf, type EpicOf } from "./grouping";

/**
 * Tickets asked of Jira for their epic, because they were not on your own
 * list (TASK-17, REV-11). Kept for the session: a ticket rarely changes epic,
 * and each list that groups by epic would otherwise ask again on every draw.
 */
const useLookedUp = create<{ byKey: Record<string, EpicOf | null>; asked: Set<string> }>(() => ({
  byKey: {},
  asked: new Set(),
}));

async function lookUp(keys: string[]) {
  const asked = useLookedUp.getState().asked;
  const fresh = keys.filter((k) => !asked.has(k));
  if (fresh.length === 0) return;
  for (const k of fresh) asked.add(k);
  try {
    const issues = await api.ticketsByKey(fresh);
    const types = typeMap(useStore.getState().issueTypes);
    const found = epicsOf(issues, (t) => isEpicType(types, t));
    // A key Jira did not return has no epic it can tell of.
    const byKey = Object.fromEntries(fresh.map((k) => [k, found[k] ?? null]));
    useLookedUp.setState((s) => ({ byKey: { ...s.byKey, ...byKey } }));
  } catch {
    // Grouping is a convenience: a failed lookup leaves these under "No
    // epic" and lets the next draw that needs them ask again.
    for (const k of fresh) asked.delete(k);
  }
}

/**
 * The epic of each of `keys`: from your own tickets where they are there,
 * else asked of Jira once. Asks nothing while `on` is false, so a list not
 * grouped by epic costs no request.
 */
export function useEpics(keys: string[], on: boolean): Record<string, EpicOf | null> {
  const issues = useStore((s) => s.issues);
  const issueTypes = useStore((s) => s.issueTypes);
  const connected = useStore((s) => s.settings?.tracker != null);
  const looked = useLookedUp((s) => s.byKey);
  const own = useMemo(() => {
    const types = typeMap(issueTypes);
    return epicsOf(issues, (t) => isEpicType(types, t));
  }, [issues, issueTypes]);

  const missing = on && connected ? keys.filter((k) => !(k in own) && !(k in looked)) : [];
  const ask = missing.join(",");
  useEffect(() => {
    if (ask) void lookUp(ask.split(","));
  }, [ask]);

  return useMemo(() => ({ ...looked, ...own }), [looked, own]);
}
