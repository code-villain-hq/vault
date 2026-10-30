import type { Block, Point } from "@almanac/types";
import { pictureOf, picturesFor, pictureWidth } from "./collection";

/**
 * How the page draws a block: from the shape of what it keeps, the same way
 * every time, never from the look a talk gave it. Today, the week and You
 * are the page's own. Three kinds a talk gave say what the points are,
 * not how they look, so they stay until a talk can say it in what it
 * keeps: a list is steps to tick, questions are to practise, and a shelf
 * is the things a routine uses.
 *
 * - things with the same facts are cards, or a table past eight;
 * - days are a timeline, and days with numbers a trend;
 * - numbers are bars;
 * - writing that runs to paragraphs is a tile per section;
 * - short facts are chips, other sentences lines;
 * - and sentences about a thing, or about several, are a card each.
 *
 * A block of the set is drawn as its kind, and a collection as the picture
 * its fields pick (`pictureOf`).
 *
 * A block that keeps nothing has no view ("") and the page leaves it out.
 */
export function viewOf(block: Block, points: Point[]): string {
  if (SET_KINDS.has(block.kind)) return setViewOf(block, points);
  if (OWN.has(block.kind)) return block.kind;
  if (points.length === 0) return "";
  if (SAYS.has(block.kind)) return block.kind;
  const things = themesIn(block, points);
  if (block.facets.length > 0) return things > TABLE_PAST ? "table" : "cards";
  if (most(points, (p) => !!p.happened)) {
    return most(points, (p) => DAY.test(p.happened) && p.amount != null) ? "trend" : "timeline";
  }
  if (most(points, (p) => p.amount != null)) return "bars";
  const length = points.reduce((sum, p) => sum + p.text.length, 0) / points.length;
  if (length > PARAGRAPH) return "text";
  if (things > 0) return "cards";
  return points.every((p) => p.text.length <= CHIP_FITS) ? "chips" : "lines";
}

/**
 * How much of a row a block takes, of the page's twelve columns: as much as
 * its view needs. The row's blocks grow to fill it.
 */
export function widthOf(view: string, block: Block, points: Point[]): string {
  if (SET_KINDS.has(block.kind)) return setWidthOf(view, block, points);
  switch (view) {
    case "chips":
      return points.length > 8 ? "1/2" : "1/3";
    case "trend":
    case "parts":
      return "1/3";
    case "bars":
    case "questions":
    case "list":
    case "lines":
    case "timeline":
      return "1/2";
    case "cards":
    case "text":
    case "shelf": {
      // One card with its facts is a profile, with a tile per fact.
      const tiles = themesIn(block, points) || 1;
      if (view === "cards" && block.facets.length > 0 && tiles === 1) return "whole";
      return tiles === 1 ? "1/3" : tiles === 2 ? "1/2" : "whole";
    }
    default:
      return "whole";
  }
}

/**
 * The view a block is drawn with: the one a person picked with View as,
 * while it can still draw what the block keeps, or else the app's own.
 */
export function shownAs(block: Block, points: Point[]): string {
  return viewsFor(block, points).includes(block.view) ? block.view : viewOf(block, points);
}

/**
 * What View as offers for a block, the app's own pick among them: the
 * views that can draw what it keeps. A table needs its columns, cards
 * things to be about and a timeline days; a list draws anything, with
 * ticks only for steps. None for the page's own blocks, or when there's
 * only the one.
 */
export function viewsFor(block: Block, points: Point[]): string[] {
  if (block.kind === "collection") {
    const pictures = picturesFor(block, points);
    return pictures.length > 1 ? pictures : [];
  }
  if (SET_KINDS.has(block.kind)) return [];
  const pick = viewOf(block, points);
  if (!pick || OWN.has(pick)) return [];
  const things = themesIn(block, points);
  const fits = new Set([pick, block.kind === "list" ? "list" : "lines"]);
  if (block.facets.length > 0) fits.add("table");
  if (block.facets.length > 0 || things > 0) fits.add("cards");
  if (points.some((p) => p.happened)) fits.add("timeline");
  const views = Object.keys(VIEW_NAMES).filter((view) => fits.has(view));
  return views.length > 1 ? views : [];
}

/** Each view's name in View as, in the order it offers them. */
export const VIEW_NAMES: Record<string, string> = {
  table: "Table",
  cards: "Cards",
  text: "Text",
  shelf: "Shelf",
  questions: "Questions",
  chips: "Chips",
  bars: "Bars",
  trend: "Trend",
  parts: "Parts",
  timeline: "Timeline",
  list: "List",
  lines: "List",
  line: "Line",
  board: "Board",
  week: "Week",
};

/**
 * The one set of blocks every page is made of, beside today's kinds until
 * pages move into it: a heading, a paragraph, bullets, a checklist, a
 * toggle, a callout, and a collection, rows with fields.
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

/** A block of the set: a heading by its title, the rest by what they keep. */
function setViewOf(block: Block, points: Point[]): string {
  if (block.kind === "heading") return block.title.trim() ? "heading" : "";
  if (block.kind === "collection") return pictureOf(block, points);
  return points.length > 0 ? block.kind : "";
}

/**
 * A block of the set takes half a row, as writing keeps its measure; a
 * heading and a callout the whole, and a collection what its picture needs.
 */
function setWidthOf(view: string, block: Block, points: Point[]): string {
  if (block.kind === "collection") return pictureWidth(view, block, points);
  return block.kind === "heading" || block.kind === "callout" ? "whole" : "1/2";
}

/** The page's own blocks, drawn as they are. */
const OWN = new Set(["today", "week", "you"]);
/** Kinds that say what their points are. */
const SAYS = new Set(["list", "questions", "shelf"]);
/** Cards up to this many things; past it, a table compares them. */
const TABLE_PAST = 8;
/** A chip's most characters, as the server's `CHIP_FITS`. */
const CHIP_FITS = 48;
/** Points longer than this on average are paragraphs, not sentences. */
const PARAGRAPH = 150;
const DAY = /^\d{4}-\d{2}-\d{2}$/;

/**
 * How many things a block's points are about: their sections, but not one
 * named as the block is, which is the block itself ("Advice" in Advice).
 */
function themesIn(block: Block, points: Point[]): number {
  const names = new Set(points.map((p) => p.theme.trim()).filter(Boolean));
  const [only] = names;
  if (names.size === 1 && only?.toLowerCase() === block.title.trim().toLowerCase()) return 0;
  return names.size;
}

/** At least half of them. */
function most(points: Point[], is: (point: Point) => boolean): boolean {
  return points.filter(is).length * 2 >= points.length;
}
