import { useState } from "react";
import type { TrackerKind } from "@layer/types";
import { api } from "../lib/api";
import { trackerName } from "../lib/tracker";
import { useStore } from "../store";
import { Confirm, ErrorMessage, Field, useFailure } from "./ui";
import { useWorkspace } from "./SharedSettings";

const TRACKERS: [TrackerKind, string][] = [["jira", "Jira"], ["github", "GitHub Issues"]];

/**
 * Where the workspace's tickets come from (TKT-15): the workspace's, so only
 * admins change it. Changing it first says how many tasks keep a ticket in
 * the tracker being left (TKT-16).
 */
export function TrackerChoice() {
  const chosen = useStore((s) => s.settings?.tracker_chosen) ?? null;
  const tasks = useStore((s) => s.tasks);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const refreshIssues = useStore((s) => s.refreshIssues);
  const workspace = useWorkspace();
  const admin = workspace?.admin !== false;
  const failure = useFailure();
  const [asking, setAsking] = useState<TrackerKind | null>(null);

  // A GitHub issue's key names its repository (`acme/web#12`); a Jira key never does.
  const keeping = chosen
    ? tasks.filter((t) => t.issue_key && t.issue_key.includes("#") === (chosen === "github")).length
    : 0;

  async function choose(uses: TrackerKind) {
    failure.clear();
    try {
      await api.setTracker(uses);
      await refreshSettings();
      void refreshIssues();
    } catch (e) {
      failure.fail(e);
    }
  }

  return (
    <Field label="Tickets come from" hint={admin ? undefined : `${workspace?.name ?? "The workspace"}'s choice: admins change it.`}>
      <div className="ui-segmented">
        {TRACKERS.map(([kind, label]) => (
          <button
            key={kind}
            className="ui-segment"
            aria-pressed={chosen === kind}
            disabled={!admin}
            onClick={() => {
              if (kind === chosen) return;
              if (keeping > 0) setAsking(kind);
              else void choose(kind);
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
      {asking && chosen && (
        <Confirm
          title={`Take tickets from ${trackerName(asking)}?`}
          body={
            <>
              {keeping} task{keeping === 1 ? " has a ticket" : "s have tickets"} in {trackerName(chosen)}.{" "}
              {keeping === 1 ? "It keeps it" : "They keep them"} as a link, but the app stops reading and moving{" "}
              {keeping === 1 ? "it" : "them"}.
            </>
          }
          confirmLabel={`Use ${trackerName(asking)}`}
          danger={false}
          onConfirm={() => choose(asking)}
          onCancel={() => setAsking(null)}
        />
      )}
    </Field>
  );
}
