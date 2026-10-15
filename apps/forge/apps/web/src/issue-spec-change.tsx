// An issue's spec change in the review bar, next to its pull request: what
// approving the work also changes in the specs (decision "Specs change only
// when the work is done"), so the PO judges sentences, not code.
import type { Issue, SpecChange, SpecDelta } from "@forge/types";
import { Link } from "@tanstack/react-router";
import { type ReactNode, useMemo } from "react";
import { byId, identifier, useStates } from "./data";
import { Markdown } from "./markdown";
import {
  type ChangedSince,
  changedAfterStart,
  changedSince,
  newSpecsTaken,
  type SpecChangePart,
  specChangeParts,
  specConflicts,
  specNameKey,
  statement,
} from "./spec-change";
import { TextDiff } from "./suggestions";
import { useRecords, useWorkspace } from "./workspace";

/** The spec changes of open issues (not done or canceled) that haven't taken effect. */
export function useOpenSpecChanges(): SpecChange[] {
  const changes = useRecords("SpecChange");
  const issues = byId(useRecords("Issue"));
  const states = useStates();
  return changes.filter((change) => {
    const issue = change.issue_id ? issues.get(change.issue_id) : undefined;
    const type = issue && states.get(issue.state_id)?.type;
    return !change.applied_at && type !== undefined && type !== "completed" && type !== "canceled";
  });
}

/**
 * Where the issue's spec change stands against the other open issues' and
 * the specs as they are now (decision "A spec change is written first, and
 * only one issue changes a requirement at a time"): what keeps it from
 * starting, and who else wants what it changes.
 */
function SpecChangeStanding({ issue }: { issue: Issue }) {
  const open = useOpenSpecChanges();
  const states = useStates();
  // Agents can't start it; once it's started (by a person, or before the
  // conflict arose), it still has to be settled.
  const started = states.get(issue.state_id)?.type === "started";
  const issues = byId(useRecords("Issue"));
  const specs = byId(useRecords("Spec"));
  const mine = open.find((c) => c.issue_id === issue.id);
  if (!mine) return null;
  const conflicts = specConflicts(open);
  const issueOf = (change: string) => {
    const id = open.find((c) => c.id === change)?.issue_id;
    return id ? issues.get(id) : undefined;
  };
  const what = (spec: string, requirement: string | null) => {
    const title = specs.get(spec)?.title ?? "a spec";
    return requirement
      ? `“${requirement}” in the ${title} spec`
      : `the ${title} spec (a rewrite changes all of it)`;
  };
  const first = conflicts.filter((c) => c.second === mine.id);
  const second = conflicts.filter((c) => c.first === mine.id);
  const outdated = mine.specs.flatMap((delta) => {
    const spec = delta.spec_id ? specs.get(delta.spec_id) : undefined;
    const changed = spec ? changedSince(delta, spec.body) : null;
    return spec && changed ? [{ id: spec.id, spec: spec.title, changed }] : [];
  });
  const taken = newSpecsTaken(open, mine);
  // A spec of that name came about meanwhile: the new one can't be created.
  const names = new Set([...specs.values()].map((spec) => specNameKey(spec.title)));
  const exists = mine.specs.flatMap((d) =>
    !d.spec_id && d.new_title && names.has(specNameKey(d.new_title)) ? [d.new_title.trim()] : [],
  );
  if (
    first.length === 0 &&
    second.length === 0 &&
    outdated.length === 0 &&
    taken.length === 0 &&
    exists.length === 0
  )
    return null;
  return (
    <div className="flex flex-col gap-2">
      {(first.length > 0 || outdated.length > 0 || taken.length > 0 || exists.length > 0) && (
        <div className="flex flex-col gap-1 rounded-md border border-line px-3 py-2 text-sm">
          <p className="font-medium text-warn">
            {started ? "Its spec change has to be settled" : "This issue can't start yet"}
          </p>
          <ul className="flex flex-col gap-1">
            {first.map((conflict) => (
              <li key={`${conflict.first}-${conflict.spec_id}-${conflict.requirement}`}>
                <OtherIssue issue={issueOf(conflict.first)} /> already changes{" "}
                {what(conflict.spec_id, conflict.requirement)}. Settle it together: wait until it's
                done, drop it from this spec change, combine the two issues, or split the
                requirement.
              </li>
            ))}
            {taken.map(({ change, title }) => (
              <li key={`${change}-${title}`}>
                <OtherIssue issue={issueOf(change)} /> already writes a new spec “{title}”. Settle
                it together: wait until it's done and then change that spec, or drop it from this
                spec change.
              </li>
            ))}
            {exists.map((title) => (
              <li key={title}>
                There's a spec called “{title}” now, so its new spec can't be created. It has to be
                written again as a change to that spec.
              </li>
            ))}
            {outdated.map(({ id, spec, changed }) => (
              <li key={id}>
                Its part for the {spec} spec was written against older text: {changedWords(changed)}{" "}
                changed since. It has to be written again before it can{" "}
                {started ? "take effect" : "start or take effect"}.
              </li>
            ))}
          </ul>
        </div>
      )}
      {second.map((conflict) => (
        <p
          key={`${conflict.second}-${conflict.spec_id}-${conflict.requirement}`}
          className="text-xs text-warn"
        >
          <OtherIssue issue={issueOf(conflict.second)} /> also wants to change{" "}
          {what(conflict.spec_id, conflict.requirement)}.
        </p>
      ))}
    </div>
  );
}

/**
 * A spec change that says something else than when work on its issue
 * started, opening to what it said then against what it says now.
 */
function ChangedAfterStart({ change }: { change: SpecChange }) {
  const specs = byId(useRecords("Spec"));
  const then = change.as_started;
  if (!then || !changedAfterStart(change)) return null;
  // A new spec has no id yet: its name is what saving it again goes by.
  const keyed = (parts: SpecDelta[]) =>
    parts.map((d) => ({ d, k: d.spec_id ?? `new ${specNameKey(d.new_title ?? "")}` }));
  const beforeParts = keyed(then.specs);
  const now = keyed(change.specs);
  const keys = [...new Set([...beforeParts, ...now].map(({ k }) => k))];
  const name = (d: SpecDelta) =>
    d.spec_id
      ? `The ${specs.get(d.spec_id)?.title ?? "deleted"} spec`
      : `The new spec “${d.new_title}”`;
  const nothingThen =
    then.specs.length === 0 &&
    then.no_change_reason === null &&
    (then.replaces_decision_ids ?? []).length === 0;
  return (
    <div className="flex flex-col gap-1 rounded-md border border-line px-3 py-2 text-sm">
      <p className="font-medium text-warn">Changed after work started</p>
      <p className="text-xs text-muted">
        {nothingThen
          ? "It was written only after work on this issue started."
          : "It says something else now than when work on this issue started. The comments should say why."}
      </p>
      {!nothingThen && (
        <details>
          <summary className="cursor-pointer text-xs text-accent">See what it said then</summary>
          <div className="mt-2 flex flex-col gap-3">
            {[...(then.replaces_decision_ids ?? [])].sort().join() !==
              [...change.replaces_decision_ids].sort().join() && (
              <p className="text-xs text-muted">The decisions it replaces changed too.</p>
            )}
            {then.no_change_reason !== change.no_change_reason && (
              <TextDiff
                label="Why no spec changes"
                before={then.no_change_reason ? `No spec change: ${then.no_change_reason}` : ""}
                after={change.no_change_reason ? `No spec change: ${change.no_change_reason}` : ""}
              />
            )}
            {keys.map((k) => {
              const before = beforeParts.find((p) => p.k === k)?.d;
              const after = now.find((p) => p.k === k)?.d;
              const part = after ?? before;
              if (!part || (before?.text === after?.text && before?.rewrite === after?.rewrite)) {
                // The same as then, unless a new spec's title or service changed.
                const renamed =
                  before &&
                  after &&
                  (before.new_title !== after.new_title ||
                    before.new_service_id !== after.new_service_id);
                return renamed ? (
                  <p key={k} className="text-xs text-muted">
                    {name(after)}: its title or service changed.
                  </p>
                ) : null;
              }
              return (
                <div key={k} className="flex flex-col gap-1">
                  <p className="text-xs font-medium">{name(part)}</p>
                  <TextDiff
                    label={`What changed in its part for ${name(part)}`}
                    before={before?.text ?? ""}
                    after={after?.text ?? ""}
                  />
                </div>
              );
            })}
          </div>
        </details>
      )}
    </div>
  );
}

function changedWords(changed: ChangedSince): string {
  return changed.kind === "spec" ? "the spec" : changed.names.map((n) => `“${n}”`).join(", ");
}

/** Another issue, linking to it, with who it's assigned to. */
function OtherIssue({ issue }: { issue: Issue | undefined }) {
  const { workspace } = useWorkspace();
  const teams = byId(useRecords("Team"));
  const users = byId(useRecords("User"));
  if (!issue) return <>Another issue</>;
  const id = identifier(issue, teams);
  const assignee = issue.assignee_id ? users.get(issue.assignee_id)?.name : undefined;
  return (
    <>
      <Link
        to="/$workspace/issue/$identifier"
        params={{ workspace: workspace.url_key, identifier: id }}
        className="font-medium hover:underline"
      >
        {id}
      </Link>
      {assignee && ` (${assignee})`}
    </>
  );
}

export function SpecChangeSummary({
  issue,
  verb,
  author,
}: {
  issue: Issue;
  /** "Merging" with a pull request, "Approving" without. */
  verb: string;
  author: string | undefined;
}) {
  const change = useRecords("SpecChange").find((c) => c.issue_id === issue.id);
  const allSpecs = useRecords("Spec");
  const specs = useMemo(() => byId(allSpecs), [allSpecs]);
  const decisions = byId(useRecords("Decision"));
  const parts = useMemo(() => (change ? specChangeParts(change, specs) : []), [change, specs]);

  if (change?.applied_at) {
    // Reopened after it was done once: what its spec change did stays done.
    return (
      <section aria-label="Spec change" className="text-xs text-muted">
        Its spec change already took effect when it was done before, so finishing it again changes
        no spec. A further spec change goes on a new issue.
      </section>
    );
  }
  // Written, as the server counts it: it changes a spec, or says why none changes.
  if (!change || (change.specs.length === 0 && change.no_change_reason === null)) {
    return (
      <section aria-label="Spec change" className="text-xs text-muted">
        No spec change written yet: {author ?? "whoever did the work"} hasn't said what this changes
        in the specs, or that it changes none.
        {change && change.replaces_decision_ids.length > 0 && (
          <> It names decisions it replaces, but that alone isn't a spec change.</>
        )}
      </section>
    );
  }
  const replaced = change.replaces_decision_ids.map((id) => decisions.get(id));

  return (
    <section aria-label="Spec change" className="flex flex-col gap-3">
      <SpecChangeStanding issue={issue} />
      <ChangedAfterStart change={change} />
      {change.no_change_reason !== null && parts.length === 0 && (
        <p className="text-sm">
          <span className="font-medium">No spec change: </span>
          {change.no_change_reason}
        </p>
      )}
      {parts.map((part, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: parts follow the change's order of specs
        <SpecPart key={index} part={part} verb={verb} />
      ))}
      {replaced.length > 0 && (
        <div className="flex flex-col gap-1">
          <h3 className="text-sm font-medium">
            {verb} also replaces {replaced.length === 1 ? "a decision" : "these decisions"}, which
            then no longer {replaced.length === 1 ? "holds" : "hold"}:
          </h3>
          <ul className="flex flex-col gap-1 text-sm">
            {replaced.map((decision, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: a deleted decision has no record
              <li key={index}>{decision ? `“${decision.title}”` : "A decision since deleted"}</li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

function SpecPart({ part, verb }: { part: SpecChangePart; verb: string }) {
  if (part.kind === "new") {
    return (
      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">
          {verb} also adds a new spec, “{part.title}”.
        </h3>
        <details className="text-sm">
          <summary className="cursor-pointer text-xs text-accent">Read the new spec</summary>
          <div className="mt-2">
            <Markdown text={part.text} />
          </div>
        </details>
      </div>
    );
  }

  const title = part.spec ? `the ${part.spec.title} spec` : "a spec since deleted";
  if (part.kind === "rewrite") {
    const { added, changed, removed } = part.requirements;
    return (
      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">
          {verb} also rewrites {title}.
        </h3>
        <ul className="flex flex-col gap-1 text-sm">
          {added.length > 0 && <NameList change="Adds" names={added} />}
          {changed.length > 0 && <NameList change="Changes" names={changed} />}
          {removed.length > 0 && <NameList change="Removes" names={removed} />}
        </ul>
        <details className="text-sm">
          <summary className="cursor-pointer text-xs text-accent">See the full change</summary>
          <div className="mt-2">
            <TextDiff label={`What changes in ${title}`} before={part.before} after={part.after} />
          </div>
        </details>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-sm font-medium">
        {verb} also changes {title}:
      </h3>
      <ul className="flex flex-col gap-3 text-sm">
        {part.added.map((requirement) => (
          <Item key={`added ${requirement.name}`} change="Adds" name={requirement.name}>
            <Markdown text={statement(requirement)} />
          </Item>
        ))}
        {part.modified.map(({ before, after }) => (
          <Item key={`modified ${after.name}`} change="Changes" name={after.name}>
            {before ? (
              <TextDiff
                label={`What changes in “${after.name}”`}
                before={before.text}
                after={after.text}
              />
            ) : (
              <>
                <p className="text-xs text-warn">
                  The spec doesn't have this requirement any more.
                </p>
                <Markdown text={statement(after)} />
              </>
            )}
          </Item>
        ))}
        {part.removed.map(({ name, before, reason }) => (
          <Item key={`removed ${name}`} change="Removes" name={name}>
            {before && (
              <div className="text-muted line-through">
                <Markdown text={statement(before)} />
              </div>
            )}
            {reason && <Markdown text={reason} />}
          </Item>
        ))}
        {part.renamed.map(({ from, to }) => (
          <li key={`renamed ${from}`}>
            <Change>Renames</Change> “{from}” to “{to}”.
          </li>
        ))}
      </ul>
    </div>
  );
}

function Change({ children }: { children: ReactNode }) {
  return <span className="font-medium">{children}</span>;
}

function Item({ change, name, children }: { change: string; name: string; children: ReactNode }) {
  return (
    <li className="flex flex-col gap-1">
      <p>
        <Change>{change}</Change> “{name}”
      </p>
      {children}
    </li>
  );
}

function NameList({ change, names }: { change: string; names: string[] }) {
  return (
    <li>
      <Change>{change}</Change> {names.map((name) => `“${name}”`).join(", ")}.
    </li>
  );
}
