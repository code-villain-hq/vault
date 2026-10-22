import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../lib/api";
import { ErrorMessage, Spinner, useFailure } from "../ui";

/**
 * One-shot polish for a Jira description, same path as PR drafting: a cheap
 * model, streamed into the textarea. Works from create dialogs that have no
 * task or agent pane yet.
 */
export function OptimizeDescription({
  summary,
  description,
  kind,
  onChange,
  disabled,
}: {
  summary: string;
  description: string;
  kind: "epic" | "ticket";
  onChange: (next: string) => void;
  disabled?: boolean;
}) {
  // Fails under its own button, inside whichever dialog holds it (NOTE-6).
  const failure = useFailure();
  const [busy, setBusy] = useState(false);
  const requestId = useRef("");
  // Accumulator for streamed deltas, so a late chunk cannot race a keystroke
  // that updated the parent's description between events.
  const streamed = useRef("");
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    const p = listen<{ request_id: string; text: string }>("issue:draft", (e) => {
      if (e.payload.request_id !== requestId.current) return;
      streamed.current += e.payload.text;
      onChangeRef.current(streamed.current);
    });
    return () => { void p.then((un) => un()); };
  }, []);

  async function optimize() {
    if (busy) return;
    const prior = description;
    const nextId = `issue-${crypto.randomUUID()}`;
    requestId.current = nextId;
    streamed.current = "";
    setBusy(true);
    failure.clear();
    // Cleared so streamed text is not appended to the old draft.
    onChange("");
    try {
      const text = await api.optimizeIssueDescription({
        requestId: nextId,
        summary,
        description: prior,
        kind,
      });
      onChange(text.trim());
    } catch (e) {
      onChange(prior);
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  const canRun = summary.trim().length > 0 || description.trim().length > 0;

  return (
    <>
      <div className="row" style={{ marginTop: 8 }}>
        <button
          className="ui-button"
          disabled={disabled || busy || !canRun}
          title={
            description.trim()
              ? "Tighten this description for engineers and coding agents"
              : "Draft a description from the summary"
          }
          onClick={() => void optimize()}
        >
          {busy
            ? "Optimizing…"
            : description.trim()
              ? "✨ Optimize description"
              : "✨ Draft description"}
        </button>
        {busy && <Spinner />}
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
    </>
  );
}
