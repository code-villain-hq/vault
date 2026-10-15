/**
 * Specs (§19) for the mock harness: one per task, its requirements and
 * design the task's and each repository's tasks, approved or drafted;
 * drafts streamed in a line at a time; a copy changed outside the app and
 * an agent's proposal (SPEC-22, SPEC-24); and a check that answers from
 * the requirements. `&spec=split` gives ACME-123 a spec per repository from
 * before, to merge (SPEC-25). Parsing is rough, as far as the seeded texts
 * need; `spec.rs` is the real thing.
 */
import { emit } from "@tauri-apps/api/event";
import { questions } from "../lib/questions";
import type { RepoSpec, SpecKind, SpecPart, SpecProposal, SpecVerdict, TaskSpec } from "../lib/types";
import type { World } from "./world";

const PARTS: SpecPart[] = ["requirements", "design", "tasks"];

type Check = { sha: string; at: number; results: SpecVerdict[]; stale: boolean };

/** A task's spec, each file by its slot: `requirements`, `design`, `tasks:<checkout>`. */
interface Kept {
  kind: SpecKind;
  approved: Record<string, string>;
  drafts: Record<string, string>;
  stale: string[];
  /** A repository's copy that is not what was approved, by `<slot>@<checkout>`. */
  outside: Record<string, string>;
  proposals: Record<string, SpecProposal>;
  checks: Record<string, Check>;
  /** Each repository's spec from before, by checkout: the merge's input (SPEC-25). */
  before: Record<string, Record<SpecPart, string>> | null;
}

const slot = (part: SpecPart, checkout: string | null) => (part === "tasks" ? `tasks:${checkout}` : part);

const API_REQUIREMENTS = `## Goal
Two logins at once no longer leave one of them signed out.

## Requirements
- R-1 [api]: WHEN two sign-ins for one account arrive within a second THE SYSTEM SHALL keep both sessions valid.
- R-2 [api]: WHEN a session refresh loses the race THE SYSTEM SHALL retry once, then answer 409 with a reason.
- R-3 [api, web]: WHEN a sign-in lost the race THE SYSTEM SHALL say "Signed in elsewhere at the same moment. Try again." instead of looping.
- R-4 [web]: WHEN a sign-in is in flight THE SYSTEM SHALL disable the submit button.

## Out of scope
- Rate-limiting sign-ins.

## Open questions
None.
`;

const API_DESIGN = `## Approach
Refresh under a per-account lock in Redis, so the second refresh waits for the first (R-1); the web says so when the API gives up (R-3).

## Contracts
- C-1: \`POST /sessions\` answers 409 with \`{ "reason": "session_race" }\` when a refresh lost the race twice. Provided by \`api\`, used by \`web\` (R-2, R-3).

## api
### Changes
- \`src/auth/session.ts\`: take \`lock:session:<account>\` around refresh (R-1).
- \`src/auth/login.ts\`: retry once on a lost race, then C-1 (R-2).
### Error handling
A lock not taken in 2s is a lost race.
### Testing
Two parallel sign-ins in \`session.test.ts\` (R-1); a forced lost race (R-2).

## web
### Changes
- \`src/signin/Form.tsx\`: disable submit while in flight (R-4); show the message on C-1 (R-3).
### Error handling
Any other 409 shows the generic error.
### Testing
\`Form.test.tsx\`: C-1 shows the message (R-3); double submit is blocked (R-4).
`;

const API_TASKS = `## Tasks
- [x] 1. Take a per-account lock around session refresh (R-1)
- [x] 2. Test two parallel sign-ins (R-1)
- [ ] 3. Retry a lost race once, then answer 409 (R-2, C-1)
- [ ] 4. Test a forced lost race (R-2)
`;

const WEB_TASKS = `## Tasks
- [ ] 1. Disable submit while a sign-in is in flight (R-4)
- [ ] 2. Show the message on a session_race 409 (R-3, C-1; after api 3)
- [ ] 3. Test both in Form.test.tsx (R-3, R-4)
`;

/** What an agent proposed for the api's tasks (SPEC-24). */
const API_TASKS_PROPOSED = API_TASKS.replace("- [ ] 4. Test a forced lost race (R-2)", "- [ ] 4. Test a forced lost race (R-2)\n- [ ] 5. Log each lost race with the account id (R-2)");

/** The web's spec from before, when each repository had its own (SPEC-25). */
const WEB_REQUIREMENTS = `## Goal
The sign-in page says so when a sign-in lost a race, instead of looping.

## Requirements
- R-1: WHEN the API answers 409 with reason session_race THE SYSTEM SHALL show "Signed in elsewhere at the same moment. Try again."
- R-2: WHEN a sign-in is in flight THE SYSTEM SHALL disable the submit button.

## Out of scope
- The API's retry (\`api R-2\`).

## Open questions
None.
`;

/** What a draft streams in, by file. */
const DRAFT: Record<SpecPart, (folder: string) => string> = {
  requirements: (folder) => `## Goal
Password resets are rate-limited, so one address cannot be flooded with reset mails.

## Requirements
- R-1 [${folder}]: WHEN a sixth reset is asked for one address within an hour THE SYSTEM SHALL refuse it with a clear message.
- R-2 [${folder}]: WHEN resets are asked for different addresses THE SYSTEM SHALL count them separately.

## Out of scope
- Rate-limiting sign-ins.

## Open questions
- Q-1: Should support staff be able to lift the limit for an address?
  - Yes, from the admin page
  - No
- Q-2: Should the refusal say when to try again?
  - Yes, the minutes left
  - No, only that it was refused
`,
  design: (folder) => `## Approach
Count reset requests per address in Redis with a one-hour expiry, in \`${folder}\`.

## Changes
- \`src/auth/reset.ts\`: check the count before sending (R-1, R-2).

## Interfaces
None.

## Error handling
Redis down: let the reset through, and log it.

## Testing
Six requests for one address (R-1); two addresses (R-2).
`,
  tasks: () => `## Tasks
- [ ] 1. Count resets per address with an hour's expiry (R-1, R-2)
- [ ] 2. Refuse the sixth with a clear message (R-1)
- [ ] 3. Test one address and two (R-1, R-2)
`,
};

/** ACME-130's requirements, drafted and half answered (SPEC-19). */
const AUDIT_DRAFT = `## Goal
Admins can see who changed what in the workspace, and when.

## Requirements
- R-1: WHEN an admin opens the audit log THE SYSTEM SHALL list changes newest first, with who, what and when.
- R-2: WHEN the list is filtered by person THE SYSTEM SHALL show only that person's changes.
- R-3: WHEN an entry is older than the kept period THE SYSTEM SHALL leave it out.

## Out of scope
- Exporting the log.

## Open questions
- Q-1: How long are entries kept?
  - 90 days
  - A year
  - For good
  Answer: A year
- Q-2: Can admins see changes made by other admins?
  - Yes
  - Only their own
- Q-3: Should the page load more as it scrolls, or in pages of 50?
`;

/**
 * What Apply answers does here (SPEC-20), roughly: each answered question
 * becomes a requirement and leaves the list. The real run rewrites the ones
 * it decides instead.
 */
function applied(text: string): string {
  const qs = questions(text);
  const head = text.split(/^## Open questions/m)[0].trimEnd();
  const ids = requirements(text).map((r) => Number(r.id.slice(2)));
  let next = Math.max(0, ...ids);
  const added = qs.filter((q) => q.answer).map((q) =>
    `- R-${++next}: WHEN ${q.text.replace(/\?$/, "").replace(/^\w/, (c) => c.toLowerCase())} THE SYSTEM SHALL do as decided: ${q.answer}.`);
  const withReqs = head.replace(/(^- R-\d+[^\n]*\n)(?![\s\S]*^- R-\d+)/m, (m) => m + added.map((a) => a + "\n").join(""));
  const left = qs.filter((q) => !q.answer).map((q) => [`- ${q.id}: ${q.text}`, ...q.options.map((o) => `  - ${o}`)].join("\n"));
  return `${withReqs}\n\n## Open questions\n${left.length > 0 ? left.join("\n") : "None."}\n`;
}

function requirements(text: string) {
  const at = text.search(/^## (Requirements|Expected behaviour)/m);
  if (at < 0) return [];
  const part = text.slice(at).split(/^## (?!Unchanged)/m)[1] ?? "";
  return [...part.matchAll(/^- (?:\[[ xX]\] )?(R-\d+)(?: \[([^\]]*)\])?[:.]? (.+)$/gm)].map((m) => ({
    id: m[1],
    repos: (m[2] ?? "").split(",").map((r) => r.trim()).filter(Boolean),
    text: m[3].trim(),
  }));
}

function steps(text: string) {
  return [...text.matchAll(/^- \[([ xX])\] (\d+)\. (.+)$/gm)].map((m) => {
    const inner = m[3].match(/\(([^)]*)\)\s*$/)?.[1] ?? "";
    return {
      number: Number(m[2]),
      text: m[3].trim(),
      done: m[1] !== " ",
      serves: [...inner.matchAll(/(?:^|[,;] ?)(R-\d+)/g)].map((x) => x[1]),
      after: [...inner.matchAll(/after ([^,;)]+)/g)].map((x) => x[1].trim()),
    };
  });
}

/** Streams `text` into an editor as `spec:draft`, a line at a time. */
async function stream(requestId: unknown, part: SpecPart, checkoutId: string | null, text: string, pause = 40) {
  let sent = "";
  for (const piece of text.match(/[^\n]*\n?/g) ?? []) {
    await new Promise((r) => setTimeout(r, pause));
    sent += piece;
    void emit("spec:draft", { request_id: requestId, part, checkout_id: checkoutId, text: sent });
  }
  return sent;
}

export function specAnswers(world: World): Record<string, (a: Record<string, unknown>) => unknown> {
  const kept = new Map<string, Kept>();
  const get = (taskId: string): Kept => {
    let k = kept.get(taskId);
    if (!k) {
      k = { kind: "feature", approved: {}, drafts: {}, stale: [], outside: {}, proposals: {}, checks: {}, before: null };
      kept.set(taskId, k);
    }
    return k;
  };
  const task = (id: string) => world.tasks.find((t) => t.id === id);
  const checkouts = (taskId: string) => task(taskId)?.checkouts ?? [];
  const folderOf = (taskId: string, checkout: string) => checkouts(taskId).find((c) => c.id === checkout)?.project_name ?? "";

  // ACME-123, under way: one spec for api and web, api two steps in; the
  // web's copy of the requirements edited by an agent (SPEC-22), and the
  // api's agent proposing a step (SPEC-24). With `&spec=split`, the specs
  // each repository had of its own before, to merge (SPEC-25).
  if (task("t-login")) {
    const k = get("t-login");
    if (new URLSearchParams(location.search).get("spec") === "split") {
      k.before = {
        "c-login-api": { requirements: API_REQUIREMENTS.replace(/ \[[^\]]*\]/g, ""), design: API_DESIGN, tasks: API_TASKS },
        "c-login-web": { requirements: WEB_REQUIREMENTS, design: "", tasks: "" },
      };
    } else {
      Object.assign(k.approved, {
        requirements: API_REQUIREMENTS, design: API_DESIGN,
        [slot("tasks", "c-login-api")]: API_TASKS, [slot("tasks", "c-login-web")]: WEB_TASKS,
      });
      k.outside[`requirements@c-login-web`] = API_REQUIREMENTS.replace("disable the submit button", "disable the submit button and show a spinner");
      k.drafts[slot("tasks", "c-login-api")] = API_TASKS_PROPOSED;
      k.proposals[slot("tasks", "c-login-api")] = {
        why: "A lost race leaves no trace in the logs, so step 3 cannot be checked in staging.",
        by: "Claude Code", pane_id: "pane-claude", at: Date.now() - 120_000,
      };
      k.checks["c-login-api"] = { sha: "a1b2c3d4", at: Date.now() - 600_000, stale: true, results: [
        { id: "R-1", verdict: "met", evidence: "src/auth/session.ts:41, session.test.ts" },
        { id: "R-2", verdict: "not met", evidence: "no retry in login.ts yet" },
        { id: "R-3", verdict: "unclear", evidence: "the 409 body is not sent yet" },
      ] };
    }
  }

  // ACME-130: requirements drafted, with questions, one of them answered.
  if (task("t-audit")) get("t-audit").drafts.requirements = AUDIT_DRAFT;

  /** What an approved file is: what was recorded, or the first copy from before. */
  const approved = (taskId: string, key: string, checkout: string | null): string | null => {
    const k = get(taskId);
    if (k.approved[key]) return k.approved[key];
    if (!k.before) return null;
    const part = key.startsWith("tasks") ? "tasks" : (key as SpecPart);
    const from = checkout ?? Object.keys(k.before)[0];
    return k.before[from]?.[part] || null;
  };

  const view = (taskId: string): TaskSpec => {
    const t = task(taskId);
    const k = get(taskId);
    const files: { part: SpecPart; checkout: string | null }[] = [
      { part: "requirements", checkout: null },
      { part: "design", checkout: null },
      ...checkouts(taskId).map((c) => ({ part: "tasks" as SpecPart, checkout: c.id })),
    ];
    const parts = files.map(({ part, checkout }) => {
      const key = slot(part, checkout);
      return {
        part,
        checkout_id: checkout,
        file: part === "requirements" && k.kind === "bugfix" ? "bugfix.md" : `${part}.md`,
        approved: approved(taskId, key, checkout),
        draft: k.drafts[key] ?? null,
        stale: k.stale.includes(key),
        outside: Object.entries(k.outside)
          .filter(([at]) => at.split("@")[0] === key)
          .map(([at, text]) => ({ checkout_id: at.split("@")[1], folder: folderOf(taskId, at.split("@")[1]), text })),
        proposal: k.proposals[key] ?? null,
      };
    });
    const repos: RepoSpec[] = checkouts(taskId).map((c) => {
      const project = world.projects.find((p) => p.id === c.project_id);
      const inRepo = !project?.specs_in_app;
      return {
        checkout_id: c.id,
        repo: c.project_name,
        folder: c.project_name,
        home: inRepo ? `${c.project_name}/${project?.spec_folder ?? "specs"}/${t!.branch}` : `~/Library/Application Support/…/specs/${c.project_name}/${t!.branch}`,
        in_repo: inRepo,
        steps: steps(approved(taskId, slot("tasks", c.id), c.id) ?? ""),
        check: k.checks[c.id] ?? null,
      };
    });
    const split = !!k.before && !k.approved.requirements;
    return structuredClone({
      kind: k.kind, parts, requirements: requirements(approved(taskId, "requirements", null) ?? ""), repos, legacy: null, split,
    });
  };

  return {
    read_spec: (a) => view(a.taskId as string),
    save_spec_draft: (a) => {
      const k = get(a.taskId as string);
      const key = slot(a.part as SpecPart, (a.checkoutId as string | null) ?? null);
      if (a.text) k.drafts[key] = a.text as string;
      else delete k.drafts[key];
      return null;
    },
    approve_spec: (a) => {
      const taskId = a.taskId as string;
      const k = get(taskId);
      if (a.kind) k.kind = a.kind as SpecKind;
      const texts = a.texts as { part: SpecPart; checkout_id: string | null; text: string }[];
      const first = Math.min(...texts.map((t) => PARTS.indexOf(t.part)));
      const done = texts.map((t) => slot(t.part, t.checkout_id));
      for (const t of texts) {
        const key = slot(t.part, t.checkout_id);
        if (t.text.trim()) k.approved[key] = t.text.trim() + "\n";
        else delete k.approved[key];
        delete k.drafts[key];
        delete k.proposals[key];
        for (const at of Object.keys(k.outside)) if (at.split("@")[0] === key) delete k.outside[at];
      }
      k.stale = k.stale.filter((s) => !done.includes(s));
      const later = [ "design", ...checkouts(taskId).map((c) => slot("tasks", c.id)) ];
      for (const key of later) {
        const part = key.startsWith("tasks") ? 2 : 1;
        if (part > first && k.approved[key] && !done.includes(key) && !k.stale.includes(key)) k.stale.push(key);
      }
      return { spec: view(taskId), failed: [] };
    },
    put_back_spec: (a) => {
      const k = get(a.taskId as string);
      delete k.outside[`${slot(a.part as SpecPart, a.checkoutId as string)}@${a.checkoutId}`];
      return view(a.taskId as string);
    },
    turn_down_spec_proposal: (a) => {
      const k = get(a.taskId as string);
      const key = slot(a.part as SpecPart, (a.checkoutId as string | null) ?? null);
      delete k.drafts[key];
      delete k.proposals[key];
      return view(a.taskId as string);
    },
    draft_spec: async (a) => {
      const taskId = a.taskId as string;
      const k = get(taskId);
      const part = a.part as SpecPart;
      if (a.kind) k.kind = a.kind as SpecKind;
      const cs = checkouts(taskId);
      if (part === "tasks") {
        for (const c of cs) k.drafts[slot("tasks", c.id)] = await stream(a.requestId, part, c.id, DRAFT.tasks(c.project_name));
      } else {
        k.drafts[part] = await stream(a.requestId, part, null, DRAFT[part](cs.map((c) => c.project_name).join(", ")));
      }
      return null;
    },
    merge_specs: async (a) => {
      const taskId = a.taskId as string;
      const k = get(taskId);
      k.drafts.requirements = await stream(a.requestId, "requirements", null, API_REQUIREMENTS.replace("instead of looping.", "instead of looping. (was web R-1)"), 25);
      k.drafts.design = await stream(a.requestId, "design", null, API_DESIGN, 25);
      k.drafts[slot("tasks", "c-login-api")] = await stream(a.requestId, "tasks", "c-login-api", API_TASKS, 25);
      k.drafts[slot("tasks", "c-login-web")] = await stream(a.requestId, "tasks", "c-login-web", WEB_TASKS, 25);
      return null;
    },
    apply_spec_answers: async (a) => {
      const k = get(a.taskId as string);
      const before = k.drafts.requirements ?? k.approved.requirements;
      if (!before || !questions(before).some((q) => q.answer)) throw "no open question has an answer yet";
      const after = applied(before);
      k.drafts.requirements = await stream(a.requestId, "requirements", null, after, 30);
      const old = requirements(before).map((r) => r.id);
      return { changed: [], added: requirements(after).map((r) => r.id).filter((id) => !old.includes(id)), removed: [] };
    },
    check_spec: async (a) => {
      await new Promise((r) => setTimeout(r, 900));
      const taskId = a.taskId as string;
      const folder = folderOf(taskId, a.checkoutId as string);
      const mine = requirements(approved(taskId, "requirements", null) ?? "").filter((r) => r.repos.length === 0 || r.repos.includes(folder));
      const results: SpecVerdict[] = mine.map((r, i) => ({
        id: r.id, verdict: i === 0 ? "met" : "unclear", evidence: i === 0 ? "src/auth/session.ts:41" : "needs a run to tell",
      }));
      const check = { sha: "e5f6a7b8", at: Date.now(), results, stale: false };
      get(taskId).checks[a.checkoutId as string] = check;
      return check;
    },
    spec_work_prompt: (a) => `Work through the steps in the approved tasks.md, and call spec_task with task id \`${a.taskId}\` as soon as each is done.`,
    tell_spec_change: (a) => world.panes.filter((p) => p.task_id === a.taskId && p.running && p.kind === "agent").length,
    set_project_specs: (a) => {
      const p = world.projects.find((x) => x.id === a.projectId);
      if (p) {
        p.specs_in_app = a.inApp as boolean;
        p.spec_folder = (a.folder as string) === "specs" ? null : (a.folder as string);
      }
      return null;
    },
  };
}
