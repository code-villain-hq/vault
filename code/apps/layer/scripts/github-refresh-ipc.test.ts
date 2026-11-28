import { afterEach, expect, test } from "bun:test";
import { mockIPC } from "@tauri-apps/api/mocks";
import { SCENARIOS } from "../src/__mock__/world";
import type { ReviewQueue, TaskPrs } from "../src/lib/types";
import { useStore } from "../src/store";

const { settings, tasks, prs, reviews } = SCENARIOS.busy();
const priorSettings = { ...settings, tracker: null };
const before = useStore.getState();
const originalWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
const get = useStore.getState;
const mergedRow: TaskPrs = {
  ...prs[0],
  rows: prs[0].rows.map((row) => ({
    ...row,
    pr: row.pr && { ...row.pr, state: "closed", merged: true },
  })),
};
const newSettings = {
  ...priorSettings,
  github_sources: settings.github_sources.map((g) =>
    g.key === "github.com" ? { ...g, review_team: "acme/backend" } : g,
  ),
};
const newQueue: ReviewQueue = {
  sources: reviews.sources.map((source) =>
    source.source === "github.com"
      ? {
          ...source,
          team: { slug: "acme/backend", name: "Backend", prs: [], more: false, error: null },
        }
      : source,
  ),
};

function ipc(answers: Record<string, () => unknown>) {
  Object.defineProperty(globalThis, "window", { configurable: true, value: {} });
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (!answers[cmd]) throw new Error(`Unexpected IPC: ${cmd}`);
    return answers[cmd]();
  });
  useStore.setState({
    settings: priorSettings,
    tasks,
    prs: {},
    prReadRevision: {},
    reviewQueue: null,
    reviewQueueLoading: false,
    reviewQueueError: null,
    failure: null,
  });
  return calls;
}

afterEach(() => {
  if (originalWindow) Object.defineProperty(globalThis, "window", originalWindow);
  else Reflect.deleteProperty(globalThis, "window");
  useStore.setState(before, true);
});

test("a reconnect starts a fresh sweep while the old sweep and its queued successor are pending", async () => {
  const old = Promise.withResolvers<TaskPrs[]>();
  const current = Promise.withResolvers<TaskPrs[]>();
  const replies = [old.promise, current.promise];
  const calls = ipc({
    github_task_prs: () => prs[0].rows,
    github_all_prs: () => replies.shift() ?? [],
    github_review_queue: () => reviews,
    get_settings: () => priorSettings,
    add_message: () => 1,
  });
  const id = mergedRow.task_id;
  await get().refreshTaskPrs(id);
  const oldRun = get().refreshPrs();
  const superseded = get().refreshPrs();
  await get().refreshSettings({ githubReconnected: true });
  const currentRun = get().refreshPrs();
  const sweeps = () => calls.filter(({ cmd }) => cmd === "github_all_prs").map(({ args }) => args);
  const beforeOldCompletes = sweeps();
  old.resolve([
    {
      ...prs[0],
      ticket: {
        key: "ACME-7",
        stage: "review",
        moved_to: null,
        unchosen: false,
        error: "Old ticket",
      },
    },
  ]);
  // Let the old IPC settle, without waiting for a possibly mis-scheduled sweep.
  await Bun.sleep(0);
  const overlap1 = get().refreshPrs();
  const overlap2 = get().refreshPrs();
  const beforeCurrentCompletes = sweeps();
  current.resolve([mergedRow]);
  await Promise.all([oldRun, superseded, currentRun, overlap1, overlap2]);

  expect(beforeOldCompletes).toEqual([{ knownTaskIds: [id] }, { knownTaskIds: [] }]);
  expect(beforeCurrentCompletes).toEqual(beforeOldCompletes);
  expect(sweeps()).toEqual([...beforeOldCompletes, { knownTaskIds: [id] }]);
  expect(get().prs[id]).toEqual(mergedRow.rows);
  expect(get().failure).toBeNull();
  expect(calls.some(({ cmd }) => cmd === "add_message")).toBe(false);
});

test("a sweep sends only held task IDs, and snapshots them before a concurrent task read completes", async () => {
  const sweep = Promise.withResolvers<TaskPrs[]>();
  const calls = ipc({
    github_task_prs: () => prs[0].rows,
    github_all_prs: () => sweep.promise,
  });
  useStore.setState({ prs: { "already-held": [] } });
  const pending = get().refreshPrs();
  await get().refreshTaskPrs("arrived-during-sweep");
  sweep.resolve([mergedRow]);
  await pending;

  expect(calls).toEqual([
    { cmd: "github_all_prs", args: { knownTaskIds: ["already-held"] } },
    { cmd: "github_task_prs", args: { taskId: "arrived-during-sweep" } },
  ]);
  expect(get().prs[mergedRow.task_id]).toEqual(mergedRow.rows);
  expect(get().prs["arrived-during-sweep"]).toEqual(prs[0].rows);
});

test("a confirmed team save starts a new queue before settings reload and drops old manual errors", async () => {
  const old = Promise.withResolvers<ReviewQueue>();
  const current = Promise.withResolvers<ReviewQueue>();
  const reload = Promise.withResolvers<typeof newSettings>();
  const replies = [old.promise, current.promise];
  const calls = ipc({
    github_review_queue: () => replies.shift() ?? reviews,
    get_settings: () => reload.promise,
    add_message: () => 1,
  });
  const held = { [mergedRow.task_id]: mergedRow.rows };
  useStore.setState({ reviewQueue: reviews, prs: held });
  const connection = get().githubConnectionRevision;
  const oldManual = get().refreshReviewQueue();
  const oldQueued = get().refreshReviewQueue();
  // Code Settings already got a successful response to the team-only save.
  const saved = {
    githubReconnected: false,
    githubReviewTeam: { source: "github.com", team: "acme/backend" },
  };
  const settingsRead = get().refreshSettings(saved);
  const started = get();
  const callsBeforeReload = calls.slice();
  current.resolve(newQueue);
  await Bun.sleep(0);
  const beforeOldCompletes = get().reviewQueue;
  old.reject(new Error("The GitHub connection changed. Try again."));
  await Promise.all([oldManual, oldQueued]);
  const duringReload = get();
  reload.resolve(newSettings);
  await settingsRead;

  expect(callsBeforeReload).toEqual([
    { cmd: "github_review_queue", args: {} },
    { cmd: "github_review_queue", args: {} },
    { cmd: "get_settings", args: {} },
  ]);
  expect(calls).toEqual(callsBeforeReload);
  expect(started.reviewQueue).toBe(reviews);
  expect(started.reviewQueueLoading).toBe(false);
  expect(beforeOldCompletes).toEqual(newQueue);
  expect(duringReload.reviewQueue).toEqual(newQueue);
  expect(duringReload.reviewQueueError).toBeNull();
  expect(duringReload.failure).toBeNull();
  expect(get().reviewQueue).toEqual(newQueue);
  expect(get().reviewQueueError).toBeNull();
  expect(get().reviewQueueLoading).toBe(false);
  expect(get().failure).toBeNull();
  expect(get().githubConnectionRevision).toBe(connection);
  expect(get().prs).toBe(held);
});

test("a team change discovered on reload starts a new queue and keeps its loading state", async () => {
  const old = Promise.withResolvers<ReviewQueue>();
  const current = Promise.withResolvers<ReviewQueue>();
  const replies = [old.promise, current.promise];
  const calls = ipc({
    github_review_queue: () => replies.shift() ?? reviews,
    get_settings: () => newSettings,
    add_message: () => 1,
  });
  const oldManual = get().refreshReviewQueue();
  const oldQueued = get().refreshReviewQueue();
  await get().refreshSettings();
  const callsBeforeOldCompletes = calls.slice();
  old.reject(new Error("Superseded team"));
  await oldManual;
  await Bun.sleep(0);
  const during = get();
  current.resolve(newQueue);
  await Promise.all([oldQueued, get().refreshReviewQueue({ quiet: true })]);

  expect(callsBeforeOldCompletes).toEqual([
    { cmd: "github_review_queue", args: {} },
    { cmd: "get_settings", args: {} },
    { cmd: "github_review_queue", args: {} },
  ]);
  expect(calls).toEqual(callsBeforeOldCompletes);
  expect(during.reviewQueueLoading).toBe(true);
  expect(during.reviewQueueError).toBeNull();
  expect(during.failure).toBeNull();
  expect(get().reviewQueue).toEqual(newQueue);
  expect(get().reviewQueueLoading).toBe(false);
});
