import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { AcpStanding, AcpVersion } from "../lib/types";
import { BusyLabel, ErrorMessage, Field, useFailure } from "./ui";

/** What each standing says, and its pill (ACP-17). */
const SAYS: Record<AcpStanding, [string, string]> = {
  supported: ["Supported", "ui-status-ok"],
  older: ["Too old", "ui-status-danger"],
  newer: ["Not checked yet", "ui-status-warn"],
  unknown: ["Version unknown", "ui-status-warn"],
  missing: ["Not installed", ""],
};

/** What to do about it, under the agent's name. */
function advice(a: AcpVersion): string {
  switch (a.standing) {
    case "supported":
      return `Checked with ${a.checked}.`;
    case "older":
      return `Checked with ${a.checked}. Update it before running it as a conversation.`;
    case "newer":
      return `Checked with ${a.checked}. A new major release: it may work, or go wrong in ways Layer can't see.`;
    case "unknown":
      return `${a.command} --version said no version. Checked with ${a.checked}.`;
    case "missing":
      return `Checked with ${a.checked}. Install it to run it as a conversation.`;
  }
}

/** Settings → ACP: each agent's ACP command here, against the version checked (ACP-17). */
export function AcpSettings() {
  const [agents, setAgents] = useState<AcpVersion[] | null>(null);
  const [busy, setBusy] = useState(false);
  const failure = useFailure();

  async function check() {
    setBusy(true);
    failure.clear();
    try {
      setAgents(await api.acpVersions());
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void check();
    // Once, as the tab opens; "Check again" after an update.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Field
      label="Agents as conversations"
      hint={
        <>
          An agent runs as a conversation over the Agent Client Protocol. Its ACP mode changes
          between releases, and an old or untried one can go wrong in ways Layer can't see. Each is
          supported from the version it was checked with, up to its next major release.
        </>
      }
    >
      <div className="col">
        {agents === null && !failure.error && <span className="muted">Reading versions…</span>}
        {agents?.map((a) => {
          const [label, pill] = SAYS[a.standing];
          return (
            <div key={a.id} className="row acp-version">
              <div className="col acp-version-name">
                <span>
                  {a.name} <code>{a.command}</code>
                </span>
                <span className="muted">{advice(a)}</span>
              </div>
              <div className="spacer" />
              {a.version && <code>{a.version}</code>}
              <span className={`ui-status ${pill}`}>{label}</span>
            </div>
          );
        })}
        <div className="row">
          <button className="ui-button" disabled={busy} onClick={() => void check()}>
            <BusyLabel busy={busy} idle="Check again" working="Checking…" />
          </button>
        </div>
        <ErrorMessage>{failure.error}</ErrorMessage>
      </div>
    </Field>
  );
}
