import { expect, test } from "bun:test";
import type { Spec, SpecChange, SpecDelta, SpecVersion } from "@forge/types";
import {
  changedAfterStart,
  changedSince,
  compare,
  compareInstants,
  describeDelta,
  historyEntries,
  lastChanged,
  newSpecsTaken,
  parseDelta,
  pendingChanges,
  specChangePart,
  specConflicts,
  statement,
} from "./spec-change";

// The server's own example (`forge_domain::spec_change` tests).
const DELTA = `## ADDED Requirements

### Requirement: Limits reset
The limit SHALL reset every minute.

#### Scenario: A new minute
- **WHEN** a minute passes
- **THEN** the token can send 100 requests again

## MODIFIED Requirements

### Requirement: Tokens are limited
The API SHALL allow 200 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 201st request within a minute
- **THEN** it gets 429

## REMOVED Requirements

### Requirement: Old limit
**Reason**: Replaced by the per-token limit.

## RENAMED Requirements

- FROM: \`### Requirement: Login\`
- TO: \`### Requirement: Signing in\`
`;

const SPEC: Spec = {
  id: "spec",
  workspace_id: "w",
  title: "Rate limits",
  body: `## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429

### Requirement: Login
People SHALL sign in.

#### Scenario: Signing in
- **WHEN** someone signs in
- **THEN** they're in
`,
  status: "active",
  service_ids: [],
  domain_id: null,
  project_id: null,
  created_by: "u",
  created_at: "",
  updated_at: "",
};

const delta = (fields: Partial<SpecDelta>): SpecDelta => ({
  spec_id: "spec",
  new_title: null,
  new_service_id: null,
  written_at: null,
  touched_at: {},
  based_on: null,
  text: DELTA,
  rewrite: false,
  ...fields,
});

test("each delta section is read, as the server reads it", () => {
  const { added, modified, removed, renamed } = parseDelta(DELTA);
  expect(added.map((r) => r.name)).toEqual(["Limits reset"]);
  expect(modified.map((r) => r.name)).toEqual(["Tokens are limited"]);
  expect(modified[0]?.text).toContain("201st request");
  expect(removed.map((r) => r.name)).toEqual(["Old limit"]);
  expect(renamed).toEqual([{ from: "Login", to: "Signing in" }]);
});

test("a requirement's sentence leaves out its scenarios", () => {
  const [added] = parseDelta(DELTA).added;
  expect(added && statement(added)).toBe("The limit SHALL reset every minute.");
});

test("a changed requirement is shown against the spec as it is now", () => {
  const part = specChangePart(delta({}), new Map([["spec", SPEC]]));
  if (part.kind !== "delta") throw new Error(part.kind);
  expect(part.spec?.title).toBe("Rate limits");
  expect(part.modified[0]?.before?.text).toContain("100 requests a minute");
  expect(part.modified[0]?.after.text).toContain("200 requests a minute");
  // The spec doesn't have "Old limit" (any more): there's nothing to show it against.
  expect(part.removed).toEqual([
    { name: "Old limit", before: null, reason: "**Reason**: Replaced by the per-token limit." },
  ]);
});

test("a rewrite says which requirements it adds, changes and removes", () => {
  const after = SPEC.body
    .replace("100 requests", "200 requests")
    .replace("### Requirement: Login", "### Requirement: Signing in");
  expect(compare(SPEC.body, after)).toEqual({
    added: ["Signing in"],
    changed: ["Tokens are limited"],
    removed: ["Login"],
  });
});

test("rewrites and new specs are whole texts", () => {
  const specs = new Map([["spec", SPEC]]);
  expect(specChangePart(delta({ text: "## Purpose\n\nNew.", rewrite: true }), specs)).toEqual({
    kind: "rewrite",
    spec: SPEC,
    before: SPEC.body,
    after: "## Purpose\n\nNew.",
    requirements: { added: [], changed: [], removed: ["Tokens are limited", "Login"] },
  });
  expect(
    specChangePart(
      delta({ spec_id: null, new_title: "Quotas", text: "## Purpose", rewrite: true }),
      specs,
    ),
  ).toEqual({ kind: "new", title: "Quotas", text: "## Purpose" });
});

test("says in words what a spec change did to a spec", () => {
  const delta: SpecDelta = {
    spec_id: "spec",
    new_title: null,
    new_service_id: null,
    written_at: null,
    touched_at: {},
    based_on: null,
    text: DELTA,
    rewrite: false,
  };
  expect(describeDelta(delta)).toBe(
    "Added “Limits reset”; changed “Tokens are limited”; removed “Old limit”; renamed “Login” to “Signing in”.",
  );
  expect(describeDelta({ ...delta, text: "## Purpose", rewrite: true })).toBe("Rewrote it.");
  expect(describeDelta(undefined)).toBe("Created it.");
});

test("says in words what a spec change will do, while it's on the way", () => {
  expect(describeDelta(delta({}), "ahead")).toBe(
    "Will add “Limits reset”; change “Tokens are limited”; remove “Old limit”; rename “Login” to “Signing in”.",
  );
  expect(describeDelta(delta({ rewrite: true }), "ahead")).toBe("Will rewrite it.");
});

const version = (at: string, body: string, created = false): SpecVersion => ({
  at,
  actor_id: "u",
  created,
  body,
  spec_change_id: null,
});

test("a spec's history says what each version changed, newest first", () => {
  const changed = SPEC.body.replace("100 requests", "200 requests");
  const versions = [
    version("2026-10-14T10:00:00Z", "## Purpose\n\nRate limits.", true),
    version("2026-10-14T11:00:00Z", SPEC.body),
    version("2026-10-14T12:00:00Z", changed),
    version("2026-10-14T13:00:00Z", changed.replace("Rate limits.", "Limits per token.")),
  ];
  const history = historyEntries(versions);
  expect(history.map((h) => h.what)).toEqual([
    "Changed its text.",
    "Changed “Tokens are limited”.",
    "Added “Tokens are limited”, “Login”.",
    "Created it.",
  ]);
  expect(history[1]?.before).toBe(SPEC.body);
  expect(history[3]?.before).toBe("");

  const last = lastChanged(versions);
  expect(last.get("Tokens are limited")?.at).toBe("2026-10-14T12:00:00Z");
  expect(last.get("Login")?.at).toBe("2026-10-14T11:00:00Z");
});

/** An issue's spec change, not applied yet, that applies `DELTA` to the spec. */
const change = (id: string, fields: Partial<SpecChange>): SpecChange => ({
  id,
  workspace_id: "w",
  issue_id: `issue-${id}`,
  specs: [delta({})],
  replaces_decision_ids: [],
  no_change_reason: null,
  applied_at: null,
  applied_spec_ids: [],
  not_applied: [],
  as_started: null,
  created_by: "u",
  created_at: "2026-10-14T10:00:00Z",
  updated_at: "2026-10-14T10:00:00Z",
  ...fields,
});

test("changes on the way are those of open issues not applied yet, oldest first", () => {
  const done = new Set(["issue-done", "issue-canceled"]);
  const pending = pendingChanges(
    SPEC,
    [
      change("newer", { created_at: "2026-10-14T12:00:00Z" }),
      change("older", { created_at: "2026-10-14T11:00:00Z" }),
      change("applied", { applied_at: "2026-10-14T13:00:00Z", applied_spec_ids: ["spec"] }),
      change("done", {}),
      change("canceled", {}),
      change("elsewhere", { specs: [delta({ spec_id: "other" })] }),
    ],
    (issueId) => !done.has(issueId),
  );
  expect(pending.map((p) => p.change.id)).toEqual(["older", "newer"]);
  expect(pending[0]?.what).toBe(
    "Will add “Limits reset”; change “Tokens are limited”; remove “Old limit”; rename “Login” to “Signing in”.",
  );
  // "Old limit" isn't in the spec, so there's no requirement to mark.
  expect(pending[0]?.touches.map(({ name, upcoming, to }) => ({ name, upcoming, to }))).toEqual([
    { name: "Tokens are limited", upcoming: "change", to: undefined },
    { name: "Login", upcoming: "rename", to: "Signing in" },
  ]);
  expect(pending[0]?.touches[0]?.after).toContain("200 requests a minute");
  expect(pending[0]?.adds.map((r) => r.name)).toEqual(["Limits reset"]);
});

test("a rewrite on the way marks the requirements it changes and removes", () => {
  const after = SPEC.body
    .replace("100 requests", "200 requests")
    .replace(/### Requirement: Login[\s\S]*$/, "");
  const [pending] = pendingChanges(
    SPEC,
    [change("rewrite", { specs: [delta({ text: after, rewrite: true })] })],
    () => true,
  );
  expect(pending?.what).toBe("Will rewrite it.");
  expect(pending?.touches.map(({ name, upcoming }) => ({ name, upcoming }))).toEqual([
    { name: "Tokens are limited", upcoming: "change" },
    { name: "Login", upcoming: "remove" },
  ]);
  expect(pending?.touches[0]?.after).toContain("200 requests");
  expect(pending?.adds).toEqual([]);
});

const modifies = (name: string, rule: string) =>
  `## MODIFIED Requirements\n\n### Requirement: ${name}\n${rule}\n\n#### Scenario: S\n- **WHEN** w\n- **THEN** t\n`;
const adds = (name: string) =>
  `## ADDED Requirements\n\n### Requirement: ${name}\nPasswords SHALL obey it.\n\n#### Scenario: S\n- **WHEN** w\n- **THEN** t\n`;
const written = (id: string, text: string, minute: number, rewrite = false) =>
  change(id, {
    specs: [delta({ text, rewrite, written_at: `2026-10-15T10:0${minute}:00Z` })],
  });

test("two open issues changing one requirement conflict, the later one second", () => {
  const conflicts = specConflicts([
    written("max", adds("Maximum length"), 1),
    written("special", adds("Special character"), 2),
    written("min12", modifies("Tokens are limited", "At least 12."), 4),
    written("min8", modifies("Tokens are limited", "At least 8."), 3),
    change("elsewhere", { specs: [delta({ spec_id: "other" })] }),
  ]);
  expect(conflicts).toEqual([
    { spec_id: "spec", requirement: "Tokens are limited", first: "min8", second: "min12" },
  ]);
  expect(
    specConflicts([written("max", adds("Maximum length"), 1), written("all", "x", 2, true)]),
  ).toEqual([{ spec_id: "spec", requirement: null, first: "max", second: "all" }]);
});

test("a part written against older text says what changed since", () => {
  const part = delta({ text: modifies("Tokens are limited", "At least 12."), based_on: SPEC.body });
  expect(changedSince(part, SPEC.body)).toBeNull();
  const now = SPEC.body.replace("100 requests", "150 requests");
  expect(changedSince(part, now)).toEqual({ kind: "requirements", names: ["Tokens are limited"] });
  expect(changedSince(delta({ text: "All.", rewrite: true, based_on: SPEC.body }), now)).toEqual({
    kind: "spec",
  });
  expect(changedSince(delta({}), now)).toBeNull();
  // Parts saved before Forge kept what they were written against have no such field.
  const { based_on: _, ...older } = delta({ text: "All.", rewrite: true });
  expect(changedSince(older as SpecDelta, now)).toBeNull();
});

test("a spec change says whether it changed after work started", () => {
  const started = change("c", {});
  expect(changedAfterStart(started)).toBe(false);
  const then = { specs: started.specs, no_change_reason: null, replaces_decision_ids: [] };
  expect(changedAfterStart({ ...started, as_started: then })).toBe(false);
  const rewritten = { ...started, specs: [delta({ text: "Other." })], as_started: then };
  expect(changedAfterStart(rewritten)).toBe(true);
  expect(changedAfterStart({ ...started, replaces_decision_ids: ["d"], as_started: then })).toBe(
    true,
  );
  expect(
    changedAfterStart({
      ...started,
      as_started: { specs: [], no_change_reason: null, replaces_decision_ids: [] },
    }),
  ).toBe(true);
});

test("widening an old part later never jumps ahead", () => {
  const b = change("b", {
    specs: [
      delta({
        text: `${adds("Bursts")}\n${modifies("Tokens are limited", "At least 12.")}`,
        written_at: "2026-10-15T10:00:00Z",
        touched_at: {
          Bursts: "2026-10-15T10:00:00Z",
          "Tokens are limited": "2026-10-15T10:05:00Z",
        },
      }),
    ],
  });
  const a = written("a", modifies("Tokens are limited", "At least 8."), 1);
  expect(specConflicts([a, b])).toEqual([
    { spec_id: "spec", requirement: "Tokens are limited", first: "a", second: "b" },
  ]);
});

test("a new spec another open issue wrote first holds this one back", () => {
  const quotas = (id: string, minute: number, title = "Quotas") =>
    change(id, {
      specs: [
        delta({
          spec_id: null,
          new_title: title,
          text: "## Purpose\n\nQuotas.",
          rewrite: true,
          written_at: `2026-10-15T10:0${minute}:00Z`,
        }),
      ],
    });
  const [first, second] = [quotas("first", 1), quotas("second", 2, " quotas ")];
  expect(newSpecsTaken([first, second], second)).toEqual([{ change: "first", title: "quotas" }]);
  expect(newSpecsTaken([first, second], first)).toEqual([]);
  expect(newSpecsTaken([first, quotas("other", 3, "Limits")], first)).toEqual([]);
});

test("timestamps compare as instants, whoever wrote them", () => {
  expect(
    compareInstants("2026-10-15T10:00:00.123Z", "2026-10-15T10:00:00.123456789Z"),
  ).toBeLessThan(0);
  expect(
    compareInstants("2026-10-15T10:00:00.5+00:00", "2026-10-15T10:00:00.499999Z"),
  ).toBeGreaterThan(0);
  expect(compareInstants("2026-10-15T12:00:00+02:00", "2026-10-15T10:00:00Z")).toBe(0);
});

test("parts only saved again in another order haven't changed since work started", () => {
  const two = change("c", { specs: [delta({}), delta({ spec_id: "other" })] });
  const then = { specs: two.specs, no_change_reason: null, replaces_decision_ids: [] };
  expect(changedAfterStart({ ...two, specs: [...two.specs].reverse(), as_started: then })).toBe(
    false,
  );
});

test("a part adding a requirement the spec has gained since is out of date", () => {
  const adding = delta({ text: adds("Limits reset"), based_on: SPEC.body });
  expect(changedSince(adding, SPEC.body)).toBeNull();
  const gained = `${SPEC.body}\n${adds("Limits reset").replace("## ADDED Requirements\n\n", "")}`;
  expect(changedSince(adding, gained)).toEqual({ kind: "requirements", names: ["Limits reset"] });
});
