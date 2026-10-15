// What an issue's spec change does to each spec, read the way the server
// reads it (`forge_domain::parse_delta`): OpenSpec's delta sections, or a
// spec's whole new text. The review bar shows it next to the pull request,
// so the PO approves the spec change together with the work.
import type { Spec, SpecChange, SpecConflict, SpecDelta, SpecVersion } from "@forge/types";
import { type Requirement, requirements } from "./verdicts";

export type Delta = {
  added: Requirement[];
  modified: Requirement[];
  removed: Requirement[];
  renamed: { from: string; to: string }[];
};

const SECTIONS = {
  "added requirements": "added",
  "modified requirements": "modified",
  "removed requirements": "removed",
  "renamed requirements": "renamed",
} as const;

/** The delta's sections. The server has already refused text that isn't one. */
export function parseDelta(text: string): Delta {
  const delta: Delta = { added: [], modified: [], removed: [], renamed: [] };
  const sections: { kind: keyof Delta; lines: string[] }[] = [];
  let fenced = false;
  for (const line of text.split(/\r?\n/)) {
    if (line.trimStart().startsWith("```")) fenced = !fenced;
    if (!fenced && line.startsWith("## ")) {
      const kind = SECTIONS[line.slice(3).trim().toLowerCase() as keyof typeof SECTIONS];
      if (kind) sections.push({ kind, lines: [] });
      continue;
    }
    sections.at(-1)?.lines.push(line);
  }
  for (const { kind, lines } of sections) {
    const body = lines.join("\n");
    if (kind === "renamed") delta.renamed.push(...renames(body));
    else delta[kind].push(...requirements(body));
  }
  return delta;
}

/** `- FROM: `### Requirement: Old`` followed by `- TO: `### Requirement: New``. */
function renames(body: string): { from: string; to: string }[] {
  const found: { from: string; to: string }[] = [];
  let from: string | null = null;
  for (const raw of body.split(/\r?\n/)) {
    const line = raw
      .trim()
      .replace(/^[-*]+/, "")
      .trim();
    const name = (label: string) =>
      line
        .slice(label.length)
        .trim()
        .replace(/^`+|`+$/g, "")
        .trim()
        .replace(/^### Requirement:/, "")
        .trim();
    if (line.startsWith("FROM:")) from = name("FROM:") || null;
    else if (line.startsWith("TO:") && from) {
      const to = name("TO:");
      if (to) found.push({ from, to });
      from = null;
    }
  }
  return found;
}

/** A requirement's sentence: the text under its heading, before its first scenario. */
export function statement(requirement: Requirement): string {
  const lines: string[] = [];
  for (const line of requirement.text.split(/\r?\n/).slice(1)) {
    if (line.startsWith("#### ")) break;
    lines.push(line);
  }
  return lines.join("\n").trim();
}

/** Requirements a rewrite adds, changes and removes, by name. */
export type Comparison = { added: string[]; changed: string[]; removed: string[] };

export function compare(before: string, after: string): Comparison {
  const old = requirements(before);
  const now = requirements(after);
  const had = (name: string) => old.find((r) => r.name === name);
  return {
    added: now.filter((r) => !had(r.name)).map((r) => r.name),
    changed: now.filter((r) => had(r.name) && had(r.name)?.text !== r.text).map((r) => r.name),
    removed: old.filter((r) => !now.some((n) => n.name === r.name)).map((r) => r.name),
  };
}

/** What a spec change does to one spec, against the spec as it is now. */
export type SpecChangePart =
  | { kind: "new"; title: string; text: string }
  | {
      kind: "rewrite";
      spec: Spec | undefined;
      before: string;
      after: string;
      requirements: Comparison;
    }
  | {
      kind: "delta";
      spec: Spec | undefined;
      added: Requirement[];
      /** `before` is null when the spec no longer has the requirement. */
      modified: { after: Requirement; before: Requirement | null }[];
      removed: { name: string; before: Requirement | null; reason: string }[];
      renamed: { from: string; to: string }[];
    };

export function specChangePart(delta: SpecDelta, specs: Map<string, Spec>): SpecChangePart {
  const spec = delta.spec_id ? specs.get(delta.spec_id) : undefined;
  if (!delta.spec_id) return { kind: "new", title: delta.new_title ?? "", text: delta.text };
  if (delta.rewrite) {
    const before = spec?.body ?? "";
    return {
      kind: "rewrite",
      spec,
      before,
      after: delta.text,
      requirements: compare(before, delta.text),
    };
  }
  const { added, modified, removed, renamed } = parseDelta(delta.text);
  const current = requirements(spec?.body ?? "");
  const find = (name: string) => current.find((r) => r.name === name) ?? null;
  return {
    kind: "delta",
    spec,
    added,
    modified: modified.map((after) => ({ after, before: find(after.name) })),
    removed: removed.map((r) => ({ name: r.name, before: find(r.name), reason: statement(r) })),
    renamed,
  };
}

/** Each spec the change touches, in the order it names them. */
export function specChangeParts(change: SpecChange, specs: Map<string, Spec>): SpecChangePart[] {
  return change.specs.map((delta) => specChangePart(delta, specs));
}

const VERBS = {
  done: { created: "Created it.", rewrote: "Rewrote it.", changed: "Changed it.", lead: "" },
  ahead: {
    created: "Will create it.",
    rewrote: "Will rewrite it.",
    changed: "Will change it.",
    lead: "will ",
  },
};

/**
 * What a spec change did to one spec, in words; `undefined` for a spec it
 * created. `ahead` says what it will do, for one still on the way.
 */
export function describeDelta(
  delta: SpecDelta | undefined,
  when: "done" | "ahead" = "done",
): string {
  const verbs = VERBS[when];
  if (!delta) return verbs.created;
  if (delta.rewrite) return verbs.rewrote;
  const { added, modified, removed, renamed } = parseDelta(delta.text);
  const past = when === "done";
  const names = (verb: string, list: string[]) =>
    list.length > 0 ? `${verb} ${list.map((name) => `“${name}”`).join(", ")}` : null;
  const parts = [
    names(
      past ? "added" : "add",
      added.map((r) => r.name),
    ),
    names(
      past ? "changed" : "change",
      modified.map((r) => r.name),
    ),
    names(
      past ? "removed" : "remove",
      removed.map((r) => r.name),
    ),
    ...renamed.map(({ from, to }) => `${past ? "renamed" : "rename"} “${from}” to “${to}”`),
  ].filter((part): part is string => part !== null);
  if (parts.length === 0) return verbs.changed;
  const sentence = verbs.lead + parts.join("; ");
  return `${sentence.charAt(0).toUpperCase()}${sentence.slice(1)}.`;
}

/** What a spec change still on the way will do to a requirement the spec has now. */
export type Upcoming = "change" | "remove" | "rename";

/** What a change on the way will do to one requirement the spec has now. */
export type Touch = {
  /** The requirement's name now. */
  name: string;
  upcoming: Upcoming;
  /** Its whole text afterwards, for a change. */
  after?: string;
  /** Its new name, for a rename. */
  to?: string;
};

/** A spec change of an open issue that will change the spec (FOR-75). */
export type PendingChange = {
  change: SpecChange;
  /** What it will do to the spec, in words. */
  what: string;
  /** The spec's requirements it will change, remove or rename. */
  touches: Touch[];
  /** The requirements it will add, which the spec doesn't have yet. */
  adds: Requirement[];
};

/**
 * The spec changes still on the way to a spec: those of issues that aren't
 * done or canceled yet (`open`), oldest first. Once the issue is done the
 * change takes effect and moves to the spec's history.
 */
export function pendingChanges(
  spec: Spec,
  changes: SpecChange[],
  open: (issueId: string) => boolean,
): PendingChange[] {
  return changes
    .flatMap((change) => {
      const delta = change.specs.find((d) => d.spec_id === spec.id);
      if (!delta || change.applied_at || !change.issue_id || !open(change.issue_id)) return [];
      return [
        {
          change,
          what: describeDelta(delta, "ahead"),
          touches: touches(spec, delta),
          adds: adds(spec, delta),
        },
      ];
    })
    .sort((a, b) => compareInstants(a.change.created_at, b.change.created_at));
}

/** The requirements a delta or rewrite will change, remove or rename. */
function touches(spec: Spec, delta: SpecDelta): Touch[] {
  const has = new Set(requirements(spec.body).map((r) => r.name));
  if (delta.rewrite) {
    const { changed, removed } = compare(spec.body, delta.text);
    const after = requirements(delta.text);
    return [
      ...changed.map((name) => ({
        name,
        upcoming: "change" as const,
        after: after.find((r) => r.name === name)?.text,
      })),
      ...removed.map((name) => ({ name, upcoming: "remove" as const })),
    ];
  }
  const { modified, removed, renamed } = parseDelta(delta.text);
  return [
    ...modified.map((r) => ({ name: r.name, upcoming: "change" as const, after: r.text })),
    ...removed.map((r) => ({ name: r.name, upcoming: "remove" as const })),
    ...renamed.map(({ from, to }) => ({ name: from, upcoming: "rename" as const, to })),
  ].filter(({ name }) => has.has(name));
}

/** The requirements a delta or rewrite will add that the spec doesn't have yet. */
function adds(spec: Spec, delta: SpecDelta): Requirement[] {
  const has = new Set(requirements(spec.body).map((r) => r.name));
  const added = delta.rewrite
    ? requirements(delta.text).filter((r) => !has.has(r.name))
    : parseDelta(delta.text).added;
  return added.filter((r) => !has.has(r.name));
}

/** One step of a spec's history: a version of its text and the one before. */
export type HistoryEntry = {
  version: SpecVersion;
  /** The text before; empty for the version that created the spec. */
  before: string;
  /** What changed, in words. */
  what: string;
};

/** A spec's history from its versions (oldest first), newest first. */
export function historyEntries(versions: SpecVersion[]): HistoryEntry[] {
  return versions
    .map((version, index) => {
      const before = versions[index - 1]?.body ?? "";
      return { version, before, what: describeVersion(before, version) };
    })
    .reverse();
}

function describeVersion(before: string, version: SpecVersion): string {
  if (version.created) return "Created it.";
  const { added, changed, removed } = compare(before, version.body);
  const names = (verb: string, list: string[]) =>
    list.length > 0 ? `${verb} ${list.map((name) => `“${name}”`).join(", ")}` : null;
  const parts = [
    names("added", added),
    names("changed", changed),
    names("removed", removed),
  ].filter((part): part is string => part !== null);
  if (parts.length === 0) return "Changed its text.";
  const sentence = parts.join("; ");
  return `${sentence.charAt(0).toUpperCase()}${sentence.slice(1)}.`;
}

/**
 * The version each requirement the spec has now last changed in: added,
 * or its text changed. Versions oldest first.
 */
export function lastChanged(versions: SpecVersion[]): Map<string, SpecVersion> {
  const last = new Map<string, SpecVersion>();
  versions.forEach((version, index) => {
    const before = requirements(versions[index - 1]?.body ?? "");
    for (const requirement of requirements(version.body)) {
      const was = before.find((r) => r.name === requirement.name);
      if (!was || was.text !== requirement.text) last.set(requirement.name, version);
    }
  });
  return last;
}

/**
 * Where open issues' spec changes (`open`: not done or canceled, not taken
 * effect) change the same requirement of a spec, or one rewrites a spec
 * another changes, read the way the server reads it
 * (`forge_domain::spec_conflicts`). Of two, the part written first goes
 * first; the other issue can't start until they settle it.
 */
export function specConflicts(open: SpecChange[]): SpecConflict[] {
  type Part = {
    when: string;
    change: string;
    spec: string;
    names: string[] | null;
    touchedAt: Record<string, string>;
  };
  const parts: Part[] = [];
  for (const change of open) {
    for (const delta of change.specs) {
      if (!delta.spec_id) continue;
      let names: string[] | null = null;
      if (!delta.rewrite) {
        const { added, modified, removed, renamed } = parseDelta(delta.text);
        names = [
          ...modified.map((r) => r.name),
          ...removed.map((r) => r.name),
          ...renamed.map((r) => r.from),
          ...added.map((r) => r.name),
          ...renamed.map((r) => r.to),
        ];
      }
      parts.push({
        when: delta.written_at ?? change.created_at,
        change: change.id,
        spec: delta.spec_id,
        names,
        // Parts saved before Forge kept this have no such field.
        touchedAt: delta.touched_at ?? {},
      });
    }
  }
  // As instants: the server's and the database's timestamps are written differently.
  const order = (a: [string, string], b: [string, string]) =>
    compareInstants(a[0], b[0]) || a[1].localeCompare(b[1]);
  parts.sort((a, b) => order([a.when, a.change], [b.when, b.change]));
  // When a part first touched a requirement; older parts count from when they were written.
  const since = (p: Part, name: string): [string, string] => [
    p.touchedAt[name] ?? p.when,
    p.change,
  ];
  const conflicts: SpecConflict[] = [];
  parts.forEach((later, index) => {
    for (const earlier of parts.slice(0, index)) {
      if (earlier.spec !== later.spec || earlier.change === later.change) continue;
      const conflict = (requirement: string | null, first: Part, second: Part) => ({
        spec_id: later.spec,
        requirement,
        first: first.change,
        second: second.change,
      });
      if (earlier.names && later.names) {
        const theirs = earlier.names;
        for (const name of later.names.filter((n) => theirs.includes(n))) {
          const earlierFirst = order(since(earlier, name), since(later, name)) <= 0;
          conflicts.push(
            earlierFirst ? conflict(name, earlier, later) : conflict(name, later, earlier),
          );
        }
      } else {
        conflicts.push(conflict(null, earlier, later));
      }
    }
  });
  return conflicts;
}

/** A spec's name as Forge compares names: ignoring case and surrounding spaces. */
export function specNameKey(name: string): string {
  return name.trim().toLowerCase();
}

/**
 * The new specs `mine` writes that another open issue's spec change wrote
 * first, read the way the server reads it: the one there first writes it.
 */
export function newSpecsTaken(
  open: SpecChange[],
  mine: SpecChange,
): { change: string; title: string }[] {
  const title = (d: SpecDelta) =>
    d.spec_id || d.new_title == null ? null : specNameKey(d.new_title);
  const since = (c: SpecChange, d: SpecDelta) => [d.written_at ?? c.created_at, c.id] as const;
  const before = (a: readonly [string, string], b: readonly [string, string]) =>
    (compareInstants(a[0], b[0]) || a[1].localeCompare(b[1])) < 0;
  return mine.specs.flatMap((delta) => {
    const name = title(delta);
    if (!name) return [];
    return open.flatMap((other) => {
      if (other.id === mine.id) return [];
      const theirs = other.specs.find((d) => title(d) === name);
      return theirs && before(since(other, theirs), since(mine, delta))
        ? [{ change: other.id, title: delta.new_title?.trim() ?? "" }]
        : [];
    });
  });
}

/** What changed in a spec since a part of a spec change was written against it. */
export type ChangedSince = { kind: "spec" } | { kind: "requirements"; names: string[] };

/**
 * What changed in the spec (`body`, as it is now) since the part was
 * written, which taking effect would undo; null when nothing it relies on
 * did, or it doesn't say what it was written against.
 */
export function changedSince(delta: SpecDelta, body: string): ChangedSince | null {
  // Parts saved before Forge kept this have no such field at all.
  if (delta.based_on == null) return null;
  if (delta.rewrite) {
    return delta.based_on.trimEnd() !== body.trimEnd() ? { kind: "spec" } : null;
  }
  const { modified, removed, renamed } = parseDelta(delta.text);
  const text = (list: Requirement[], name: string) => list.find((r) => r.name === name)?.text;
  const then = requirements(delta.based_on);
  const now = requirements(body);
  const { added } = parseDelta(delta.text);
  const names = [
    ...[
      ...modified.map((r) => r.name),
      ...removed.map((r) => r.name),
      ...renamed.map((r) => r.from),
    ].filter((name) => text(then, name) !== text(now, name)),
    // A name it adds (or renames to) that the spec has gained since.
    ...[...added.map((r) => r.name), ...renamed.map((r) => r.to)].filter(
      (name) => text(then, name) === undefined && text(now, name) !== undefined,
    ),
  ];
  return names.length > 0 ? { kind: "requirements", names } : null;
}

/**
 * Whether the spec change says something else now than when work on its
 * issue started (`SpecChange::changed_after_start` on the server).
 */
export function changedAfterStart(change: SpecChange): boolean {
  const started = change.as_started;
  if (!started) return false;
  const what = (d: SpecDelta) =>
    JSON.stringify([d.spec_id, d.new_title, d.new_service_id, d.text, d.rewrite]);
  // Copies kept before Forge kept the decisions too have no such field.
  // In any order: saving parts again can reorder them.
  const same = (a: string[], b: string[]) => [...a].sort().join("\n") === [...b].sort().join("\n");
  return (
    started.no_change_reason !== change.no_change_reason ||
    !same(started.replaces_decision_ids ?? [], change.replaces_decision_ids) ||
    !same(started.specs.map(what), change.specs.map(what))
  );
}

/**
 * Two timestamps compared as instants, to the nanosecond as the server
 * compares them: the server and the database write them differently
 * ("…Z" or "…+00:00", with as many fraction digits as they have).
 */
export function compareInstants(a: string, b: string): number {
  // Date.parse keeps milliseconds; the rest of the fraction breaks ties.
  const below = (at: string) => Number(`${/\.(\d+)/.exec(at)?.[1] ?? ""}000000000`.slice(3, 9));
  return Date.parse(a) - Date.parse(b) || below(a) - below(b);
}
