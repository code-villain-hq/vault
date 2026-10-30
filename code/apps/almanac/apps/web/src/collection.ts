// A collection: rows with fields, a point a row, its text the row's name
// and its cells the rest, by field name. Its fields pick its picture, by
// the same rules on every page; nobody designs one.

import type { Block, Cell, Field, Point } from "@almanac/types";
import { amountWords, dateWords, today } from "./format";
import { when } from "./timeline";

/** The pictures a collection is drawn as, in the order View as offers them. */
export const PICTURES = ["line", "board", "cards", "timeline", "week", "bars", "table", "list"];

const DAY = /^\d{4}-\d{2}-\d{2}$/;

/** A row's cell in `field`, if it has one: an empty one is none. */
export function cellOf(row: Point, field: Field): Cell | undefined {
  const cell = row.cells[field.name];
  return cell === undefined || cell === "" ? undefined : cell;
}

/** The collection's fields its rows fill, in its order. */
export function filled(block: Block, rows: Point[]): Field[] {
  return block.fields.filter((field) => rows.some((row) => cellOf(row, field) !== undefined));
}

/** A row's day in `field`, when it's a day ("2026-10-30") and not a span or a year. */
export function dayOf(row: Point, field: Field): string | null {
  const cell = cellOf(row, field);
  return typeof cell === "string" && DAY.test(cell) ? cell : null;
}

/** A row's number in `field`. */
export function numberOf(row: Point, field: Field): number | null {
  const cell = cellOf(row, field);
  return typeof cell === "number" ? cell : null;
}

/**
 * The picture the app draws a collection as: the first rule that fits.
 *
 * - a day and a number are a line;
 * - a status is a board;
 * - a photo is cards;
 * - days, or spans, all past are a timeline;
 * - days still to come are a week;
 * - a name and a number are bars;
 * - three fields or more, the name with them, are a table;
 * - anything else is a list.
 *
 * A collection with no rows has no picture ("") and the page leaves it out.
 */
export function pictureOf(block: Block, rows: Point[], on = today()): string {
  if (rows.length === 0) return "";
  const fields = filled(block, rows);
  const of = (kind: string) => fields.find((f) => f.kind === kind);
  const day = of("day");
  const number = of("number");
  if (day && number && rows.some((r) => dayOf(r, day) && numberOf(r, number) !== null)) {
    return "line";
  }
  if (of("status")) return "board";
  if (of("photo")) return "cards";
  if (day) return rows.some((r) => (dayOf(r, day) ?? "") >= on) ? "week" : "timeline";
  if (number) return "bars";
  return fields.length + 1 >= 3 ? "table" : "list";
}

/**
 * What View as offers for a collection, the app's pick among them: the
 * pictures its fields can draw. A line needs days with numbers, a board a
 * status or a choice, a timeline days or spans, a week days, bars numbers;
 * cards, a table and a list draw anything.
 */
export function picturesFor(block: Block, rows: Point[], on = today()): string[] {
  const pick = pictureOf(block, rows, on);
  if (!pick) return [];
  const fields = filled(block, rows);
  const of = (kind: string) => fields.find((f) => f.kind === kind);
  const day = of("day");
  const number = of("number");
  const fits = new Set([pick, "cards", "table", "list"]);
  if (day && number && rows.some((r) => dayOf(r, day) && numberOf(r, number) !== null)) {
    fits.add("line");
  }
  if (of("status") || of("choice")) fits.add("board");
  if (day) fits.add("timeline");
  if (day && rows.some((r) => dayOf(r, day))) fits.add("week");
  if (number) fits.add("bars");
  return PICTURES.filter((picture) => fits.has(picture));
}

/**
 * How much of a row a collection takes, of the page's twelve columns: a
 * board, a week and a timeline the whole; a table the whole past three
 * columns; cards the whole past two; a line, bars and a list half.
 */
export function pictureWidth(picture: string, block: Block, rows: Point[]): string {
  switch (picture) {
    case "board":
    case "week":
    case "timeline":
      return "whole";
    case "table":
      return filled(block, rows).length + 1 > 3 ? "whole" : "1/2";
    case "cards":
      return rows.length > 2 ? "whole" : "1/2";
    default:
      return "1/2";
  }
}

/**
 * A board's lanes: the status's words in its order, or those its rows use;
 * the first is still open and the last is done.
 */
export function lanesOf(field: Field, rows: Point[]): string[] {
  const used = rows.flatMap((row) => {
    const cell = cellOf(row, field);
    return typeof cell === "string" ? [cell] : [];
  });
  const lanes = field.options.length > 0 ? [...field.options] : [];
  for (const word of used) if (!lanes.includes(word)) lanes.push(word);
  return lanes;
}

/** A cell in words: "3,4 kg", "Oct 30", "1898–1962", a link's site. Checks and photos have none. */
export function cellWords(field: Field, cell: Cell | undefined): string {
  if (cell === undefined || typeof cell === "boolean") return "";
  if (typeof cell === "number") {
    return field.unit ? `${amountWords(cell)} ${field.unit}` : amountWords(cell);
  }
  if (field.kind === "photo") return "";
  if (field.kind === "day") return DAY.test(cell) ? dateWords(cell) : cell;
  if (field.kind === "link") {
    try {
      return new URL(cell).hostname.replace(/^www\./, "");
    } catch {
      return cell;
    }
  }
  return cell;
}

/** Rows by their day or span in `field`, oldest first; those with none last. */
export function byDay(rows: Point[], field: Field): Point[] {
  const key = (row: Point) => {
    const cell = cellOf(row, field);
    return typeof cell === "string" ? when(cell).key : null;
  };
  return [...rows].sort((a, b) => {
    const [x, y] = [key(a), key(b)];
    if (x === null || y === null) return (x === null ? 1 : 0) - (y === null ? 1 : 0);
    return x - y || a.position - b.position;
  });
}
