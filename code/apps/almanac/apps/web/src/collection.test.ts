import { describe, expect, test } from "bun:test";
import type { Block, Cell, Field, Point } from "@almanac/types";
import { byDay, cellWords, lanesOf, pictureOf, picturesFor, pictureWidth } from "./collection";

const field = (name: string, kind: string, more: Partial<Field> = {}): Field => ({
  name,
  kind,
  unit: "",
  goal: null,
  options: [],
  ...more,
});

const collection = (...fields: Field[]): Block => ({
  id: "b1",
  workspace_id: "w",
  topic_id: "t",
  kind: "collection",
  title: "Rows",
  facets: [],
  fields,
  width: "",
  view: "",
  row_id: null,
  position: 0,
  created_at: "",
  updated_at: "",
});

let n = 0;
const row = (text: string, cells: Record<string, Cell> = {}): Point => ({
  id: `p${++n}`,
  workspace_id: "w",
  topic_id: "t",
  block_id: "b1",
  theme: "",
  text,
  happened: "",
  facet: "",
  look: "",
  amount: null,
  note: "",
  done: false,
  cells,
  source: null,
  source_url: null,
  talk_id: null,
  seen: true,
  position: n,
  created_at: "",
  updated_at: "",
});

const ON = "2026-10-30";

describe("pictureOf", () => {
  test("draws days with numbers as a line, Lisa's weight", () => {
    const weight = collection(field("Tag", "day"), field("Gewicht", "number", { unit: "kg" }));
    const rows = [row("Wiegen", { Tag: "2026-06-12", Gewicht: 3.6 })];
    expect(pictureOf(weight, rows, ON)).toBe("line");
  });

  test("draws a check, with a day at most besides, as a checklist, ticked or not", () => {
    const todo = collection(field("Done", "check"), field("Bis", "day"));
    expect(pictureOf(todo, [row("Haare waschen")], ON)).toBe("checklist");
    expect(pictureOf(todo, [row("Müll", { Done: true, Bis: "2026-11-02" })], ON)).toBe("checklist");
    const shoes = collection(field("Vegan", "check"), field("Preis", "number"));
    expect(pictureOf(shoes, [row("Saola", { Vegan: true, Preis: 1599 })], ON)).toBe("bars");
  });

  test("draws a status as a board, before anything else it has", () => {
    const repairs = collection(field("Raum", "text"), field("Status", "status"));
    expect(pictureOf(repairs, [row("Abfluss", { Raum: "Bad", Status: "Offen" })], ON)).toBe(
      "board",
    );
  });

  test("draws a photo as cards", () => {
    const cats = collection(field("Foto", "photo"), field("Alter", "text"));
    expect(pictureOf(cats, [row("Lisa", { Foto: "ph1", Alter: "3½ Jahre" })], ON)).toBe("cards");
  });

  test("draws days and spans that are past as a timeline, and days to come as a week", () => {
    const themes = collection(field("Zeitraum", "day"));
    expect(pictureOf(themes, [row("Kuba", { Zeitraum: "1898–1962" })], ON)).toBe("timeline");
    expect(pictureOf(themes, [row("Termin", { Zeitraum: "2026-10-01" })], ON)).toBe("timeline");
    expect(pictureOf(themes, [row("Hausmeister", { Zeitraum: "2026-11-03" })], ON)).toBe("week");
    expect(pictureOf(themes, [row("Heute", { Zeitraum: ON })], ON)).toBe("week");
  });

  test("draws a name and a number as bars", () => {
    const offers = collection(field("Preis", "number", { unit: "€" }));
    expect(pictureOf(offers, [row("Spirale", { Preis: 15 })], ON)).toBe("bars");
  });

  test("draws three fields or more as a table, and fewer as a list", () => {
    const food = collection(field("Art", "choice"), field("Bei Lisa", "text"));
    expect(pictureOf(food, [row("Felix", { Art: "Nass", "Bei Lisa": "Mag sie" })], ON)).toBe(
      "table",
    );
    const terms = collection(field("Heißt", "text"));
    expect(pictureOf(terms, [row("Intifada", { Heißt: "Aufstand" })], ON)).toBe("list");
  });

  test("draws a shoe's offers as a table, then as a line a shop once a shop has two days", () => {
    const offers = collection(
      field("Preis", "number", { unit: "kr" }),
      field("Tag", "day"),
      field("Link", "link"),
    );
    const first = [
      row("Scorett", { Preis: 1599, Tag: "2026-10-30", Link: "https://example.com/a" }),
      row("Skocenter", { Preis: 1799, Tag: "2026-10-30", Link: "https://example.com/b" }),
    ];
    expect(pictureOf(offers, first, ON)).toBe("table");
    const again = [...first, row("Scorett", { Preis: 1499, Tag: "2026-11-02" })];
    expect(pictureOf(offers, again, ON)).toBe("line");
    // Two rows a day that never come again are no line: a timeline.
    const moves = collection(field("Preis", "number"), field("Tag", "day"));
    const sold = [
      row("Sofa", { Preis: 300, Tag: "2026-10-01" }),
      row("Tisch", { Preis: 80, Tag: "2026-10-01" }),
      row("Lampe", { Preis: 20, Tag: "2026-10-02" }),
    ];
    expect(pictureOf(moves, sold, ON)).toBe("timeline");
  });

  test("draws a name, a number and one field more as bars, and more as a table", () => {
    const prices = collection(
      field("Preis", "number"),
      field("Laden", "text"),
      field("Größe", "text"),
    );
    expect(pictureOf(prices, [row("Saola", { Preis: 1599, Laden: "Scorett" })], ON)).toBe("bars");
    expect(
      pictureOf(prices, [row("Saola", { Preis: 1599, Laden: "Scorett", Größe: "42" })], ON),
    ).toBe("table");
  });

  test("counts only the fields its rows fill", () => {
    const food = collection(
      field("Art", "choice"),
      field("Bei Lisa", "text"),
      field("Foto", "photo"),
    );
    expect(pictureOf(food, [row("Felix", { Art: "Nass", "Bei Lisa": "" })], ON)).toBe("list");
  });

  test("has no picture with no rows, so the page leaves it out", () => {
    expect(pictureOf(collection(field("Preis", "number")), [], ON)).toBe("");
  });
});

describe("figures", () => {
  test("a few names each with a short value are figures, a long one a list", () => {
    const facts = collection(field("Wert", "text"));
    const short = [row("Alter", { Wert: "3½ Jahre" }), row("Gewicht", { Wert: "3,4 kg" })];
    expect(pictureOf(facts, short, ON)).toBe("figures");
    expect(pictureWidth("figures", facts, short)).toBe("whole");
    const long = [...short, row("Schlaf", { Wert: "zwölf bis sechzehn Stunden am Tag" })];
    expect(pictureOf(facts, long, ON)).toBe("list");
  });

  test("steps are offered for a few rows in their order, never picked", () => {
    const leash = collection(field("Geübt", "check"), field("Tipp", "text"));
    const rows = [row("Geschirr zeigen", { Geübt: true }), row("Drinnen tragen", { Tipp: "kurz" })];
    expect(pictureOf(leash, rows, ON)).not.toBe("steps");
    expect(picturesFor(leash, rows, ON)).toContain("steps");
  });
});

describe("picturesFor", () => {
  test("offers what the fields can draw, the app's pick among them", () => {
    const weight = collection(field("Tag", "day"), field("Gewicht", "number"));
    const rows = [row("Wiegen", { Tag: "2026-06-12", Gewicht: 3.6 })];
    expect(picturesFor(weight, rows, ON)).toEqual([
      "line",
      "cards",
      "timeline",
      "week",
      "bars",
      "table",
      "list",
    ]);
  });

  test("offers a board for a choice, but no line without days", () => {
    const food = collection(field("Art", "choice"));
    expect(picturesFor(food, [row("Felix", { Art: "Nass" })], ON)).toEqual([
      "board",
      "cards",
      "table",
      "list",
    ]);
  });

  test("offers nothing with no rows", () => {
    expect(picturesFor(collection(field("Art", "choice")), [], ON)).toEqual([]);
  });
});

test("a board's lanes are the status's words, then any others its rows use", () => {
  const status = field("Status", "status", { options: ["Offen", "Bestellt", "Erledigt"] });
  expect(
    lanesOf(status, [row("a", { Status: "Erledigt" }), row("b", { Status: "Warten" })]),
  ).toEqual(["Offen", "Bestellt", "Erledigt", "Warten"]);
  expect(lanesOf(field("Status", "status"), [row("a", { Status: "Neu" })])).toEqual(["Neu"]);
});

test("a cell reads as words: a number with its unit, a day, a link's site", () => {
  expect(cellWords(field("Preis", "number", { unit: "€" }), 89)).toMatch(/^89 €$/);
  expect(cellWords(field("Tag", "day"), "2026-11-03")).toBe("Nov 3");
  expect(cellWords(field("Zeitraum", "day"), "1898–1962")).toBe("1898–1962");
  expect(cellWords(field("Seite", "link"), "https://www.btselem.org/stats")).toBe("btselem.org");
  expect(cellWords(field("Geimpft", "check"), true)).toBe("");
});

test("rows go by their day or span, oldest first, those with none last", () => {
  const span = field("Zeitraum", "day");
  const rows = [
    row("Ohne"),
    row("Kuba", { Zeitraum: "1898–1962" }),
    row("Europa", { Zeitraum: "1500" }),
  ];
  expect(byDay(rows, span).map((r) => r.text)).toEqual(["Europa", "Kuba", "Ohne"]);
});

test("a board, a week and a timeline take the whole row; a line half", () => {
  const block = collection(field("Status", "status"));
  expect(pictureWidth("board", block, [])).toBe("whole");
  expect(pictureWidth("line", block, [])).toBe("1/2");
});
