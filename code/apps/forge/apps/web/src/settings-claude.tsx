import {
  answersHere,
  type ClaudeCodeInfo,
  claudeHere,
  messageOf,
  setAnswersHere,
} from "@code-villain/web";
import { useCallback, useEffect, useState } from "react";
import { PageHeader } from "./layout";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/**
 * Settings → Claude, in the desktop app: Claude Code on this Mac runs what
 * you ask Forge AI, wherever the workspace is (`studio/design/behaviour.md`,
 * "Claude"). On a server's workspace you may turn it off, and the server's
 * key runs it instead. Yours, on this Mac, not the workspace's.
 */
export function ClaudeSettingsPage() {
  const { place } = useWorkspace();
  const onThisMac = place.kind === "mac";
  const [on, setOn] = useState(answersHere);
  const [code, setCode] = useState<ClaudeCodeInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const runsHere = onThisMac || on;

  const check = useCallback(async () => {
    setProblem(null);
    setChecking(true);
    try {
      setCode(await claudeHere(place));
    } catch (e) {
      setProblem(messageOf(e));
    }
    setChecking(false);
  }, [place]);
  useEffect(() => {
    if (runsHere) void check();
  }, [runsHere, check]);

  const turn = (value: boolean) => {
    setAnswersHere(value);
    setOn(value);
  };

  return (
    <>
      <PageHeader title="Claude" />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-8 px-6 py-8">
          <section className="flex flex-col gap-3">
            <div className="flex items-center gap-3">
              <h2 className="min-w-0 flex-1 font-medium">
                {onThisMac ? "Claude Code on this Mac" : "Answer with Claude Code on this Mac"}
              </h2>
              {!onThisMac && (
                <fieldset className="ui-segmented" aria-label="Answer with Claude Code on this Mac">
                  {(
                    [
                      [true, "On"],
                      [false, "Off"],
                    ] as const
                  ).map(([value, label]) => (
                    <button
                      key={label}
                      type="button"
                      className="ui-segment"
                      aria-pressed={on === value}
                      onClick={() => on !== value && turn(value)}
                    >
                      {label}
                    </button>
                  ))}
                </fieldset>
              )}
            </div>
            <p className="text-muted">
              {onThisMac
                ? "Checks and triage you ask Forge AI for run with Claude Code here, as you're signed in to it, with only the run's tools. The workspaces on this Mac have no key."
                : on
                  ? "Checks and triage you ask Forge AI for run with Claude Code here, as you're signed in to it, with only the run's tools on the server. The server's key runs them only when Claude Code can't. This is yours, on this Mac."
                  : "The server's key runs what you ask Forge AI, as it does in the browser. This is yours, on this Mac."}
            </p>
            {!onThisMac && (
              <p className="text-muted">
                Scheduled checks, checks after a push and triage of new issues stay with the
                server's key.
              </p>
            )}
            {runsHere && code && <Status code={code} />}
            {runsHere && !code && !problem && (
              <p className="text-muted">Looking for Claude Code on this Mac…</p>
            )}
            <ErrorMessage>{problem}</ErrorMessage>
            {runsHere && (code || problem) && (
              <div>
                <Button variant="secondary" disabled={checking} onClick={() => void check()}>
                  {checking ? "Checking…" : "Check again"}
                </Button>
              </div>
            )}
          </section>
        </div>
      </div>
    </>
  );
}

/** Whether Claude Code can run here, and what to do when it can't. */
function Status({ code }: { code: ClaudeCodeInfo }) {
  if (code.state === "ready") {
    return (
      <p>
        Claude Code is ready
        {code.version && <span className="text-muted"> (ACP adapter {code.version})</span>}.
      </p>
    );
  }
  if (code.state === "not_installed") {
    return (
      <div className="flex flex-col gap-2">
        <p>
          Forge reaches Claude Code through its ACP adapter, which isn't installed. Install it in
          Terminal, then check again.
        </p>
        <code className="block break-all rounded-md bg-panel p-3 font-mono text-xs">
          {code.install}
        </code>
      </div>
    );
  }
  if (code.state === "signed_out") {
    return <p>Claude Code isn't signed in. Run claude in Terminal and sign in there.</p>;
  }
  return <p>{code.problem ?? "Claude Code didn't answer."}</p>;
}
