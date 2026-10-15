import { describe, expect, test } from "bun:test";
import { heldConversation, holdConversation, pruneConversations } from "../src/lib/acpPool";
import type { AcpEntry, AcpView } from "../src/lib/types";

const say = (index: number, text: string, rev = index): AcpEntry => ({ index, rev, kind: "agent", text });
const view = (rev: number, entries: AcpEntry[], reset = false, base = 0): AcpView => ({
  rev, base, len: base + entries.length, reset, entries,
  agent: "Agent 1.0", ready: true, busy: false, queued: 0, images: false,
  settings: [], commands: [], usage: null, exited: false,
});

describe("a conversation kept while its pane is off screen (ACP-7)", () => {
  test("a pane coming back starts from what it had, and asks from its revision", () => {
    holdConversation("back", view(2, [say(0, "a"), say(1, "b")], true));
    const held = heldConversation("back");
    expect(held.rev).toBe(2);
    expect(held.entries.map((e) => e.index)).toEqual([0, 1]);
    expect(held.view?.agent).toBe("Agent 1.0");
  });

  test("changes since that revision merge into what was kept, untouched entries staying the same objects", () => {
    const before = holdConversation("merge", view(1, [say(0, "a"), say(1, "Lo")], true)).entries;
    const after = holdConversation("merge", view(3, [say(1, "Look", 3)])).entries;
    expect(after[0]).toBe(before[0]);
    expect(after.map((e) => e.kind === "agent" && e.text)).toEqual(["a", "Look"]);
    expect(heldConversation("merge").rev).toBe(3);
  });

  test("a reset from the backend replaces what was kept", () => {
    holdConversation("reset", view(9, [say(0, "old"), say(1, "older")], true));
    const held = holdConversation("reset", view(2, [say(0, "new")], true));
    expect(held.entries.map((e) => e.kind === "agent" && e.text)).toEqual(["new"]);
    expect(held.rev).toBe(2);
  });

  test("the kept view does not hold a second copy of the entries", () => {
    expect(holdConversation("copy", view(1, [say(0, "a")], true)).view?.entries).toEqual([]);
  });

  test("a closed pane's conversation is forgotten, and an open one's kept", () => {
    holdConversation("open", view(1, [say(0, "a")], true));
    holdConversation("closed", view(1, [say(0, "a")], true));
    pruneConversations(["open"]);
    expect(heldConversation("open").rev).toBe(1);
    expect(heldConversation("closed")).toEqual({ entries: [], rev: null, view: null });
  });

  test("a pane never seen asks from no revision", () => {
    expect(heldConversation("never").rev).toBeNull();
  });
});
