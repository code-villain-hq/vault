import { describe, expect, test } from "bun:test";
import { pinnedFirst, toggled } from "../src/lib/pins";

const chat = (id: string, cwd: string | null = `/rooms/${id}`) => ({ id, cwd });
const ids = (xs: { id: string }[]) => xs.map((x) => x.id);

describe("pinned chats (CHAT-6)", () => {
  test("are listed first, in the order they were pinned", () => {
    const { pinned, rest } = pinnedFirst(
      [chat("a"), chat("b"), chat("c"), chat("d")],
      ["/rooms/c", "/rooms/a"],
    );
    expect(ids(pinned)).toEqual(["c", "a"]);
    expect(ids(rest)).toEqual(["b", "d"]);
  });
  test("leave the list as it was while nothing is pinned", () => {
    const { pinned, rest } = pinnedFirst([chat("a"), chat("b")], []);
    expect(pinned).toEqual([]);
    expect(ids(rest)).toEqual(["a", "b"]);
  });
  test("are known by room, so a chat restarted under a new id stays pinned", () => {
    const { pinned } = pinnedFirst([{ id: "new-id", cwd: "/rooms/a" }], ["/rooms/a"]);
    expect(ids(pinned)).toEqual(["new-id"]);
  });
  test("never take in something with no room", () => {
    const { pinned, rest } = pinnedFirst([chat("x", null)], ["/rooms/x"]);
    expect(pinned).toEqual([]);
    expect(ids(rest)).toEqual(["x"]);
  });
  test("are pinned at the end and unpinned in place", () => {
    expect(toggled(["/a"], "/b")).toEqual(["/a", "/b"]);
    expect(toggled(["/a", "/b", "/c"], "/b")).toEqual(["/a", "/c"]);
  });
});
