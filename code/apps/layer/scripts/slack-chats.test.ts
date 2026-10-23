import { describe, expect, test } from "bun:test";
import { showsFolded } from "../src/lib/derive";
import type { PaneInfo } from "../src/lib/types";

function chat(id: string, activity: PaneInfo["activity"], running = true): PaneInfo {
  return {
    id, task_id: "chat", checkout_id: null, kind: "agent", title: "Claude Code", agent_id: "claude",
    cwd: `/tmp/_chat/${id}`, running, exit_code: running ? null : 0, started_at: "", last_output_at: "",
    notice: null, activity, activity_since: "", topic: null,
  };
}

describe("the Slack bot's chats under a folded heading (CHAT-5)", () => {
  test("still show the ones at work or asking", () => {
    expect(showsFolded(chat("a", "working"), null)).toBe(true);
    expect(showsFolded(chat("b", "asking"), null)).toBe(true);
  });
  test("hide the ones that answered or sit idle, and the ended ones", () => {
    expect(showsFolded(chat("c", "done"), null)).toBe(false);
    expect(showsFolded(chat("d", "idle"), null)).toBe(false);
    expect(showsFolded(chat("e", "working", false), null)).toBe(false);
  });
  test("keep the one on screen in the list", () => {
    expect(showsFolded(chat("f", "done"), "f")).toBe(true);
  });
});
