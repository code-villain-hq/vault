import { describe, expect, test } from "bun:test";
import { sortAgents } from "../src/lib/agentSort";
import type { PaneInfo, Task } from "../src/lib/types";

const at = (minute: number) => new Date(Date.UTC(2026, 10, 11, 12, minute)).toISOString();

function pane(id: string, over: Partial<PaneInfo> = {}): PaneInfo {
  return {
    id,
    task_id: "t1",
    checkout_id: null,
    kind: "agent",
    title: id,
    agent_id: "claude",
    cwd: "/tmp",
    running: true,
    exit_code: null,
    started_at: at(0),
    last_output_at: at(0),
    notice: null,
    activity: "idle",
    activity_since: at(0),
    topic: null,
    acp: false,
    ...over,
  };
}

const tasks = [
  { id: "t1", name: "ACME-2 Beta" },
  { id: "t2", name: "ACME-10 Alpha" },
] as Task[];

describe("the All agents view's order (PANE-17)", () => {
  test("latest activity puts agents at work first, then the most recently active", () => {
    const panes = [
      pane("idle-early", { activity: "idle", activity_since: at(5) }),
      pane("working-long", { activity: "working", activity_since: at(1) }),
      pane("done-late", { activity: "done", activity_since: at(30) }),
      pane("exited", { running: false, activity: "idle", activity_since: at(40) }),
      pane("working-new", { activity: "working", activity_since: at(20) }),
      pane("shell", { kind: "shell", activity_since: at(59) }),
    ];
    expect(sortAgents(panes, tasks, "activity")).toEqual([
      "working-new",
      "working-long",
      "exited",
      "done-late",
      "idle-early",
    ]);
  });

  test("needs you first is the order it always had, ties kept as they came", () => {
    const panes = [
      pane("a"),
      pane("asking", { activity: "asking" }),
      pane("b"),
      pane("done", { activity: "done" }),
      pane("chat-done", { task_id: "chat", activity: "done" }),
    ];
    expect(sortAgents(panes, tasks, "needs")).toEqual(["asking", "done", "a", "b", "chat-done"]);
  });

  test("by task sorts names as people read numbers, chats last, newest agent first within one", () => {
    const panes = [
      pane("chat", { task_id: "chat", started_at: at(50) }),
      pane("beta-old", { task_id: "t1", started_at: at(1) }),
      pane("alpha", { task_id: "t2", started_at: at(2) }),
      pane("beta-new", { task_id: "t1", started_at: at(9) }),
    ];
    // ACME-2 before ACME-10.
    expect(sortAgents(panes, tasks, "task")).toEqual(["beta-new", "beta-old", "alpha", "chat"]);
  });

  test("newest first is by when each agent started", () => {
    const panes = [pane("old", { started_at: at(1) }), pane("new", { started_at: at(9) })];
    expect(sortAgents(panes, tasks, "started")).toEqual(["new", "old"]);
  });
});
