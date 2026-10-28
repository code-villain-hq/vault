// When a timeline's event happened, as a talk wrote it: a year (1959), a
// span (1987–1993), a day (11. September 2001), a decade or a century, with
// "v. Chr." or "BC" before the year 1. The page sorts by it and shows the
// year in its column, a day's date before the event.

import type { Point } from "@almanac/types";

export interface When {
  /** For sorting, oldest first; null when it names no year. */
  key: number | null;
  /** The year's column: "1959", "1987–1993", "500 v. Chr.". */
  year: string;
  /** A day's date without its year, before the event: "11. September". */
  day: string | null;
  /** A span gets a longer mark than a year. */
  span: boolean;
}

const MONTHS: [RegExp, number][] = [
  [/\b(?:januar|jänner|january|jan)\b\.?/i, 1],
  [/\b(?:februar|feber|february|feb)\b\.?/i, 2],
  [/\b(?:märz|march|mar|mär)\b\.?/i, 3],
  [/\b(?:april|apr)\b\.?/i, 4],
  [/\bmai\b|\bmay\b/i, 5],
  [/\b(?:juni|june|jun)\b\.?/i, 6],
  [/\b(?:juli|july|jul)\b\.?/i, 7],
  [/\b(?:august|aug)\b\.?/i, 8],
  [/\b(?:september|sept|sep)\b\.?/i, 9],
  [/\b(?:oktober|october|okt|oct)\b\.?/i, 10],
  [/\b(?:november|nov)\b\.?/i, 11],
  [/\b(?:dezember|december|dez|dec)\b\.?/i, 12],
];
const BEFORE = /\s*(?:v\.\s?Chr\.?|v\.\s?u\.\s?Z\.?|BCE|BC)/i;
const SPAN = /\d\s*(?:[–—-]|\bbis\b|\bto\b|\buntil\b)\s*(?:etwa\s+|ca\.\s*|circa\s+|about\s+)?\d/i;
const CENTURY = /(\d{1,2})(?:\.|st|nd|rd|th)?\s*(?:jh\b\.?|jahrhundert|century)/i;

export function when(happened: string): When {
  const text = happened.trim();
  const sign = BEFORE.test(text) ? -1 : 1;
  const plain = { year: text, day: null };
  const key = (year: number, month = 0, day = 0) => sign * year * 10000 + month * 100 + day;

  const iso = text.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (iso) return { ...plain, key: key(n(iso[1]), n(iso[2]), n(iso[3])), span: false };
  if (SPAN.test(text)) {
    const first = text.match(/\d{3,4}/) ?? text.match(/\d{1,4}/);
    return { ...plain, key: first ? key(n(first[0]), monthOf(text)) : null, span: true };
  }
  const century = text.match(CENTURY);
  if (century) return { ...plain, key: key((n(century[1]) - 1) * 100), span: false };

  // A day: "11. September 2001", "September 11, 2001", "11.09.2001".
  const numeric = text.match(/^(\d{1,2})\.(\d{1,2})\.(\d{1,4})/);
  const month = numeric ? n(numeric[2]) : monthOf(text);
  if (month) {
    const year = numeric?.[3] ?? (text.match(/\d{3,4}/) ?? text.match(/\d{1,4}(?=\D*$)/))?.[0];
    if (!year) return { ...plain, key: null, span: false };
    const words = numeric
      ? `${numeric[1]}.${numeric[2]}.`
      : text
          .replace(BEFORE, "")
          .replace(year, "")
          .replace(/[\s,]+$/, "")
          .trim();
    const day = n(numeric ? numeric[1] : words.match(/\d{1,2}/)?.[0]);
    const before = text.match(BEFORE)?.[0].trim();
    return {
      key: key(n(year), month, day),
      year: before ? `${year} ${before}` : year,
      day: words || null,
      span: false,
    };
  }
  // A year, or a decade ("1960er", "1960s"): its first number that isn't a day's.
  const year = text.match(/\d{1,4}(?!\s*\.\s*\S)/) ?? text.match(/\d{1,4}/);
  return { ...plain, key: year ? key(n(year[0])) : null, span: false };
}

const n = (digits: string | undefined) => Number(digits ?? 0);

function monthOf(text: string): number {
  return MONTHS.find(([name]) => name.test(text))?.[1] ?? 0;
}

/** A timeline's events, oldest first; those with no year last, as they were kept. */
export function inOrder(points: Point[]): [Point, When][] {
  return points
    .map((point) => [point, when(point.happened)] as [Point, When])
    .sort(
      ([, a], [, b]) => (a.key ?? Number.MAX_SAFE_INTEGER) - (b.key ?? Number.MAX_SAFE_INTEGER),
    );
}

/**
 * The first and last year an event covers, below 0 before the year 1: the
 * same year twice unless it's a span. Null when it names no year.
 */
export function years(happened: string): [number, number] | null {
  const at = when(happened);
  if (at.key === null) return null;
  // A key's month and day stay under 1232, so its year is the nearest 10000th.
  const sign = at.key < 0 ? -1 : 1;
  const from = sign * Math.round(Math.abs(at.key) / 10000);
  if (!at.span) return [from, from];
  const last = happened.match(/\d{1,4}/g)?.at(-1) ?? "";
  const first = String(Math.abs(from));
  // "1945–46" ends in 1946; a shorter last number that isn't a year's end is a day's.
  const end =
    last.length >= first.length
      ? Number(last)
      : last.length === 2
        ? Number(first.slice(0, -2) + last)
        : Math.abs(from);
  return [from, Math.max(from, sign * end)];
}

/** A line of the topic page's timeline: a section's events, or one kept under no section. */
export interface Line {
  /** The section, which opens on its own page with its events. */
  section: string | null;
  /** Oldest first. */
  points: Point[];
  /** When its events begin and end: "1816–1953", or one event's year. */
  year: string;
  span: boolean;
}

/**
 * The topic page's timeline: a line for each section, oldest first by its
 * first event, when its events begin and end; an event kept under no
 * section is a line of its own.
 */
export function bySection(points: Point[]): Line[] {
  const lines: Line[] = [];
  const sections = new Map<string, Line>();
  for (const [point, at] of inOrder(points)) {
    const line = point.theme ? sections.get(point.theme) : undefined;
    if (line) {
      line.points.push(point);
      continue;
    }
    const made = { section: point.theme || null, points: [point], year: at.year, span: at.span };
    if (made.section) sections.set(made.section, made);
    lines.push(made);
  }
  for (const line of sections.values()) {
    if (line.points.length === 1) continue;
    const covered = line.points.flatMap((p) => years(p.happened) ?? []);
    if (covered.length === 0) continue;
    const from = Math.min(...covered);
    const to = Math.max(...covered);
    const era = line.points.map((p) => p.happened.match(BEFORE)?.[0].trim()).find(Boolean);
    line.year = between(from, to, era ?? "v. Chr.");
    line.span = from !== to;
  }
  return lines;
}

/** "1816–1953", "500–400 v. Chr.", "44 v. Chr.–14", or one year. */
function between(from: number, to: number, era: string): string {
  const year = (y: number) => String(Math.abs(y));
  if (from === to) return from < 0 ? `${year(from)} ${era}` : year(from);
  if (to < 0) return `${year(from)}–${year(to)} ${era}`;
  if (from < 0) return `${year(from)} ${era}–${year(to)}`;
  return `${from}–${to}`;
}
