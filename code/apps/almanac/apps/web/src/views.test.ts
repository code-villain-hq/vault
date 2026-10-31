import { describe, expect, test } from "bun:test";
import type { Block, Point } from "@almanac/types";
import { shownAs, viewOf, viewsFor, widthOf } from "./views";

const block = (kind: string, title = "Notes", facets: string[] = [], view = ""): Block => ({
  id: "b1",
  workspace_id: "w",
  topic_id: "t",
  kind,
  title,
  facets: facets.map((name) => ({ name, icon: "" })),
  fields: [],
  width: "",
  view,
  position: 0,
  created_at: "",
  updated_at: "",
});

let n = 0;
const point = (theme: string, text: string, more: Partial<Point> = {}): Point => ({
  id: `p${++n}`,
  workspace_id: "w",
  topic_id: "t",
  block_id: "b1",
  theme,
  text,
  happened: "",
  facet: "",
  look: "",
  amount: null,
  note: "",
  done: false,
  cells: {},
  source: null,
  source_url: null,
  talk_id: null,
  seen: true,
  position: n,
  created_at: "",
  updated_at: "",
  ...more,
});

describe("the one set", () => {
  test("draws a block as its kind once it has something to show", () => {
    expect(viewOf(block("heading", "Care"), [])).toBe("heading");
    expect(viewOf(block("heading", ""), [])).toBe("");
    expect(viewOf(block("bullets"), [])).toBe("");
    expect(viewOf(block("bullets"), [point("", "Water weekly")])).toBe("bullets");
  });

  test("leaves out a block of a kind from before the set, and keeps You for About", () => {
    expect(viewOf(block("chips"), [point("", "Water weekly")])).toBe("");
    expect(viewOf(block("you", "You"), [])).toBe("you");
  });

  test("gives a heading and a callout the whole row, the rest half", () => {
    expect(widthOf("heading", block("heading", "Care"), [])).toBe("whole");
    expect(widthOf("callout", block("callout"), [])).toBe("whole");
    expect(widthOf("paragraph", block("paragraph"), [])).toBe("1/2");
  });

  test("offers no other views for a block of writing", () => {
    expect(viewsFor(block("checklist"), [point("", "Bring the CV up to date")])).toEqual([]);
  });

  test("draws a collection as the picture its fields pick, and keeps a person's pick", () => {
    const words = {
      ...block("collection", "Words"),
      fields: [{ name: "Means", kind: "text", unit: "", goal: null, options: [] }],
    };
    const rows = [point("", "madrugada", { cells: { Means: "the hours after midnight" } })];
    expect(shownAs(words, rows)).toBe("list");
    expect(shownAs({ ...words, view: "cards" }, rows)).toBe("cards");
    expect(shownAs({ ...words, view: "line" }, rows)).toBe("list");
  });
});
