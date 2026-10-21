/**
 * The run log (§22) for the mock harness: two weeks of made-up runs over the
 * busy world's tasks, the same every time, with one of each way a run can
 * end. The empty scenario has none.
 */
import { emit } from "@tauri-apps/api/event";
import type { Run, RunEnd, RunLoop, TaskDone } from "../lib/types-runs";

const api = { id: "p-api", name: "api" };
const web = { id: "p-web", name: "web" };
const TASKS = [
  { task_id: "t-login", task: "ACME-123 Fix login race", repos: [api, web] },
  { task_id: "t-audit", task: "ACME-130 Audit log page", repos: [web] },
  { task_id: "t-gone", task: "", repos: [api] },
  { task_id: "t-pages", task: "ACME-101 Paginate the order list", repos: [web] },
  { task_id: "t-retry", task: "ACME-108 Retry the webhook with backoff", repos: [api] },
  { task_id: "t-csv", task: "ACME-112 Export invoices as CSV", repos: [api, web] },
  { task_id: "chat", task: "Chat", repos: [] },
];
const AGENTS = ["claude", "claude", "claude", "copilot", "gemini"];

/** Most runs ask nothing; some wait on you a minute, a few far longer. */
function asked(roll: number): Pick<Run, "asks" | "waited_secs"> {
  if (roll < 0.45) return { asks: 0, waited_secs: 0 };
  const asks = 1 + Math.floor(roll * 6);
  return { asks, waited_secs: Math.floor(asks * (roll < 0.9 ? 40 : 900) * roll) };
}

function tools(calls: number, roll: number): Run["tools"] {
  return { calls, failed: Math.floor(calls * roll * 0.15) };
}

function made(): Run[] {
  // The same runs at every load: a small linear congruential generator.
  let seed = 7;
  const next = () => (seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31;
  const now = Date.now();
  const runs: Run[] = [];
  // Past a page of the list (RUN-3), so "Show more" can be clicked.
  for (let i = 0; i < 146; i++) {
    const ended = now - Math.floor(next() * 14 * 86_400_000) - 60_000;
    const took = Math.floor(60_000 + next() * 50 * 60_000);
    const t = TASKS[Math.floor(next() * TASKS.length)];
    const agent = AGENTS[Math.floor(next() * AGENTS.length)];
    const acp = agent === "claude" && next() < 0.6;
    const roll = next();
    // Claude Code and Copilot say whether they are working; Gemini's title says nothing to time.
    const worked_secs = agent === "gemini" ? null : Math.floor((took / 1000) * (0.15 + next() * 0.6));
    const end: RunEnd = roll < 0.12 ? "failed" : roll < 0.3 ? "exited" : "stopped";
    let loop: RunLoop | null = null;
    if (end === "stopped" && next() < 0.45) {
      const passed = next() < 0.75;
      loop = passed ? { used: Math.floor(next() * 4), rounds: 5, end: "passed" } : { used: 5, rounds: 5, end: "gave_up" };
    }
    runs.push({
      id: `run-${i}`,
      agent,
      ...t,
      acp,
      started_at: new Date(ended - took).toISOString(),
      ended_at: new Date(ended).toISOString(),
      end,
      code: end === "stopped" ? 1 : end === "exited" ? 0 : 2,
      loop,
      // Claude Code says, over ACP or in its transcript; the others do not.
      tokens: agent === "claude"
        ? { input: Math.floor(next() * 4000), output: Math.floor(next() * 60_000), cached_read: Math.floor(next() * 3_000_000), cached_write: Math.floor(next() * 200_000) }
        : null,
      worked_secs,
      turns: 1 + Math.floor(next() * 12),
      ...asked(next()),
      limited: next() < 0.05,
      // Claude Code and Copilot say through their hooks, an agent over ACP itself; Gemini does not.
      tools: agent === "gemini" ? null : tools(Math.floor(next() * 80), next()),
    });
  }
  // A loop stopped by hand, and a run still on its loop when its agent ended.
  runs.push({ ...runs[0], id: "run-loop-stopped", end: "stopped", loop: { used: 2, rounds: 5, end: "stopped" }, ended_at: new Date(now - 90_000).toISOString(), started_at: new Date(now - 1_500_000).toISOString() });
  return runs.sort((a, b) => Date.parse(b.ended_at) - Date.parse(a.ended_at));
}

/** Tasks finished or deleted over the two weeks (RUN-9); t-gone went before they were kept. */
function ended(): TaskDone[] {
  const at = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString();
  return [
    { id: "t-pages", name: "ACME-101 Paginate the order list", issue_key: "ACME-101", repos: [web], created_at: at(13), ended_at: at(9), finished: true },
    { id: "t-retry", name: "ACME-108 Retry the webhook with backoff", issue_key: "ACME-108", repos: [api], created_at: at(8), ended_at: at(6.5), finished: true },
    { id: "t-csv", name: "ACME-112 Export invoices as CSV", issue_key: "ACME-112", repos: [api, web], created_at: at(5), ended_at: at(0.3), finished: true },
    { id: "t-spike", name: "Try the new date picker", issue_key: null, repos: [web], created_at: at(4), ended_at: at(3.8), finished: false },
  ];
}

export function runAnswers(seeded: boolean) {
  let runs = seeded ? made() : [];
  let tasks = seeded ? ended() : [];
  return {
    list_runs: () => structuredClone({ runs, tasks }),
    clear_runs: () => {
      runs = [];
      tasks = [];
      void emit("runs:changed");
      return null;
    },
  };
}
