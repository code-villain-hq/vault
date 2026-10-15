import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useStore, type ReviewNote } from "../store";
import type { RepoOutgoing } from "../lib/types";
import { Spinner } from "./ui";

const NO_NOTES: ReviewNote[] = [];
const KIND_TONE = { debug: "del", "focused test": "del", "conflict marker": "del", todo: "warn" } as const;

/**
 * What Open pull requests is about to push, looked at first (PR-11): per
 * repo the commits going out and the leftovers in their added lines, notes
 * on the Diff tab not yet sent, and spec files changed outside the app,
 * which a reviewer would read as agreed (SPEC-23). Advice, never a gate:
 * `onFlags` says how many things deserve a second look, and the dialog's
 * button says "anyway" when there are some.
 */
export function Outgoing({ taskId, onFlags }: { taskId: string; onFlags: (n: number) => void }) {
  const fail = useStore((s) => s.fail);
  const notes = useStore((s) => s.notes[taskId] ?? NO_NOTES);
  const [rows, setRows] = useState<RepoOutgoing[] | null>(null);
  const [specOutside, setSpecOutside] = useState<string[]>([]);

  useEffect(() => {
    let current = true;
    api.taskOutgoing(taskId)
      .then((r) => { if (current) setRows(r); })
      .catch((e) => { if (current) { setRows([]); fail(e); } });
    api.readSpec(taskId)
      .then((s) => { if (current) setSpecOutside(s.parts.flatMap((p) => p.outside.map((o) => `${o.folder}/…/${p.file}`))); })
      .catch(() => {});
    return () => { current = false; };
  }, [taskId, fail]);

  const going = useMemo(() => (rows ?? []).filter((r) => r.commits.length > 0 || r.error), [rows]);
  const leftovers = going.reduce((n, r) => n + r.leftovers.length, 0);
  const unsent = notes.length;

  useEffect(() => { onFlags(leftovers + unsent + specOutside.length); }, [leftovers, unsent, specOutside.length, onFlags]);

  if (rows === null) {
    return <div className="outgoing muted"><Spinner /> Looking at what this would push…</div>;
  }
  if (going.length === 0 && specOutside.length === 0) return null;

  return (
    <div className="outgoing">
      <div className="outgoing-head">
        <b>Before it goes</b>
        {unsent > 0 && (
          <span className="muted">{unsent} note{unsent === 1 ? "" : "s"} on the Diff tab not sent to an agent</span>
        )}
      </div>
      {specOutside.length > 0 && (
        <div className="outgoing-repo del">
          The spec was changed outside the app, and nobody approved it: {specOutside.join(", ")}. Approve it
          or put it back on the Spec tab first, or the pull request carries it as if it were agreed.
        </div>
      )}
      {going.map((r) => (
        <div key={r.checkout_id} className="outgoing-repo">
          <div className="muted">
            <code>{r.repo}</code>{" "}
            {r.error
              ? <span className="del">could not be read: {r.error}</span>
              : `${r.commits.length}${r.commits.length >= 100 ? "+" : ""} commit${r.commits.length === 1 ? "" : "s"} going out: ` +
                r.commits.slice(0, 3).map((c) => c.subject).join("; ") +
                (r.commits.length > 3 ? "; …" : "")}
          </div>
          {r.leftovers.map((l, i) => (
            <div key={i} className="outgoing-leftover" title={l.text}>
              <span className={`chip ${KIND_TONE[l.kind]}`}>{l.kind}</span>
              <code>{l.path}:{l.line}</code>
              <span className="text">{l.text}</span>
            </div>
          ))}
          {r.leftovers_more && <div className="muted">and more not listed</div>}
        </div>
      ))}
    </div>
  );
}
