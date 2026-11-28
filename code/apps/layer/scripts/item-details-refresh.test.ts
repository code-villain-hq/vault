import { afterEach, expect, test } from "bun:test";
import { SCENARIOS } from "../src/__mock__/world";
import { api } from "../src/lib/api";
import type { ItemDetails, PaneInfo, TaskView, WaitingPane } from "../src/lib/types";
import { useStore } from "../src/store";

const original = { ...api };
afterEach(() => Object.assign(api, original));
const details: ItemDetails = { display_name: "Saved", tags: ["Review"], color: "data-2" };
const task: TaskView = { ...SCENARIOS.busy().tasks[0], id: "t", name: "Original" };
const pane: PaneInfo = {
  ...SCENARIOS.busy().panes[0],
  id: "p",
  task_id: "chat",
  cwd: "/room",
  title: "Original",
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

test("a saved task edit survives a task response requested before the save", async () => {
  useStore.setState({ tasks: [task] });
  const old = deferred<TaskView[]>();
  let reads = 0;
  api.listTasks = () => (reads++ === 0 ? old.promise : Promise.resolve([{ ...task, details }]));
  api.setTaskDetails = async () => details;
  const refresh = useStore.getState().refreshTasks();
  const save = useStore.getState().saveTaskDetails("t", { display_name: "Saved" });
  old.resolve([task]);
  await Promise.all([refresh, save]);
  expect(useStore.getState().tasks[0].details).toEqual(details);
});

test("refreshAll cannot restore stale task or pane details after saves", async () => {
  useStore.setState({ tasks: [task], panes: [pane] });
  const oldTasks = deferred<TaskView[]>();
  const oldPanes = deferred<PaneInfo[]>();
  let taskReads = 0;
  let paneReads = 0;
  api.listTasks = () =>
    taskReads++ === 0 ? oldTasks.promise : Promise.resolve([{ ...task, details }]);
  api.listPanes = () =>
    paneReads++ === 0 ? oldPanes.promise : Promise.resolve([{ ...pane, details }]);
  api.waitingPanes = async () => [];
  api.listProjects = async () => [];
  api.listAgents = async () => [];
  api.cursorIdeInstalled = async () => false;
  api.getSettings = async () => SCENARIOS.empty().settings;
  api.setTaskDetails = async () => details;
  api.setChatDetails = async () => details;
  const refresh = useStore.getState().refreshAll();
  const saveTask = useStore.getState().saveTaskDetails("t", { display_name: "Saved" });
  const saveChat = useStore.getState().saveChatDetails("/room", { display_name: "Saved" });
  oldTasks.resolve([task]);
  oldPanes.resolve([pane]);
  await Promise.all([refresh, saveTask, saveChat]);
  expect(useStore.getState().tasks[0].details).toEqual(details);
  expect(useStore.getState().panes[0].details).toEqual(details);
});

test("waiting and open pane responses cannot undo a saved room edit", async () => {
  useStore.setState({ panes: [pane], waiting: [{ ...pane, id: "waiting" }] });
  const open = deferred<PaneInfo[]>();
  const waiting = deferred<WaitingPane[]>();
  let paneReads = 0;
  let waitingReads = 0;
  api.listPanes = () =>
    paneReads++ === 0 ? open.promise : Promise.resolve([{ ...pane, details }]);
  api.waitingPanes = () =>
    waitingReads++ === 0 ? waiting.promise : Promise.resolve([{ ...pane, id: "waiting", details }]);
  api.setChatDetails = async () => details;
  const before = [useStore.getState().refreshPanes(), useStore.getState().refreshWaiting()];
  const save = useStore.getState().saveChatDetails("/room", { tags: ["Review"] });
  open.resolve([pane]);
  waiting.resolve([{ ...pane, id: "waiting" }]);
  await Promise.all([...before, save]);
  expect(useStore.getState().panes[0].details).toEqual(details);
  expect(useStore.getState().waiting[0].details).toEqual(details);
});

test("pane reuse observes details and keeps identical rows", async () => {
  useStore.setState({ panes: [pane] });
  api.listPanes = async () => [{ ...pane, details }];
  await useStore.getState().refreshPanes();
  const held = useStore.getState().panes;
  expect(held[0].details).toEqual(details);
  await useStore.getState().refreshPanes();
  expect(useStore.getState().panes).toBe(held);
});

test("a rejected save preserves visible details and rejects to the editor", async () => {
  useStore.setState({ tasks: [task] });
  api.setTaskDetails = async () => {
    throw new Error("Disk full");
  };
  await expect(useStore.getState().saveTaskDetails("t", { tags: ["New"] })).rejects.toThrow(
    "Disk full",
  );
  expect(useStore.getState().tasks[0]).toBe(task);
});

test("catalog saves survive an older catalog read and reuse canonical spelling", async () => {
  useStore.setState({ tags: [] });
  const old = deferred<string[]>();
  api.listTags = () => old.promise;
  api.addTag = async () => "Review";
  const refresh = useStore.getState().refreshTags();
  expect(await useStore.getState().addTag(" review ")).toBe("Review");
  old.resolve([]);
  await refresh;
  expect(useStore.getState().tags).toEqual(["Review"]);
});

test("task events refresh tasks and the catalog without fetching chat lists", async () => {
  const calls: string[] = [];
  api.listTasks = async () => {
    calls.push("tasks");
    return [{ ...task, details }];
  };
  api.listTags = async () => {
    calls.push("tags");
    return ["Review"];
  };
  api.listPanes = async () => {
    throw new Error("Unrelated pane refresh");
  };
  api.waitingPanes = async () => {
    throw new Error("Unrelated waiting refresh");
  };
  await useStore.getState().detailsChanged({ kind: "task", catalog_changed: true });
  expect(calls.sort()).toEqual(["tags", "tasks"]);
  expect(useStore.getState().tasks[0].details).toEqual(details);
  expect(useStore.getState().tags).toEqual(["Review"]);
});

test("chat events refresh open and waiting room details without fetching tasks", async () => {
  const calls: string[] = [];
  api.listPanes = async () => {
    calls.push("panes");
    return [{ ...pane, details }];
  };
  api.waitingPanes = async () => {
    calls.push("waiting");
    return [{ ...pane, id: "waiting", details }];
  };
  api.listTasks = async () => {
    throw new Error("Unrelated task refresh");
  };
  api.listTags = async () => {
    throw new Error("Unchanged catalog refresh");
  };
  await useStore.getState().detailsChanged({ kind: "chat", catalog_changed: false });
  expect(calls.sort()).toEqual(["panes", "waiting"]);
  expect(useStore.getState().panes[0].details).toEqual(details);
  expect(useStore.getState().waiting[0].details).toEqual(details);
});

for (const kind of ["task", "chat"] as const) {
  for (const applied of [true, false]) {
    test(`an older ${kind} save reply cannot supersede a newer writer's ${applied ? "applied" : "pending"} event refresh`, async () => {
      useStore.setState({ tasks: [task], panes: [pane], waiting: [{ ...pane, id: "waiting" }] });
      const replyA = deferred<ItemDetails>();
      const readTaskB = deferred<TaskView[]>();
      const readPaneB = deferred<PaneInfo[]>();
      const readWaitingB = deferred<WaitingPane[]>();
      const b: ItemDetails = { display_name: "Writer B", tags: [], color: "data-4" };
      let taskReads = 0;
      let paneReads = 0;
      let waitingReads = 0;
      api.setTaskDetails = () => replyA.promise;
      api.setChatDetails = () => replyA.promise;
      api.listTasks = () =>
        taskReads++ === 0 ? readTaskB.promise : Promise.resolve([{ ...task, details: b }]);
      api.listPanes = () =>
        paneReads++ === 0 ? readPaneB.promise : Promise.resolve([{ ...pane, details: b }]);
      api.waitingPanes = () =>
        waitingReads++ === 0
          ? readWaitingB.promise
          : Promise.resolve([{ ...pane, id: "waiting", details: b }]);
      const save =
        kind === "task"
          ? useStore.getState().saveTaskDetails("t", { display_name: "Writer A" })
          : useStore.getState().saveChatDetails("/room", { display_name: "Writer A" });
      // A persisted first, but its IPC reply is delayed until after B's notification.
      const event = useStore.getState().detailsChanged({ kind, catalog_changed: false });
      const releaseB = () => {
        readTaskB.resolve([{ ...task, details: b }]);
        readPaneB.resolve([{ ...pane, details: b }]);
        readWaitingB.resolve([{ ...pane, id: "waiting", details: b }]);
      };
      if (applied) {
        releaseB();
        await event;
      }
      const visibleNames: (string | null | undefined)[] = [];
      const unsubscribe = useStore.subscribe((s) => {
        if (kind === "task") visibleNames.push(s.tasks[0]?.details?.display_name);
        else
          visibleNames.push(s.panes[0]?.details?.display_name, s.waiting[0]?.details?.display_name);
      });
      replyA.resolve({ display_name: "Writer A", tags: [], color: null });
      if (!applied) releaseB();
      await Promise.all([save, event]);
      unsubscribe();
      expect(visibleNames).not.toContain("Writer A");
      if (kind === "task") expect(useStore.getState().tasks[0].details).toEqual(b);
      else {
        expect(useStore.getState().panes[0].details).toEqual(b);
        expect(useStore.getState().waiting[0].details).toEqual(b);
      }
    });
  }
}

test("an older save reply does not drop an independent writer's catalog refresh", async () => {
  useStore.setState({ tasks: [task], tags: [] });
  const replyA = deferred<ItemDetails>();
  const catalogB = deferred<string[]>();
  api.setTaskDetails = () => replyA.promise;
  api.listTasks = async () => [{ ...task, details }];
  api.listTags = () => catalogB.promise;
  const save = useStore.getState().saveTaskDetails("t", { tags: ["Review"] });
  const event = useStore.getState().detailsChanged({ kind: "catalog", catalog_changed: true });
  replyA.resolve(details);
  await save;
  catalogB.resolve(["Review", "Writer B tag"]);
  await event;
  expect(useStore.getState().tags).toEqual(["Review", "Writer B tag"]);
});

test("a post-save read failure does not turn a durable mutation into a rejected save", async () => {
  useStore.setState({ tasks: [task], failure: null });
  api.setTaskDetails = async () => details;
  api.listTasks = async () => {
    throw new Error("Reconciliation read failed");
  };
  api.addMessage = async () => 1;
  await expect(
    useStore.getState().saveTaskDetails("t", { display_name: "Saved" }),
  ).resolves.toEqual(details);
  expect(useStore.getState().failure?.text ?? "").toContain("Reconciliation read failed");
});

test("a deleted tag leaves the catalog, and an older read cannot bring it back (SET-15)", async () => {
  useStore.setState({ tasks: [task], panes: [pane], tags: ["Review", "Bug"] });
  const older = deferred<string[]>();
  api.listTags = () => older.promise;
  const stale = useStore.getState().refreshTags();
  api.deleteTag = async () => undefined;
  api.listTags = async () => ["Bug"];
  api.listTasks = async () => [task];
  api.listPanes = async () => [pane];
  api.waitingPanes = async () => [];
  await useStore.getState().deleteTag("Review");
  expect(useStore.getState().tags).toEqual(["Bug"]);
  older.resolve(["Review", "Bug"]);
  await stale;
  expect(useStore.getState().tags).toEqual(["Bug"]);
});

test("a renamed tag reloads the tasks and chats that carry it (SET-15)", async () => {
  useStore.setState({ tasks: [task], panes: [pane], tags: ["Review"] });
  const renamed = { ...details, tags: ["Needs review"] };
  api.renameTag = async () => "Needs review";
  api.listTags = async () => ["Needs review"];
  api.listTasks = async () => [{ ...task, details: renamed }];
  api.listPanes = async () => [{ ...pane, details: renamed }];
  api.waitingPanes = async () => [];
  expect(await useStore.getState().renameTag("review", "Needs review")).toBe("Needs review");
  expect(useStore.getState().tags).toEqual(["Needs review"]);
  expect(useStore.getState().tasks[0].details?.tags).toEqual(["Needs review"]);
  expect(useStore.getState().panes[0].details?.tags).toEqual(["Needs review"]);
});
