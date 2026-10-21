import { describe, expect, test } from "bun:test";
import { CHAT_LIST_DEFAULT, clampSidebar, sidebarMost } from "../src/lib/drag";

describe("the sidebar's width (TASK-16)", () => {
  test("is never narrower than its own controls need", () => {
    expect(clampSidebar(40, 1600, 1)).toBe(200);
  });

  test("is never wider than 560px, however wide the window", () => {
    expect(clampSidebar(2000, 3000, 1)).toBe(560);
  });

  test("leaves the smallest window at least half for the terminals", () => {
    expect(sidebarMost(900, 1)).toBe(450);
    expect(clampSidebar(520, 900, 1)).toBe(450);
  });

  test("counts in the UI's pixels when the UI is scaled up", () => {
    expect(sidebarMost(900, 1.5)).toBe(300);
  });

  test("falls back to the usual width when what was saved is not a number", () => {
    expect(clampSidebar("wide", 1600, 1)).toBe(260);
    expect(clampSidebar(Number.NaN, 1600, 1)).toBe(260);
    expect(clampSidebar(null, 1600, 1)).toBe(260);
  });
});

describe("the chat list's width (CHAT-4)", () => {
  test("falls back to its own usual width, not the task list's", () => {
    expect(clampSidebar(undefined, 1600, 1, CHAT_LIST_DEFAULT)).toBe(230);
  });

  test("is held to the same bounds as the task list", () => {
    expect(clampSidebar(40, 1600, 1, CHAT_LIST_DEFAULT)).toBe(200);
    expect(clampSidebar(2000, 3000, 1, CHAT_LIST_DEFAULT)).toBe(560);
  });
});
