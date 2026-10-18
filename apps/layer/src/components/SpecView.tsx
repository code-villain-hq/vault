import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { approveWarning, questions } from "../lib/questions";
import { approvedText, editorText, partView, slotKey, useSpecs, type SpecFile } from "../lib/specs";
import { useStore } from "../store";
import type { SpecKind, SpecPart, TaskSpec, TaskView } from "../lib/types";
import { Confirm, Spinner } from "./ui";
import { SpecBanners } from "./spec/SpecBanners";
import { SpecQuestions } from "./spec/SpecQuestions";
import { SpecSide } from "./spec/SpecSide";
import { SpecStartAgent } from "./spec/SpecStartAgent";

const PARTS: SpecPart[] = ["requirements", "design", "tasks"];

/** What "Write them yourself" starts from: the headings a draft has (SPEC-1). */
const TEMPLATE: Record<SpecKind, string> = {
  feature: "## Goal\n\n## Requirements\n\n- R-1: WHEN … THE SYSTEM SHALL …\n\n## Out of scope\n\n-\n\n## Open questions\n\nNone.\n",
  bugfix: "## Current behaviour\n\n## Expected behaviour\n\n- R-1: WHEN … THE SYSTEM SHALL …\n\n## Unchanged behaviour\n\n- R-2: WHEN … THE SYSTEM SHALL CONTINUE TO …\n\n## Open questions\n\nNone.\n",
};

export function partName(part: SpecPart, kind: SpecKind): string {
  if (part === "requirements") return kind === "bugfix" ? "Bug analysis" : "Requirements";
  return part === "design" ? "Design" : "Tasks";
}

/** Every file of the spec for one part: the task's one, or every repository's tasks. */
export function filesOf(spec: TaskSpec, part: SpecPart): SpecFile[] {
  return part === "tasks" ? spec.repos.map((r) => ({ part, checkoutId: r.checkout_id })) : [{ part, checkoutId: null }];
}

/**
 * A task's spec (§19): one set of requirements and one design for the whole
 * task, and each repository's tasks, each drafted, edited here and approved
 * in turn. Approving writes the file into every repository and commits it
 * on the task's branch there, so it goes to review with the code and stays
 * in the repository after the task is gone.
 */
export function SpecView({ task }: { task: TaskView }) {
  const state = useSpecs((s) => s.byTask[task.id]);
  const load = useSpecs((s) => s.load);
  const edit = useSpecs((s) => s.edit);
  const draft = useSpecs((s) => s.draft);
  const approve = useSpecs((s) => s.approve);
  const pending = useSpecs((s) => s.pending);
  const setPending = useSpecs((s) => s.setPending);
  const running = useStore((s) => s.panes.filter((p) => p.task_id === task.id && p.running && p.kind === "agent").length);
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);

  const [checkoutId, setCheckoutId] = useState<string | null>(null);
  const [part, setPart] = useState<SpecPart | null>(null);
  const [kind, setKind] = useState<SpecKind>("feature");
  const [agentId, setAgentId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [replacing, setReplacing] = useState(false);
  /** Requirements about to be approved with questions still in them (SPEC-21). */
  const [asking, setAsking] = useState<{ files: SpecFile[]; warning: string } | null>(null);
  /** What was just approved while agents were running, to tell them (SPEC-14). */
  const [tell, setTell] = useState<SpecPart[] | null>(null);

  const loaded = state?.loaded ?? false;
  useEffect(() => {
    if (!loaded) void load(task.id).catch(fail);
  }, [task.id, loaded, load, fail]);
  // Feature or bug, as the spec already says once there is one.
  const savedKind = state?.spec?.kind;
  useEffect(() => {
    if (savedKind) setKind(savedKind);
  }, [task.id, savedKind]);

  const spec = state?.spec ?? null;
  const repos = spec?.repos ?? [];
  const repo = repos.find((r) => r.checkout_id === checkoutId) ?? repos[0];
  const edits = state?.edits ?? {};
  const started = (spec?.parts.some((p) => p.approved) ?? false) || Object.keys(edits).length > 0;
  const drafting = state?.drafting ?? null;

  // Start work's choice (SPEC-18): the agent to start later, and the
  // requirements drafted now, unless there is a spec already.
  useEffect(() => {
    if (!pending || pending.taskId !== task.id || !loaded) return;
    setPending(null);
    setAgentId(pending.agentId);
    if (!started && !drafting) void draft(task.id, "requirements", null).catch(fail);
  }, [pending, task.id, loaded, started, drafting, setPending, draft, fail]);

  if (!loaded || !spec) return <div className="empty"><Spinner /></div>;
  if (!repo) {
    return (
      <div className="empty">
        <h2>No repositories</h2>
        <p>A spec is kept in each repository the task works in. Add one to the task first.</p>
      </div>
    );
  }

  if (!started && !drafting) {
    return (
      <SpecStart
        kind={kind}
        setKind={setKind}
        many={repos.length > 1}
        from={task.issue_key ? "the ticket" : "the task's name"}
        legacy={spec.legacy}
        busy={busy}
        onDraft={() => void draft(task.id, "requirements", kind).catch(fail)}
        onQuick={() => void quick()}
        onWrite={(text) => edit(task.id, { part: "requirements", checkoutId: null }, text)}
      />
    );
  }

  const current = part ?? firstOpen(spec);
  // The file in the editor: the task's, or the picked repository's tasks.
  const file: SpecFile = { part: current, checkoutId: current === "tasks" ? repo.checkout_id : null };
  const view = partView(spec, file.part, file.checkoutId)!;
  const text = editorText(state, file.part, file.checkoutId);
  const dirty = edits[slotKey(file.part, file.checkoutId)] !== undefined;
  const all = filesOf(spec, current);
  const views = all.map((f) => partView(spec, f.part, f.checkoutId)!);
  const stale = views.some((v) => v.stale);
  const before = PARTS[PARTS.indexOf(current) - 1];
  const ready = !before || filesOf(spec, before).every((f) => !!approvedText(spec, f.part, f.checkoutId) || edits[slotKey(f.part, f.checkoutId)] !== undefined);
  const isDrafting = drafting?.part === current || drafting?.part === "merge";
  // Every file with something to approve: an edit, or out of date with text.
  const pendingFiles = spec.parts
    .filter((p) => edits[slotKey(p.part, p.checkout_id)] !== undefined)
    .map((p) => ({ part: p.part, checkoutId: p.checkout_id }));
  const toApprove = all.filter((f) => edits[slotKey(f.part, f.checkoutId)] !== undefined || (partView(spec, f.part, f.checkoutId)?.stale && editorText(state, f.part, f.checkoutId).trim()));
  const canApprove = !drafting && !busy && toApprove.length > 0;

  async function quick() {
    setBusy(true);
    try {
      await draft(task.id, "requirements", kind);
      await draft(task.id, "design", null);
      await draft(task.id, "tasks", null);
      setPart("requirements");
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  /** Redraft every file that is out of date, in order, each from the one before (SPEC-7). */
  async function sync() {
    if (!spec) return;
    setBusy(true);
    try {
      const design = partView(spec, "design", null);
      if (design?.stale) await draft(task.id, "design", null);
      if (design?.stale || spec.parts.some((p) => p.part === "tasks" && p.stale)) await draft(task.id, "tasks", null);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function approveFiles(files: SpecFile[]) {
    setBusy(true);
    try {
      const failed = await approve(task.id, files, files.some((f) => f.part === "requirements") ? kind : null);
      // The task's files go into every repository; tasks into their own.
      const reached = files.some((f) => f.part !== "tasks") ? repos : repos.filter((r) => files.some((f) => f.checkoutId === r.checkout_id));
      const inRepo = reached.filter((r) => r.in_repo && !failed.some((f) => f.folder === r.folder));
      if (failed.length > 0) {
        toast("error", `Not written in ${failed.map((f) => `${f.folder} (${f.error})`).join(", ")}. The others have it.`);
      } else {
        toast("success", inRepo.length > 0 ? `Approved and committed in ${inRepo.map((r) => r.folder).join(", ")}.` : "Approved.");
      }
      if (running > 0) setTell([...new Set(files.map((f) => f.part))]);
      const parts = files.map((f) => f.part);
      const next = PARTS[PARTS.indexOf(parts[parts.length - 1]) + 1];
      if (next && new Set(parts).size === 1) setPart(next);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  /** Approve, after saying what questions left in the requirements put at risk (SPEC-21). */
  function tryApprove(files: SpecFile[]) {
    const warning = files.some((f) => f.part === "requirements") ? approveWarning(questions(editorText(state, "requirements", null))) : null;
    if (warning) setAsking({ files, warning });
    else void approveFiles(files);
  }

  function redraft() {
    void draft(task.id, current, null).catch(fail);
  }

  const many = repos.length > 1;
  const status = isDrafting
    ? drafting?.part === "merge" ? "Merging the specs from before…" : current === "tasks" && many ? "Drafting every repository's tasks…" : "Drafting…"
    : drafting
      ? `Drafting the ${drafting.part}…`
      : stale
        ? `Out of date: the ${current === "tasks" ? "design or requirements" : "requirements"} changed since. Sync redrafts it.`
        : view.proposal
          ? `Proposed by ${view.proposal.by}: not approved yet`
          : dirty
            ? view.approved ? "Changed since approved: agents still work to what was approved" : "Not approved: no agent sees it yet"
            : view.approved
              ? repos.some((r) => r.in_repo) ? `Approved and committed${many && current !== "tasks" ? " in every repository" : ""}` : "Approved"
              : "Not started";
  const draftLabel = stale ? "Sync" : text.trim() ? "Redraft" : "Draft";
  const draftTitle = current === "requirements"
    ? `From ${task.issue_key ? "the ticket" : "the task's name"}, for the whole task`
    : current === "design" ? `From the requirements, reading ${many ? "every repository's" : `${repo.folder}'s`} code` : `From the requirements and the design${many ? ", for every repository at once" : ""}`;

  return (
    <div className="spec">
      {/* One primary on the screen: approving what is waiting comes before
          starting an agent on it. */}
      <SpecStartAgent task={task} spec={spec} agentId={agentId} setAgentId={setAgentId} quiet={canApprove} />
      <div className="spec-steps">
        {PARTS.map((p, i) => {
          const vs = filesOf(spec, p).map((f) => partView(spec, f.part, f.checkoutId)!);
          const has = vs.some((v) => edits[slotKey(v.part, v.checkout_id)] !== undefined);
          const approved = vs.every((v) => v.approved);
          const label = vs.some((v) => v.outside.length > 0) ? "changed outside" : vs.some((v) => v.stale) ? "out of date" : vs.some((v) => v.proposal) ? "proposal" : has ? (approved ? "changed" : "draft") : approved ? "approved" : vs.some((v) => v.approved) ? "partly approved" : "not started";
          const done = label === "approved";
          return (
            <button key={p} className={`spec-step${p === current ? " active" : ""}`} onClick={() => setPart(p)}>
              <span className={`spec-num ${done ? "done" : ""}`}>{done ? "✓" : i + 1}</span>
              <span>{partName(p, spec.kind)}</span>
              <span className={`spec-state ${label.replace(/ /g, "-")}`}>{label}</span>
            </button>
          );
        })}
        <span className="spacer" />
        <span className="muted spec-home" title={many ? "The same requirements and design in each; each repository's own tasks" : repo.in_repo ? "Committed on the task's branch" : "Specs are off for this repository (Repos): kept by the app, not in git"}>
          {many ? `in ${repos.map((r) => (r.in_repo ? r.home : `${r.folder} (kept by the app)`)).join(", ")}` : repo.in_repo ? repo.home : "kept by the app"}
        </span>
      </div>
      {current === "tasks" && many && (
        <div className="spec-repos">
          {repos.map((r) => {
            const v = partView(spec, "tasks", r.checkout_id);
            const has = edits[slotKey("tasks", r.checkout_id)] !== undefined;
            return (
              <button key={r.checkout_id} className={`spec-repo${r.checkout_id === repo.checkout_id ? " active" : ""}`} onClick={() => setCheckoutId(r.checkout_id)}>
                {r.folder}
                <span className="muted">
                  {r.steps.length > 0 ? `${r.steps.filter((s) => s.done).length}/${r.steps.length}` : "—"}
                  {v?.outside.length ? " · changed outside" : v?.stale ? " · out of date" : has ? " · draft" : ""}
                </span>
              </button>
            );
          })}
        </div>
      )}
      <SpecBanners task={task} spec={spec} file={file} busy={busy || !!drafting} onApprove={tryApprove} onShow={(t, cid) => edit(task.id, { part: file.part, checkoutId: file.part === "tasks" ? cid : null }, t)} />
      {tell && (
        <div className="spec-tell">
          {running} agent{running === 1 ? " is" : "s are"} working in this task to the spec as it was.
          <button className="ui-button" onClick={() => {
            void api.tellSpecChange(task.id, tell).then((n) => toast("success", `Told ${n} agent${n === 1 ? "" : "s"}.`)).catch(fail);
            setTell(null);
          }}>Tell them</button>
          <button className="ui-button" onClick={() => setTell(null)}>Not now</button>
        </div>
      )}
      <div className="spec-bar">
        <button
          className="ui-button"
          disabled={!!drafting || busy || !ready}
          title={ready ? (stale ? "Redraft every file that is out of date, in order" : draftTitle) : `Approve the ${before} first`}
          onClick={() => (stale ? void sync() : text.trim() ? setReplacing(true) : redraft())}
        >
          {draftLabel} {current === "requirements" ? "" : current}
        </button>
        {(isDrafting || busy) && <Spinner />}
        <span className="muted">{status}</span>
        <span className="spacer" />
        {dirty && !view.proposal && (
          <button
            className="ui-button"
            disabled={!!drafting || busy}
            onClick={() => edit(task.id, file, approvedText(spec, file.part, file.checkoutId))}
          >
            {view.approved ? "Discard changes" : "Discard draft"}
          </button>
        )}
        {pendingFiles.length > 1 && (
          <button className="ui-button" disabled={!!drafting || busy} title="Approve every file drafted or changed, together" onClick={() => tryApprove(pendingFiles)}>
            Approve all {pendingFiles.length}
          </button>
        )}
        <button
          className={`ui-button${canApprove ? " ui-button-primary" : ""}`}
          disabled={!canApprove}
          title={repos.some((r) => r.in_repo) ? `Write it ${current === "tasks" || !many ? "to its repository" : "to every repository"} and commit it on the task's branch` : "Keep it as the approved spec"}
          onClick={() => tryApprove(toApprove)}
        >
          Approve {partName(current, spec.kind).toLowerCase()}{current === "tasks" && toApprove.length > 1 ? ` of ${toApprove.length} repositories` : ""}
        </button>
      </div>
      <div className="spec-body">
        <textarea
          className="ui-field spec-editor"
          value={text}
          readOnly={isDrafting}
          spellCheck={false}
          placeholder={current === "tasks" ? "## Tasks\n\n- [ ] 1. … (R-1)" : "## …"}
          onChange={(e) => edit(task.id, file, e.target.value)}
        />
        <SpecSide task={task} spec={spec} current={current} repo={repo}>
          {current === "requirements" && <SpecQuestions task={task} />}
        </SpecSide>
      </div>
      {replacing && (
        <Confirm
          title="Replace what the editor holds?"
          body={current === "tasks" && many
            ? "A new draft replaces every repository's tasks in the editor. What is approved stays until you approve again."
            : "A new draft replaces what is in the editor. What is approved stays until you approve again."}
          confirmLabel="Redraft"
          danger={false}
          onConfirm={() => { setReplacing(false); redraft(); }}
          onCancel={() => setReplacing(false)}
        />
      )}
      {asking && (
        <Confirm
          title="Approve with open questions?"
          body={asking.warning}
          confirmLabel="Approve anyway"
          danger={false}
          onConfirm={() => { const { files } = asking; setAsking(null); void approveFiles(files); }}
          onCancel={() => setAsking(null)}
        />
      )}
    </div>
  );
}

/** The first part with a file not yet approved, or out of date, or the tasks. */
function firstOpen(spec: TaskSpec): SpecPart {
  return spec.parts.find((p) => !p.approved || p.stale || p.outside.length > 0 || p.proposal)?.part ?? "tasks";
}

/** Before any spec: what it is, a bug or a feature, and how to start one. */
function SpecStart({ kind, setKind, many, from, legacy, busy, onDraft, onQuick, onWrite }: {
  kind: SpecKind;
  setKind: (k: SpecKind) => void;
  many: boolean;
  from: string;
  legacy: string | null;
  busy: boolean;
  onDraft: () => void;
  onQuick: () => void;
  onWrite: (text: string) => void;
}) {
  return (
    <div className="empty spec-start">
      <h2>No spec yet</h2>
      <p>
        A spec is reviewed in three steps: the requirements (what done means), the design (how), and
        the tasks an agent works through. {many
          ? "The requirements and the design are one for the whole task, so the repositories cannot drift apart; each repository gets its own tasks. "
          : ""}
        Approving one commits it on the task's branch{many ? " in every repository" : ""}, so it goes to
        review with the code and stays in the repository.
      </p>
      <div className="row spec-kind">
        <label><input type="radio" checked={kind === "feature"} onChange={() => setKind("feature")} /> A feature</label>
        <label><input type="radio" checked={kind === "bugfix"} onChange={() => setKind("bugfix")} /> A bug</label>
      </div>
      <div className="row">
        <button className="ui-button ui-button-primary" disabled={busy} onClick={onDraft}>Draft the {kind === "bugfix" ? "bug analysis" : "requirements"} from {from}</button>
        <button className="ui-button" disabled={busy} onClick={onQuick} title="Requirements, design and tasks in one go, to approve together">Quick spec</button>
        <button className="ui-button" disabled={busy} onClick={() => onWrite(TEMPLATE[kind])}>Write it yourself</button>
      </div>
      {legacy && (
        <p className="muted">
          This task has a spec from before, kept beside the task.{" "}
          <button className="ui-button" onClick={() => onWrite(legacy)}>Start from it</button>
        </p>
      )}
    </div>
  );
}
