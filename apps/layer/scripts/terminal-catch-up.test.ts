import { describe, expect, test } from "bun:test";
import { whenBehind } from "../src/lib/catchUp";

describe("a terminal further behind than its scrollback (PANE-8)", () => {
  test("a full-screen program keeps its screen and is made to draw it all again", () => {
    expect(whenBehind("alternate")).toEqual({ wipe: false, repaint: true });
  });

  test("a line-by-line program is wiped and replayed, as before", () => {
    expect(whenBehind("normal")).toEqual({ wipe: true, repaint: false });
  });
});
