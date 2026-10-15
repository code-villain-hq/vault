import { describe, expect, test } from "bun:test";
import { exitIsNews, markStopping } from "../src/store";
import type { PaneInfo } from "../src/lib/types";

function pane(id: string, over: Partial<PaneInfo> = {}): PaneInfo {
  return {
    id, task_id: "t", checkout_id: null, kind: "agent", title: "Claude Code", agent_id: "claude",
    cwd: "/tmp", running: false, exit_code: 143, started_at: "", last_output_at: "",
    notice: null, activity: "idle", activity_since: "", topic: null, acp: false, ...over,
  };
}

describe("a pane's exit", () => {
  test("is news for a task's agent that nobody stopped", () => {
    expect(exitIsNews("a", pane("a"))).toBe(true);
  });

  test("is not news when the agent was stopped, and only that once", () => {
    markStopping("b");
    expect(exitIsNews("b", pane("b"))).toBe(false);
    expect(exitIsNews("b", pane("b"))).toBe(true);
  });

  test("clears the stop mark of a chat, a shell, or a pane already gone", () => {
    for (const [id, p] of [
      ["chat", pane("chat", { task_id: "chat" })],
      ["shell", pane("shell", { kind: "shell" })],
      ["gone", undefined],
    ] as const) {
      markStopping(id);
      expect(exitIsNews(id, p)).toBe(false);
      // The same id as a task's agent: nothing left over says it was stopped.
      expect(exitIsNews(id, pane(id))).toBe(true);
    }
  });
});
