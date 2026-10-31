import type { Block, Point } from "@almanac/types";
import { pictureOf, picturesFor, pictureWidth } from "./collection";

/**
 * How the page draws a block: as its kind, one of the set, and a
 * collection as the picture its fields pick (`pictureOf`). A heading shows
 * while it has a title, the rest while they keep something; one with
 * nothing to show has no view ("") and the page leaves it out. What it
 * knows about you is About's, drawn as it is.
 */
export function viewOf(block: Block, points: Point[]): string {
  if (block.kind === "you") return "you";
  if (!SET_KINDS.has(block.kind)) return "";
  if (block.kind === "heading") return block.title.trim() ? "heading" : "";
  if (block.kind === "collection") return pictureOf(block, points);
  return points.length > 0 ? block.kind : "";
}

/**
 * How much of a row a block takes, of the page's twelve columns: half, as
 * writing keeps its measure; a heading and a callout the whole, and a
 * collection what its picture needs. A person's pick (`width`, half or
 * the whole) holds over the app's. The row's blocks grow to fill it.
 */
export function widthOf(view: string, block: Block, points: Point[]): string {
  if (block.kind === "heading") return "whole";
  if (WIDTHS.includes(block.width)) return block.width;
  return ownWidth(view, block, points);
}

/** The widths a person picks between: half the page or the whole. */
export const WIDTHS = ["1/2", "whole"];

/** The width the app picks for a block. */
export function ownWidth(view: string, block: Block, points: Point[]): string {
  if (block.kind === "collection") return pictureWidth(view, block, points);
  return block.kind === "heading" || block.kind === "callout" || block.kind === "you"
    ? "whole"
    : "1/2";
}

/**
 * The view a block is drawn with: the picture a person picked with View
 * as, while it can still draw what the collection keeps, or else the app's own.
 */
export function shownAs(block: Block, points: Point[]): string {
  return viewsFor(block, points).includes(block.view) ? block.view : viewOf(block, points);
}

/**
 * What View as offers for a block, the app's own pick among them: the
 * pictures a collection's fields can draw. None for the other kinds, or
 * when there's only the one.
 */
export function viewsFor(block: Block, points: Point[]): string[] {
  if (block.kind !== "collection") return [];
  const pictures = picturesFor(block, points);
  return pictures.length > 1 ? pictures : [];
}

/** Each picture's name in View as. */
export const VIEW_NAMES: Record<string, string> = {
  table: "Table",
  cards: "Cards",
  bars: "Bars",
  timeline: "Timeline",
  list: "List",
  line: "Line",
  board: "Board",
  week: "Week",
};

/**
 * The one set of blocks every page is made of: a heading, a paragraph,
 * bullets, a checklist, a toggle, a callout, and a collection, rows with
 * fields.
 */
export const SET_KINDS = new Set([
  "heading",
  "paragraph",
  "bullets",
  "checklist",
  "toggle",
  "callout",
  "collection",
]);
