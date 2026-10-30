import { describe, expect, test } from "bun:test";
import type { Block, Point } from "@almanac/types";
import { viewOf, widthOf } from "./views";

const block = (kind: string, title = "Notes", facets: string[] = []): Block => ({
  id: "b1",
  workspace_id: "w",
  topic_id: "t",
  kind,
  title,
  facets: facets.map((name) => ({ name, icon: "" })),
  width: "",
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
  source: null,
  source_url: null,
  talk_id: null,
  seen: true,
  position: n,
  created_at: "",
  updated_at: "",
  ...more,
});

const sentence = "Eine Farbe für Akzente, nie für Flächen, und nie hinter Text.";
const paragraph = sentence.repeat(4);

describe("viewOf", () => {
  test("draws the page's own blocks as they are", () => {
    expect(viewOf(block("today"), [])).toBe("today");
    expect(viewOf(block("week"), [])).toBe("week");
    expect(viewOf(block("you"), [point("", "Lebt in Berlin")])).toBe("you");
  });

  test("leaves out a block that keeps nothing", () => {
    expect(viewOf(block("cards"), [])).toBe("");
    expect(viewOf(block("table", "Futter", ["Preis"]), [])).toBe("");
  });

  test("keeps steps, questions and a shelf as a talk said", () => {
    expect(viewOf(block("list"), [point("", "Leine anlegen")])).toBe("list");
    expect(viewOf(block("questions"), [point("Warum?", sentence)])).toBe("questions");
    expect(viewOf(block("shelf"), [point("Shampoo", "Mild")])).toBe("shelf");
  });

  test("never keeps a look a talk gave", () => {
    const facts = [point("", "Wien"), point("", "Graz")];
    expect(viewOf(block("cards"), facts)).toBe("chips");
    expect(viewOf(block("text"), facts)).toBe("chips");
    expect(viewOf(block("table"), facts)).toBe("chips");
  });

  test("draws things with the same facts as cards, or a table past eight", () => {
    const things = (count: number) =>
      Array.from({ length: count }, (_, i) => point(`Schuh ${i}`, "89 €", { facet: "Preis" }));
    expect(viewOf(block("table", "Schuhe", ["Preis"]), things(4))).toBe("cards");
    expect(viewOf(block("cards", "Schuhe", ["Preis"]), things(8))).toBe("cards");
    expect(viewOf(block("cards", "Schuhe", ["Preis"]), things(9))).toBe("table");
  });

  test("draws days as a timeline, and days with numbers as a trend", () => {
    expect(viewOf(block("text"), [point("Reform", "Thesen", { happened: "1517" })])).toBe(
      "timeline",
    );
    const weighed = [
      point("Lisa", "4,1 kg", { happened: "2026-10-01", amount: 4.1 }),
      point("Lisa", "4,3 kg", { happened: "2026-10-15", amount: 4.3 }),
    ];
    expect(viewOf(block("cards"), weighed)).toBe("trend");
  });

  test("draws numbers as bars", () => {
    expect(viewOf(block("chips"), [point("Preise", "Felix", { amount: 12 })])).toBe("bars");
  });

  test("draws paragraphs as tiles, one per section", () => {
    expect(viewOf(block("cards"), [point("Markt", paragraph)])).toBe("text");
  });

  test("draws short facts as chips, other sentences as lines", () => {
    expect(viewOf(block("chips"), [point("", "Pfoten warm"), point("", sentence)])).toBe("lines");
    expect(viewOf(block("chips"), [point("", "Pfoten warm"), point("", "Nase feucht")])).toBe(
      "chips",
    );
  });

  test("draws sentences about things as a card each", () => {
    expect(viewOf(block("text"), [point("Linie", sentence), point("Schraffur", sentence)])).toBe(
      "cards",
    );
    expect(viewOf(block("chips", "Reparaturen"), [point("Abfluss im Bad", sentence)])).toBe(
      "cards",
    );
  });

  test("takes a section named as its block for the block itself", () => {
    expect(viewOf(block("cards", "Advice"), [point("Advice", sentence)])).toBe("lines");
  });
});

describe("widthOf", () => {
  test("gives what reads at a glance a third, lines a half, the rest the row", () => {
    const facts = [point("", "Wien")];
    expect(widthOf("chips", block("chips"), facts)).toBe("1/3");
    expect(widthOf("lines", block("chips"), facts)).toBe("1/2");
    expect(widthOf("table", block("table"), facts)).toBe("whole");
    expect(widthOf("week", block("week"), [])).toBe("whole");
  });

  test("gives tiles as many thirds as they need", () => {
    const one = [point("Linie", sentence)];
    const two = [...one, point("Motiv", sentence)];
    const three = [...two, point("Licht", sentence)];
    expect(widthOf("cards", block("cards"), one)).toBe("1/3");
    expect(widthOf("cards", block("cards"), two)).toBe("1/2");
    expect(widthOf("text", block("text"), three)).toBe("whole");
  });

  test("gives a profile the row", () => {
    const lisa = [point("Lisa", "Kurzhaar", { facet: "Fell" })];
    expect(widthOf("cards", block("cards", "Lisa", ["Fell"]), lisa)).toBe("whole");
  });
});
