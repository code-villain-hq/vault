import { afterEach, expect, test } from "bun:test";
import { SCENARIOS } from "../src/__mock__/world";
import { api } from "../src/lib/api";
import type { CheckoutPr, ReviewQueue, TaskPrs } from "../src/lib/types";
import { useStore } from "../src/store";

const savedState = useStore.getState;
const realTaskRead = api.githubTaskPrs;
const realSweep = api.githubAllPrs;
const realAddMessage = api.addMessage;
const realSettings = api.getSettings;
const realQueue = api.githubReviewQueue;
const { settings, prs, reviews } = SCENARIOS.busy();
const baseRow = prs[0].rows[0];
const oldRows: CheckoutPr[] = [baseRow];
const newRows: CheckoutPr[] = [
  {
    ...baseRow,
    pr: {
      ...baseRow.pr!,
      number: 43,
      head_sha: "a".repeat(40),
      url: "https://github.com/acme/api/pull/43",
    },
    checks: [{ name: "build", status: "completed", conclusion: "success", url: null }],
  },
];
const taskReads = () => {
  const older = Promise.withResolvers<CheckoutPr[]>();
  const newer = Promise.withResolvers<CheckoutPr[]>();
  const replies = [older.promise, newer.promise];
  api.githubTaskPrs = () => replies.shift()!;
  return { older, newer };
};

let before = savedState();
function start() {
  before = savedState();
  useStore.setState({
    settings: { ...settings, tracker: null },
    prs: {},
    prReadRevision: {},
    reviewQueue: null,
    reviewQueueError: null,
    reviewQueueLoading: false,
    failure: null,
  });
  return savedState();
}

afterEach(() => {
  api.githubTaskPrs = realTaskRead;
  api.githubAllPrs = realSweep;
  api.addMessage = realAddMessage;
  api.getSettings = realSettings;
  api.githubReviewQueue = realQueue;
  useStore.setState(before, true);
});

test("a late task refresh cannot restore the previous PR and its checks", async () => {
  const { older, newer } = taskReads();
  const store = start();
  const first = store.refreshTaskPrs("two-refreshes");
  const second = store.refreshTaskPrs("two-refreshes");
  newer.resolve(newRows);
  await second;
  older.resolve(oldRows);
  await first;
  expect(savedState().prs["two-refreshes"]).toEqual(newRows);
});

test("an older request cannot fill the panel when the newer read failed", async () => {
  const { older, newer } = taskReads();
  const store = start();
  const first = store.refreshTaskPrs("refresh-failure");
  const second = store.refreshTaskPrs("refresh-failure");
  newer.reject(new Error("unavailable"));
  await expect(second).rejects.toThrow("unavailable");
  older.resolve(oldRows);
  await first;
  expect(savedState().prs["refresh-failure"]).toBeUndefined();
});

test("an older error does not replace a newer success", async () => {
  const { older, newer } = taskReads();
  const store = start();
  const first = store.refreshTaskPrs("refresh-success");
  const second = store.refreshTaskPrs("refresh-success");
  newer.resolve(newRows);
  await second;
  older.reject(new Error("unavailable"));
  await first;
  expect(savedState().prs["refresh-success"]).toEqual(newRows);
});

test("a refresh of one task does not suppress a read for another", async () => {
  const { older, newer } = taskReads();
  const store = start();
  const first = store.refreshTaskPrs("task-first");
  const second = store.refreshTaskPrs("task-second");
  newer.resolve(newRows);
  await second;
  older.resolve(oldRows);
  await first;
  expect(savedState().prs).toEqual({ "task-first": oldRows, "task-second": newRows });
});

test("a late sweep does not put back an older task snapshot during Refresh", async () => {
  const sweep = Promise.withResolvers<TaskPrs[]>();
  const task = Promise.withResolvers<CheckoutPr[]>();
  api.githubAllPrs = () => sweep.promise;
  api.githubTaskPrs = () => task.promise;
  const store = start();
  const first = store.refreshPrs();
  const second = store.refreshTaskPrs("sweep-first");
  sweep.resolve([{ task_id: "sweep-first", rows: oldRows, ticket: null }]);
  await first;
  const during = savedState().prs["sweep-first"];
  task.resolve(newRows);
  await second;
  expect(during).toBeUndefined();
  expect(savedState().prs["sweep-first"]).toEqual(newRows);
});

test("a task refresh cannot overwrite a newer sweep that already applied", async () => {
  const task = Promise.withResolvers<CheckoutPr[]>();
  api.githubTaskPrs = () => task.promise;
  api.githubAllPrs = async () => [{ task_id: "task-read-first", rows: newRows, ticket: null }];
  const store = start();
  const first = store.refreshTaskPrs("task-read-first");
  await store.refreshPrs();
  task.resolve(oldRows);
  await first;
  expect(savedState().prs["task-read-first"]).toEqual(newRows);
});

test("a sweep that overlaps an accepted refresh cannot restore old rows but still reports tickets", async () => {
  const id = "accepted-while-sweeping";
  const sweep = Promise.withResolvers<TaskPrs[]>();
  const task = Promise.withResolvers<CheckoutPr[]>();
  api.githubAllPrs = () => sweep.promise;
  api.githubTaskPrs = () => task.promise;
  api.addMessage = async () => 1;
  const store = start();
  const refresh = store.refreshTaskPrs(id);
  const background = store.refreshPrs();
  task.resolve(newRows);
  await refresh;
  expect(savedState().prs[id]).toEqual(newRows);
  sweep.resolve([
    {
      task_id: id,
      rows: oldRows,
      ticket: {
        key: "ACME-7",
        stage: "review",
        moved_to: null,
        unchosen: false,
        error: "Ticket move refused.",
      },
    },
    { task_id: "swept-other-task", rows: oldRows, ticket: null },
  ]);
  await background;
  expect(savedState().prs[id]).toEqual(newRows);
  expect(savedState().prs["swept-other-task"]).toEqual(oldRows);
  expect(savedState().failure?.text).toBe("Ticket move refused.");
});

test("an unchanged sweep after a failed refresh advances the accepted revision but reuses rows", async () => {
  const id = "unchanged-sweep";
  api.githubTaskPrs = async () => structuredClone(oldRows);
  api.githubAllPrs = async () => [{ task_id: id, rows: structuredClone(oldRows), ticket: null }];
  const store = start();
  await store.refreshTaskPrs(id);
  const { prs, prReadRevision } = savedState();
  api.githubTaskPrs = async () => {
    throw new Error("unavailable");
  };
  await expect(store.refreshTaskPrs(id)).rejects.toThrow("unavailable");
  expect(savedState().prReadRevision[id]).toBe(prReadRevision[id]);
  await store.refreshPrs();
  expect(savedState().prs[id]).toBe(prs[id]);
  expect(savedState().prReadRevision[id]).toBeGreaterThan(prReadRevision[id]);
});

test.each(["api_url", "web_url"] as const)(
  "changing GitHub %s drops snapshots and old reads",
  async (host) => {
    const store = start();
    const oldManual = Promise.withResolvers<CheckoutPr[]>();
    const oldManualError = Promise.withResolvers<CheckoutPr[]>();
    const oldSweep = Promise.withResolvers<TaskPrs[]>();
    const oldQueue = Promise.withResolvers<ReviewQueue>();
    api.githubTaskPrs = async (id) => {
      if (id === "old-manual") return oldManual.promise;
      if (id === "old-manual-error") return oldManualError.promise;
      return oldRows;
    };
    api.githubAllPrs = () => oldSweep.promise;
    const empty: ReviewQueue = { mine: [], mine_more: false, team: null, authored: null };
    api.githubReviewQueue = async () => reviews;
    api.addMessage = async () => 1;
    await store.refreshTaskPrs("previous-host");
    await store.refreshReviewQueue();
    api.githubReviewQueue = () => oldQueue.promise;
    const manual = store.refreshTaskPrs("old-manual");
    const manualError = store.refreshTaskPrs("old-manual-error");
    const sweep = store.refreshPrs();
    const queue = store.refreshReviewQueue();
    api.getSettings = async () => ({
      ...store.settings!,
      github: { ...store.settings!.github!, [host]: "https://github.example.test" },
    });
    api.githubReviewQueue = async () => empty;
    await store.refreshSettings();
    const switched = savedState();
    oldManual.resolve(oldRows);
    oldManualError.reject(new Error("old task read failed"));
    oldSweep.resolve([
      {
        task_id: "old-sweep",
        rows: oldRows,
        ticket: {
          key: "ACME-7",
          stage: "review",
          moved_to: null,
          unchosen: false,
          error: "old ticket notice",
        },
      },
    ]);
    oldQueue.resolve(reviews);
    const results = await Promise.allSettled([manual, manualError, sweep, queue]);
    expect(switched.prs).toEqual({});
    expect(switched.prReadRevision).toEqual({});
    expect(switched.reviewQueue?.mine).toEqual([]);
    expect(results.map((r) => r.status)).toEqual([
      "fulfilled",
      "fulfilled",
      "fulfilled",
      "fulfilled",
    ]);
    expect(savedState().prs).toEqual({});
    expect(savedState().reviewQueue?.mine).toEqual([]);
    expect(savedState().failure).toBeNull();
  },
);

test("a token-only reconnect starts a new queue read while the old request is in flight", async () => {
  const store = start();
  const oldQueue = Promise.withResolvers<ReviewQueue>();
  const currentQueue = Promise.withResolvers<ReviewQueue>();
  const currentSettings = Promise.withResolvers<typeof settings>();
  api.githubReviewQueue = () => oldQueue.promise;
  api.getSettings = () => currentSettings.promise;
  api.addMessage = async () => 1;
  const first = store.refreshReviewQueue();
  const queued = store.refreshReviewQueue();
  const reconnect = store.refreshSettings({ githubReconnected: true });
  const invalidated = savedState();
  api.githubReviewQueue = () => currentQueue.promise;
  currentSettings.resolve(store.settings!);
  await reconnect;
  const started = savedState();
  const current = store.refreshReviewQueue({ quiet: true });
  oldQueue.reject(new Error("old token refused"));
  await first;
  const loading = savedState();
  currentQueue.resolve(reviews);
  await Promise.all([queued, current]);
  expect(invalidated.reviewQueueLoading).toBe(false);
  expect(started.reviewQueueLoading).toBe(true);
  expect(loading.reviewQueueLoading).toBe(true);
  expect(loading.reviewQueueError).toBeNull();
  expect(loading.failure).toBeNull();
  expect(savedState().reviewQueueLoading).toBe(false);
  expect(savedState().reviewQueue).toEqual(reviews);
});

test("disconnect drops the queue and ignores an outstanding queue error", async () => {
  const store = start();
  const read = Promise.withResolvers<ReviewQueue>();
  api.getSettings = async () => ({ ...store.settings!, github_connected: false, github: null });
  api.githubReviewQueue = () => read.promise;
  api.addMessage = async () => 1;
  const pending = store.refreshReviewQueue();
  await store.refreshSettings();
  const disconnected = savedState();
  read.reject(new Error("disconnected queue error"));
  await pending;
  expect(disconnected.reviewQueueLoading).toBe(false);
  expect(savedState().reviewQueueError).toBeNull();
  expect(savedState().failure).toBeNull();
  expect(savedState().reviewQueue).toBeNull();
});

test("review team metadata keeps task snapshots and their in-flight reads", async () => {
  const store = start();
  const manual = Promise.withResolvers<CheckoutPr[]>();
  api.githubTaskPrs = () => manual.promise;
  api.githubReviewQueue = async () => reviews;
  api.getSettings = async () => ({
    ...store.settings!,
    github: { ...store.settings!.github!, review_team: "acme/backend" },
  });
  const inFlight = store.refreshTaskPrs("team-change-task");
  await store.refreshSettings();
  await store.refreshReviewQueue({ quiet: true });
  manual.resolve(newRows);
  await inFlight;
  expect(savedState().prs["team-change-task"]).toEqual(newRows);
  expect(savedState().githubConnectionRevision).toBe(store.githubConnectionRevision);
});
