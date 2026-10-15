import { useRef, useState, type ReactNode } from "react";
import { dragGrip } from "../../lib/drag";
import { read, write } from "../../lib/persist";
import { useSpecs } from "../../lib/specs";
import { useStore } from "../../store";
import type { RepoSpec, SpecPart, TaskSpec, TaskView } from "../../lib/types";
import { Spinner } from "../ui";

/** The side's usual width, its narrowest, and what the editor keeps beside it. */
const SIDE_DEFAULT = 330;
const SIDE_MIN = 260;
const EDITOR_MIN = 280;

const VERDICT_MARK: Record<string, string> = { met: "✓", "not met": "✗", unclear: "?" };

/**
 * Beside the editor: what the file being edited needs first (`children`:
 * the open questions of the requirements), the approved requirements with
 * the repositories each names and each one's last check (SPEC-15), and how
 * far each repository's tasks are (SPEC-8): on the tasks, the picked one's
 * steps.
 */
export function SpecSide({ task, spec, current, repo, children }: {
  task: TaskView;
  spec: TaskSpec;
  current: SpecPart;
  repo: RepoSpec;
  children?: ReactNode;
}) {
  const checking = useSpecs((s) => s.byTask[task.id]?.checking ?? null);
  const check = useSpecs((s) => s.check);
  const fail = useStore((s) => s.fail);
  const [width, setWidth] = useState(() => {
    const kept = read<unknown>("specSideWidth", SIDE_DEFAULT);
    return typeof kept === "number" && Number.isFinite(kept) ? Math.max(SIDE_MIN, kept) : SIDE_DEFAULT;
  });
  const sideRef = useRef<HTMLElement>(null);

  const many = spec.repos.length > 1;
  const approvedReqs = !!spec.parts.find((p) => p.part === "requirements")?.approved;
  const verdicts = (id: string) =>
    spec.repos.flatMap((r) => {
      const v = r.check?.results.find((x) => x.id === id);
      return v ? [{ folder: r.folder, ...v }] : [];
    });

  return (
    <>
      <div
        className="diff-grip spec-grip"
        title="Drag to resize; double-click for the usual width"
        onMouseDown={(e) => {
          // Not onDoubleClick: the drag the first press starts gives the
          // release to `body`, so the dblclick event never reaches the grip.
          if (e.detail === 2) {
            e.preventDefault();
            setWidth(SIDE_DEFAULT);
            write("specSideWidth", SIDE_DEFAULT);
            return;
          }
          const room = e.currentTarget.parentElement?.clientWidth ?? 0;
          const most = Math.max(SIDE_MIN, room - EDITOR_MIN);
          const startX = e.clientX;
          // The mouse moves in the window's pixels, the side in the UI's,
          // which `.shell`'s zoom makes larger.
          const scale = useStore.getState().settings?.ui.scale ?? 1;
          // A width kept from a wider window is shown capped (the CSS's max-width).
          const from = Math.min(width, most);
          let next = from;
          // Only the side's style changes while dragging; the width is kept,
          // and remembered, on the drop.
          dragGrip(e, (m) => {
            next = Math.round(Math.min(most, Math.max(SIDE_MIN, from + (startX - m.clientX) / scale)));
            if (sideRef.current) sideRef.current.style.width = `${next}px`;
          }, () => { setWidth(next); write("specSideWidth", next); });
        }}
      />
      <aside className="spec-side" ref={sideRef} style={{ width }}>
        {children}
        <section>
          <h3>{spec.kind === "bugfix" ? "Expected and unchanged" : "Requirements"}</h3>
          {spec.requirements.length === 0 && approvedReqs ? (
            <p className="spec-stale">
              No requirements found in what was approved. List them under a Requirements heading,
              each starting R-1, R-2: they are what done means.
            </p>
          ) : spec.requirements.length === 0 ? (
            <p className="muted">
              None approved yet. They are what done means: each one a WHEN … THE SYSTEM SHALL … under a
              Requirements heading{many ? ", naming the repositories that meet it: R-1 [api, web]" : ""}.
            </p>
          ) : (
            <ol className="spec-reqs">
              {spec.requirements.map((r) => {
                const vs = verdicts(r.id);
                return (
                  <li key={r.id} title={vs.map((v) => `${many ? `${v.folder}: ` : ""}${v.evidence}`).filter(Boolean).join("\n") || undefined}>
                    <b>{r.id}</b>
                    {many && <span className="spec-tag">{r.repos.length > 0 ? r.repos.join(", ") : "every repository"}</span>}
                    {vs.map((v) => (
                      <span key={v.folder} className={`spec-verdict ${v.verdict.replace(" ", "-")}`}>
                        {VERDICT_MARK[v.verdict]} {many ? v.folder : v.verdict}
                      </span>
                    ))}
                    <span>{r.text}</span>
                  </li>
                );
              })}
            </ol>
          )}
          <div className="spec-checks">
            {spec.repos.map((r) => {
              const busy = checking?.includes(r.checkout_id) ?? false;
              const named = spec.requirements.some((q) => q.repos.length === 0 || q.repos.some((f) => f.toLowerCase() === r.folder.toLowerCase()));
              return (
                <div key={r.checkout_id} className="row spec-check">
                  <button
                    className="btn btn-sm"
                    disabled={!named || busy}
                    title={named ? `Read ${r.folder}'s changes on the branch against the requirements that name it` : `No requirement names ${r.folder}`}
                    onClick={() => void check(task.id, r.checkout_id).catch(fail)}
                  >
                    Check {many ? r.folder : "against spec"}
                  </button>
                  {busy && <Spinner />}
                  {r.check && !busy && (
                    <span className={`muted${r.check.stale ? " spec-stale" : ""}`}>
                      {r.check.stale ? "Checked before the latest commits" : `Checked at ${r.check.sha.slice(0, 7)}`}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        </section>

        <section>
          {current === "tasks" || !many ? (
            <>
              <h3>
                Tasks{many ? ` of ${repo.folder}` : ""}
                {repo.steps.length > 0 && <span className="muted"> · {repo.steps.filter((s) => s.done).length} of {repo.steps.length} done</span>}
              </h3>
              {repo.steps.length === 0 ? (
                <p className="muted">None approved yet.</p>
              ) : (
                <ul className="spec-tasks">
                  {repo.steps.map((s) => (
                    <li key={s.number} className={s.done ? "done" : ""}>
                      <span className="mark">{s.done ? "✓" : "○"}</span>
                      <span>{s.number}. {s.text}</span>
                    </li>
                  ))}
                </ul>
              )}
            </>
          ) : (
            <>
              <h3>Tasks</h3>
              <ul className="spec-tasks">
                {spec.repos.map((r) => (
                  <li key={r.checkout_id} className={r.steps.length > 0 && r.steps.every((s) => s.done) ? "done" : ""}>
                    <span className="mark">{r.steps.length > 0 && r.steps.every((s) => s.done) ? "✓" : "○"}</span>
                    <span>{r.folder}: {r.steps.length === 0 ? "none approved yet" : `${r.steps.filter((s) => s.done).length} of ${r.steps.length} done`}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>
      </aside>
    </>
  );
}
