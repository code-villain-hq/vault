import type { Block, Point } from "@almanac/types";

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
 * A block that keeps nothing has no view ("") and the page leaves it out.
 */
export function viewOf(block: Block, points: Point[]): string {
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
