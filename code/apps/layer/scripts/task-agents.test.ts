import { describe, expect, test } from "bun:test";
import { fleetAgents, taskAgents } from "../src/lib/derive";
import type { PaneInfo } from "../src/lib/types";

function pane(task_id: string, over: Partial<PaneInfo> = {}): PaneInfo {
  return {
    id: Math.random().toString(36), task_id, checkout_id: null, kind: "agent", title: "Claude Code",
    agent_id: "claude", cwd: "/tmp", running: true, exit_code: null, started_at: "", last_output_at: "",
    notice: null, activity: "idle", activity_since: "", topic: null, acp: false, ...over,
  };
}

describe("a task's row in the sidebar", () => {
  test("counts its running agents, not its shells or the exited ones", () => {
    const panes = [
      pane("t"), pane("t", { kind: "shell" }), pane("t", { running: false }), pane("other"),
    ];
    expect(taskAgents(panes, "t").live).toBe(1);
  });

  test("is amber when an agent asks or finished, green only when one works", () => {
    expect(taskAgents([pane("t", { activity: "asking" })], "t")).toEqual(
      { live: 1, waiting: true, busy: false, stuck: false },
    );
    expect(taskAgents([pane("t", { activity: "done" })], "t").waiting).toBe(true);
    expect(taskAgents([pane("t", { activity: "working" })], "t")).toEqual(
      { live: 1, waiting: false, busy: true, stuck: false },
    );
    expect(taskAgents([pane("t", { kind: "shell", activity: "working" })], "t").busy).toBe(false);
  });

  test("is red when an agent ran out of budget, but not once it exited", () => {
    expect(taskAgents([pane("t", { notice: "usage_limit" })], "t").stuck).toBe(true);
    expect(taskAgents([pane("t", { notice: "usage_limit", running: false })], "t").stuck).toBe(false);
  });

  test("ignores what agents in other tasks are doing", () => {
    expect(taskAgents([pane("other", { activity: "asking", notice: "usage_limit" })], "t")).toEqual(
      { live: 0, waiting: false, busy: false, stuck: false },
    );
  });
});

describe("the All agents row", () => {
  test("adds up every task's agents, and the chats'", () => {
    const panes = [pane("a"), pane("b", { activity: "asking" }), pane("chat"), pane("a", { kind: "shell" })];
    expect(fleetAgents(panes)).toEqual({ running: 3, waiting: 1 });
  });

  test("does not count a finished chat as needing you", () => {
    expect(fleetAgents([pane("chat", { activity: "done" })]).waiting).toBe(0);
  });
});
