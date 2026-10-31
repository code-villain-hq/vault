// The one set of blocks every page is made of: paragraphs, a callout, and
// a collection drawn as the picture its fields pick (`collection.ts`): a
// line, a checklist, a board, cards, a table, a timeline, a week, bars or
// a list. The same on every topic; nobody designs a page's own. A row
// opens its own page on a topic's page (`Opens`). Also the parts the
// blocks share: a photo, a tick, a timeline's rail, ✕ to take a point off.

import type { Block, Field, Point, Reminder } from "@almanac/types";
import { type Confirmation, ConfirmDialog, RowAction } from "@code-villain/web";
import { Check, Circle, CircleCheck, Info, TriangleAlert, X } from "lucide-react";
import {
  type ComponentType,
  type CSSProperties,
  createContext,
  Fragment,
  type ReactNode,
  useContext,
  useMemo,
  useState,
} from "react";
import { TrendSet } from "./charts";
import {
  byDay,
  cellOf,
  cellWords,
  checkOf,
  dayOf,
  filled,
  isPart,
  lanesOf,
  numberOf,
  sumOf,
} from "./collection";
import { EditCell, NewRow, useSetCell } from "./editing";
import {
  addDays,
  amountWords,
  dateWords,
  dayOfMonth,
  dueWords,
  rhythmWords,
  today,
  weekdayShort,
  weekdayWords,
} from "./format";
import { usePhoto } from "./photos";
import { inline } from "./prose";
import { doneFor, isDue, markDone, markNotDone } from "./reminders";
import { when, years } from "./timeline";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";

/** A line's measure, for lines that read across. */
export const LINES = "max-w-[86ch]";

/** What opens: a section's own page, by its name, or a row's, by its id too. */
export interface PageTo {
  section: string;
  row?: string;
}

/**
 * The link that opens a section's or a row's own page, which a topic's
 * page gives its blocks; elsewhere (Settings → Blocks) names stay words.
 */
export const Opens = createContext<ComponentType<
  PageTo & { className?: string; label?: string; children: ReactNode }
> | null>(null);

/** A row's name, which opens its page where there's one to open. */
function RowName({ row, className = "" }: { row: Point; className?: string }) {
  const Open = useContext(Opens);
  if (!Open) return <span className={className}>{row.text}</span>;
  return (
    <Open section={row.text} row={row.id} className={`${className} hover:underline`}>
      {row.text}
    </Open>
  );
}

export function Photo({ id, className }: { id: string; className: string }) {
  const { place, workspace } = useWorkspace();
  const url = usePhoto(place, workspace.id, id);
  return url ? (
    <img src={url} alt="" className={`shrink-0 object-cover ${className}`} />
  ) : (
    <span aria-hidden="true" className={`shrink-0 bg-chip ${className}`} />
  );
}

/** A row's photo, small, at the start of its line, when its collection keeps one. */
function Thumb({
  row,
  fields,
  className = "size-8",
}: {
  row: Point;
  fields: Field[];
  className?: string;
}) {
  const photo = fields.find((f) => f.kind === "photo");
  const id = photo && cellOf(row, photo);
  return typeof id === "string" ? <Photo id={id} className={`rounded-md ${className}`} /> : null;
}

/**
 * Where a point comes from, if the page shows it: a name or a link. "From
 * Grok" and "From the talk" say only which talk, which a section's own
 * page shows, so the page keeps them for a tooltip.
 */
export function named(point: Point): boolean {
  return !!point.source && (!!point.source_url || !/^from\b/i.test(point.source));
}

/** The reminders that ring for checklist items, by item. */
export function useItemReminders(): Map<string, Reminder> {
  const reminders = useRecords("Reminder");
  return useMemo(
    () =>
      new Map(
        reminders
          .filter((r) => r.point_id && (r.status === "on" || r.status === "done"))
          .map((r) => [r.point_id as string, r]),
      ),
    [reminders],
  );
}

/**
 * Whether a row to tick is done: its `check`, or, with a reminder, done
 * for the last day it was due, until it comes due again.
 */
export function itemDone(row: Point, check: Field, reminder: Reminder | undefined): boolean {
  if (!reminder) return cellOf(row, check) === true;
  return doneFor(reminder) !== undefined && !isDue(reminder);
}

/**
 * A box to tick a row by its `check`. A row with a `reminder` ticks that
 * instead. Guests see it, not tick it.
 */
export function Tick({
  row,
  check,
  reminder,
  className = "",
}: {
  row: Point;
  check: Field;
  reminder?: Reminder;
  className?: string;
}) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const done = itemDone(row, check, reminder);
  const Icon = done ? CircleCheck : Circle;
  const look = `shrink-0 ${done ? "text-accent" : "text-faint"}`;
  if (guest) {
    return (
      <Icon
        size={18}
        strokeWidth={1.75}
        role="img"
        aria-label={done ? "Done" : "Not done"}
        className={`${look} ${className}`}
      />
    );
  }
  return (
    <button
      type="button"
      aria-pressed={done}
      aria-label={`${row.text}: ${done ? "done" : "to do"}`}
      onClick={() => {
        if (reminder) return done ? markNotDone(sync, reminder) : markDone(sync, reminder);
        sync.update("Point", row.id, { cells: { ...row.cells, [check.name]: !done } });
      }}
      className={`grid place-items-center rounded-full hover:text-muted ${look} ${className}`}
    >
      <Icon size={18} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

/** A timeline's lines, with the years right-aligned in a column of their own. */
export function TimelineRail({ children }: { children: ReactNode }) {
  return (
    <div className="@container">
      <ol
        className={`grid ${LINES} grid-cols-[minmax(3.5rem,max-content)_0.875rem_1fr] gap-x-3 @lg:grid-cols-[minmax(5.5rem,max-content)_0.875rem_1fr]`}
      >
        {children}
      </ol>
    </div>
  );
}

/** A line on a timeline: the year, a mark on the rail (longer for a span), then what happened. */
export function TimelineLine({
  year,
  span,
  seen,
  children,
}: {
  year: string;
  span: boolean;
  seen: boolean;
  children: ReactNode;
}) {
  return (
    <li className="group col-span-full grid grid-cols-subgrid">
      <span className="py-[5px] text-right whitespace-nowrap text-muted tabular-nums">{year}</span>
      <span
        aria-hidden="true"
        className={`relative before:absolute before:inset-y-0 before:left-1.5 before:w-[1.5px] before:bg-line group-first:before:top-3.5 group-last:before:bottom-auto group-last:before:h-3.5 group-only:before:hidden after:absolute after:left-[3px] after:w-2 after:rounded-full ${
          span ? "after:top-[9px] after:h-3.5" : "after:top-3 after:h-2"
        } ${seen ? "after:bg-muted" : "after:bg-accent"}`}
      />
      {children}
    </li>
  );
}

/** Takes a point off the page, once you say so. */
export function RemovePoint({ point }: { point: Point }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  if (guest) return null;
  return (
    <>
      <RowAction
        label="Remove this point…"
        onClick={() =>
          setConfirmation({
            title: "Remove this point?",
            body: "It leaves the page for everyone in the workspace. This can't be undone.",
            action: "Remove point",
            onConfirm: () => sync.delete("Point", point.id),
          })
        }
        className="self-center opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
      >
        <X size={13} strokeWidth={1.75} aria-hidden="true" />
      </RowAction>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** A dot on what a talk added since you last looked. */
function NewDot({ point, className = "" }: { point: Point; className?: string }) {
  if (point.seen) return null;
  return (
    <span
      role="img"
      aria-label="New"
      className={`size-1.5 shrink-0 rounded-full bg-accent ${className}`}
    />
  );
}

/** Where the points come from, once under them, not on every line. */
function Sources({ points }: { points: Point[] }) {
  const seen = new Set<string>();
  const sources = points.filter((p) => {
    if (!named(p) || seen.has(p.source ?? "")) return false;
    seen.add(p.source ?? "");
    return true;
  });
  if (sources.length === 0) return null;
  return (
    <p className="text-xs text-faint">
      {sources.map((p, i) => (
        <span key={p.id}>
          {i > 0 && " · "}
          {p.source_url ? (
            <a href={p.source_url} target="_blank" rel="noreferrer" className="ui-link">
              {p.source}
            </a>
          ) : (
            p.source
          )}
        </span>
      ))}
    </p>
  );
}

/** Paragraphs: a point each, at a reading measure. */
export function Paragraphs({ points }: { points: Point[] }) {
  return (
    <div className="flex max-w-measure flex-col gap-2">
      {points.map((point) => (
        <p key={point.id} className="group flex items-start gap-2">
          <NewDot point={point} className="mt-2" />
          <span className="min-w-0 flex-1">{inline(point.text)}</span>
          <RemovePoint point={point} />
        </p>
      ))}
      <Sources points={points} />
    </div>
  );
}

/** How many of a checklist's rows are done. */
export function ChecklistCount({ block, rows }: { block: Block; rows: Point[] }) {
  const rings = useItemReminders();
  const check = checkOf(block) ?? block.fields.find((f) => f.kind === "check");
  if (!check) return null;
  const done = rows.filter((r) => itemDone(r, check, rings.get(r.id))).length;
  return (
    <span className="flex items-center gap-2 whitespace-nowrap text-xs text-faint tabular-nums">
      <span aria-hidden="true" className="block h-1.5 w-16 rounded-full bg-chip">
        <span
          className={`block h-full rounded-full ${done === rows.length ? "bg-ok" : "bg-info"}`}
          style={{ width: `${rows.length ? (done / rows.length) * 100 : 0}%` }}
        />
      </span>
      {done} of {rows.length} done
    </span>
  );
}

/** When an item's reminder rings: its rhythm, or the day it's due. */
function Rings({ reminder }: { reminder: Reminder }) {
  const due = isDue(reminder);
  const words = reminder.every_days ? rhythmWords(reminder.every_days) : dueWords(reminder.due_on);
  return (
    <span
      title={reminder.status === "on" ? `Next ${dueWords(reminder.due_on)}` : undefined}
      className={`shrink-0 text-xs ${due ? "text-accent" : "text-faint"}`}
    >
      {words}
    </span>
  );
}

/** A callout: what to keep in mind, in a box that stands out; warn if any point is one. */
export function Callout({ points }: { points: Point[] }) {
  const warn = points.some((p) => p.look === "warn");
  const Icon = warn ? TriangleAlert : Info;
  return (
    <div className="flex max-w-[68ch] gap-2.5 rounded-md bg-chip px-3.5 py-2.5">
      <Icon
        size={16}
        strokeWidth={1.75}
        aria-label={warn ? "Mind this" : "Note"}
        className={`mt-0.5 shrink-0 ${warn ? "text-warn-ink" : "text-muted"}`}
      />
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        {points.map((point) => (
          <p key={point.id} className="group flex items-start gap-2">
            <span className="min-w-0 flex-1">{inline(point.text)}</span>
            <RemovePoint point={point} />
          </p>
        ))}
      </div>
    </div>
  );
}

/** A row's other cells, in words: "Bad · Offen · ✓ Geimpft". */
function rest(fields: Field[], row: Point): string {
  return fields
    .filter((field) => !TOLD_BY_TONE.has(field.kind) && field.kind !== "check")
    .flatMap((field) => {
      const cell = cellOf(row, field);
      const words = cellWords(field, cell);
      return words ? [words] : [];
    })
    .join(" · ");
}

/**
 * A collection, as the picture the page draws it with (`block.view`): its
 * fields pick it, or a person did with View as.
 */
/** The kinds a row's line shows as a coloured pill, not words. */
const TOLD_BY_TONE = new Set(["status", "choice"]);

/**
 * A choice's options told apart by a colour each, in its order: the data
 * hues the topic's own leaves (`hues.ts`). Status colours keep their meanings.
 */
const CHOICE_DOTS = ["bg-choice-1", "bg-choice-2", "bg-choice-3", "bg-choice-4", "bg-choice-5"];
/** A choice's pill: its colour, tinted. */
const CHOICE_PILLS = [
  "bg-choice-1/15",
  "bg-choice-2/15",
  "bg-choice-3/15",
  "bg-choice-4/15",
  "bg-choice-5/15",
];

/**
 * How a status's or a choice's word looks: a status still open at first
 * (its first option), done at its last and under way between, as a
 * board's lanes; a choice's options a dot each.
 */
/** The colour of a choice's word, its dot: by its place among the options. */
export function choiceDot(field: Field, value: string): string {
  const at = field.options.indexOf(value);
  return at >= 0 ? (CHOICE_DOTS[at % CHOICE_DOTS.length] ?? "bg-faint") : "bg-faint";
}

/** A card's tint (`bg-tinted`) by a row's choice: its option's colour. None for a status, or no choice. */
function choiceTint(fields: Field[], row: Point): CSSProperties | undefined {
  const choice = fields.find((f) => f.kind === "choice");
  const cell = choice && cellOf(row, choice);
  const at = choice && typeof cell === "string" ? choice.options.indexOf(cell) : -1;
  return at >= 0 ? ({ "--tint": `var(--choice-${(at % 5) + 1})` } as CSSProperties) : undefined;
}

export function Pill({ field, value }: { field: Field; value: string }) {
  const at = field.options.indexOf(value);
  if (field.kind === "status") {
    const tone =
      at > 0 && at === field.options.length - 1
        ? "bg-ok/15 text-ok-ink"
        : at > 0
          ? "bg-info/15 text-info-ink"
          : "bg-chip text-muted";
    return (
      <span
        className={`inline-flex max-w-full shrink-0 items-center truncate rounded-full px-2 py-px font-medium text-xs ${tone}`}
      >
        {value}
      </span>
    );
  }
  const dot = choiceDot(field, value);
  const tint = at >= 0 ? CHOICE_PILLS[at % CHOICE_PILLS.length] : "bg-chip";
  return (
    <span
      className={`inline-flex max-w-full shrink-0 items-center gap-1.5 truncate rounded-full px-2 py-px text-xs ${tint}`}
    >
      <span aria-hidden="true" className={`size-1.5 shrink-0 rounded-full ${dot}`} />
      {value}
    </span>
  );
}

/** Words that say yes or no, as a status's or a choice's options. */
const YES = new Set(["ja", "yes", "oui", "sí", "si", "true"]);
const NO = new Set(["nein", "no", "non", "false"]);

/**
 * A cell that says yes or no: a check's, or a status's or choice's whose
 * only options are yes and no. Null for any other.
 */
function yesOrNo(field: Field, cell: unknown): boolean | null {
  if (field.kind === "check") return typeof cell === "boolean" ? cell : null;
  if (!TOLD_BY_TONE.has(field.kind) || typeof cell !== "string") return null;
  const words = field.options.map((o) => o.toLowerCase());
  if (words.length !== 2 || !words.some((w) => YES.has(w)) || !words.some((w) => NO.has(w)))
    return null;
  const word = cell.toLowerCase();
  return YES.has(word) ? true : NO.has(word) ? false : null;
}

/** A yes as the field's name, "Vegan"; a no as the name crossed out. */
function NamePill({ name, yes }: { name: string; yes: boolean }) {
  return (
    <span
      title={`${name}: ${yes ? "yes" : "no"}`}
      className={`inline-flex max-w-full shrink-0 items-center truncate rounded-full px-2 py-px font-medium text-xs ${yes ? "bg-ok/15 text-ok-ink" : "bg-chip text-faint line-through"}`}
    >
      {name}
    </span>
  );
}

/** A row's statuses, choices and checks, a pill each. */
function Pills({ fields, row }: { fields: Field[]; row: Point }) {
  const pills = fields.flatMap((field) => {
    const cell = cellOf(row, field);
    const yes = yesOrNo(field, cell);
    if (yes !== null) return [<NamePill key={field.name} name={field.name} yes={yes} />];
    return TOLD_BY_TONE.has(field.kind) && typeof cell === "string"
      ? [<Pill key={field.name} field={field} value={cell} />]
      : [];
  });
  return pills.length > 0 ? (
    <span className="flex shrink-0 gap-1.5 self-center">{pills}</span>
  ) : null;
}

/** A lane's solid colour: a status open, under way or done; a choice's own. */
function laneColour(field: Field, at: number, lanes: number) {
  if (field.kind !== "status") return CHOICE_DOTS[at % CHOICE_DOTS.length];
  return at === 0 ? "bg-faint" : at === lanes - 1 ? "bg-ok" : "bg-info";
}

/** A tally's word with its count: a button that picks its rows, where a click can. */
function TallyWord({
  pressed,
  off,
  onClick,
  children,
}: {
  pressed: boolean | undefined;
  off: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  const look = `flex items-center gap-1.5 ${off ? "opacity-50" : ""}`;
  if (pressed === undefined) return <span className={look}>{children}</span>;
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={`${look} -mx-1.5 rounded px-1.5 hover:bg-chip ${pressed ? "bg-chip text-fg" : ""}`}
    >
      {children}
    </button>
  );
}

/**
 * How many rows each status or choice has, over a collection's picture:
 * one bar of them all, open first and done last, and each lane's count
 * under it. Only when it groups: two lanes or more, and one with two rows.
 */
export function Tally({
  rows,
  field,
  chosen = null,
  onChoose,
}: {
  rows: Point[];
  field: Field;
  chosen?: string | null;
  onChoose?: (lane: string) => void;
}) {
  const lanes = lanesOf(field, rows);
  const counts = lanes.map((lane) => rows.filter((row) => cellOf(row, field) === lane).length);
  if (counts.filter((n) => n > 0).length < 2 || Math.max(...counts) < 2) return null;
  const all = counts.reduce((a, b) => a + b, 0);
  return (
    <div className="flex flex-col gap-1.5">
      <div aria-hidden="true" className="flex h-2 gap-0.5 overflow-hidden rounded-full">
        {lanes.map((lane, i) =>
          counts[i] ? (
            <span
              key={lane}
              className={`${laneColour(field, i, lanes.length)} ${chosen !== null && chosen !== lane ? "opacity-30" : ""}`}
              style={{ width: `${(counts[i] / all) * 100}%` }}
            />
          ) : null,
        )}
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
        {lanes.map((lane, i) =>
          counts[i] ? (
            <TallyWord
              key={lane}
              pressed={onChoose ? chosen === lane : undefined}
              off={chosen !== null && chosen !== lane}
              onClick={() => onChoose?.(lane)}
            >
              <span
                aria-hidden="true"
                className={`size-2 rounded-full ${laneColour(field, i, lanes.length)}`}
              />
              {lane}
              <span className="font-semibold text-fg tabular-nums">{counts[i]}</span>
            </TallyWord>
          ) : null,
        )}
      </div>
    </div>
  );
}

/** A colour as words, "#a78bfa": a figure shows it as a swatch. */
const HEX_COLOUR = /^#[0-9a-f]{6}$/i;

/**
 * Figures: a tile a row, its value large over its name, as a dashboard's
 * numbers. The value is its number, or else the first cell it has.
 */
/** The field a figure shows large: its number, or else the first that isn't a tone. */
export function figureField(fields: Field[]): Field | undefined {
  const shown = fields.filter((f) => f.kind !== "photo" && f.kind !== "check");
  return (
    shown.find((f) => f.kind === "number") ??
    shown.find((f) => !TOLD_BY_TONE.has(f.kind)) ??
    shown[0]
  );
}

function Figures({ rows, fields }: { rows: Point[]; fields: Field[] }) {
  const shown = fields.filter((f) => f.kind !== "photo" && f.kind !== "check");
  const value = figureField(fields);
  const others = shown.filter((f) => f !== value);
  // A palette's swatches fit narrower than words.
  const palette = rows.every(
    (row) => value && HEX_COLOUR.test(cellWords(value, cellOf(row, value))),
  );
  return (
    <div
      className={`grid gap-2.5 ${
        palette
          ? "grid-cols-[repeat(auto-fit,minmax(min(7rem,calc(50%-0.5rem)),1fr))]"
          : "grid-cols-[repeat(auto-fit,minmax(min(10rem,calc(50%-0.5rem)),1fr))]"
      }`}
    >
      {rows.map((row) => {
        const cell = value && cellOf(row, value);
        const words = value ? cellWords(value, cell) : "";
        const more = rest(others, row);
        return (
          <article
            key={row.id}
            className="group relative flex min-w-0 flex-col gap-1 rounded-lg bg-surface px-4 py-3"
          >
            {HEX_COLOUR.test(words) && (
              <span
                aria-hidden="true"
                className="mb-1 h-10 rounded-md ring-1 ring-line ring-inset"
                style={{ backgroundColor: words }}
              />
            )}
            <span className="flex items-start gap-2">
              <span
                className={`min-w-0 flex-1 font-semibold text-hue leading-tight tabular-nums ${
                  [...words].length > 9 ? "text-lg" : "text-2xl"
                }`}
              >
                {value && TOLD_BY_TONE.has(value.kind) && typeof cell === "string" ? (
                  <Pill field={value} value={cell} />
                ) : (
                  words || <span className="text-faint">–</span>
                )}
              </span>
              <RemovePoint point={row} />
            </span>
            <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted">
              <NewDot point={row} />
              <RowName row={row} className="truncate" />
            </span>
            {more && <span className="truncate text-xs text-faint">{more}</span>}
            <Pills fields={others} row={row} />
          </article>
        );
      })}
    </div>
  );
}

/** A step's number on the rail, ✓ once done; with a check, it ticks the step. */
function StepMark({
  row,
  check,
  done,
  n,
  className,
}: {
  row: Point;
  check?: Field;
  done: boolean;
  n: number;
  className: string;
}) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const rings = useItemReminders();
  const look = `relative grid size-7 shrink-0 place-items-center rounded-full font-semibold text-xs tabular-nums ${className}`;
  const face = done ? <Check size={14} strokeWidth={2.5} aria-hidden="true" /> : n;
  if (!check || guest) return <span className={look}>{face}</span>;
  const reminder = rings.get(row.id);
  return (
    <button
      type="button"
      aria-pressed={done}
      aria-label={`${row.text}: ${done ? "done" : "to do"}`}
      onClick={() => {
        if (reminder) return done ? markNotDone(sync, reminder) : markDone(sync, reminder);
        sync.update("Point", row.id, { cells: { ...row.cells, [check.name]: !done } });
      }}
      className={`${look} hover:opacity-80`}
    >
      {face}
    </button>
  );
}

/**
 * Steps: the rows in their order, numbered on a rail, as a way through.
 * With a check, a step done is ticked on its number, and the first still
 * to do stands out.
 */
function Steps({ rows, check, fields }: { rows: Point[]; check?: Field; fields: Field[] }) {
  const rings = useItemReminders();
  const done = (row: Point) => (check ? itemDone(row, check, rings.get(row.id)) : false);
  const next = check ? rows.find((row) => !done(row)) : undefined;
  return (
    <ol className={`flex flex-col ${LINES}`}>
      {rows.map((row, i) => {
        const finished = done(row);
        const words = rest(fields, row);
        const mark = finished
          ? "bg-ok text-bg"
          : row === next
            ? "bg-primary text-on-primary"
            : "bg-hue/15 text-hue";
        return (
          <li key={row.id} className="group relative flex gap-3 pb-3 last:pb-0">
            <span
              aria-hidden="true"
              className="absolute top-7 bottom-0 left-[13px] w-[1.5px] bg-line group-last:hidden"
            />
            <StepMark row={row} check={check} done={finished} className={mark} n={i + 1} />
            <div className="flex min-w-0 flex-1 flex-col pt-[3px]">
              <span className="flex items-baseline gap-2">
                <NewDot point={row} className="self-center" />
                <RowName
                  row={row}
                  className={`min-w-0 flex-1 font-medium ${finished ? "text-faint line-through decoration-faint" : ""}`}
                />
                <Pills fields={fields} row={row} />
                <RemovePoint point={row} />
              </span>
              {words && <span className="text-xs text-muted">{words}</span>}
            </div>
          </li>
        );
      })}
    </ol>
  );
}

/** A collection: its picture, and New under it to add a row by hand. */
export function Collection({ block, rows }: { block: Block; rows: Point[] }) {
  return (
    <div className="flex flex-col gap-2">
      <Picture block={block} rows={rows} />
      <NewRow block={block} rows={rows} />
    </div>
  );
}

function Picture({ block, rows }: { block: Block; rows: Point[] }) {
  const fields = filled(block, rows);
  const of = (...kinds: string[]) => fields.find((f) => kinds.includes(f.kind));
  const day = of("day");
  const number = of("number");
  // What's measured, over the picture, unless the block's title says it already.
  const heading =
    number && !block.title.toLowerCase().includes(number.name.toLowerCase())
      ? number.name
      : undefined;
  const check = checkOf(block);
  switch (block.view) {
    case "line":
      return day && number ? (
        <Line rows={rows} day={day} number={number} heading={heading} />
      ) : null;
    case "checklist":
      return check ? (
        <RowChecklist rows={rows} check={check} fields={fields.filter((f) => f !== check)} />
      ) : null;
    case "board": {
      const status = of("status", "choice");
      return status ? (
        <Board rows={rows} status={status} fields={fields.filter((f) => f !== status)} />
      ) : null;
    }
    case "timeline": {
      if (!day) return null;
      const by = of("status", "choice");
      const placed = placedOn(rows, day);
      return placed ? (
        <AxisTimeline
          placed={placed}
          rows={rows}
          by={by}
          day={day}
          fields={fields.filter((f) => f !== day)}
        />
      ) : (
        <Told rows={rows} by={by}>
          {(shown, rest) => (
            <RowTimeline rows={shown} day={day} fields={rest(fields).filter((f) => f !== day)} />
          )}
        </Told>
      );
    }
    case "week":
      return day ? <Week rows={rows} day={day} /> : null;
    case "bars":
      return number ? (
        <Told rows={rows} by={of("status", "choice")}>
          {/* The bars keep the tally's colours, picked or not. */}
          {(shown) => (
            <RowBars
              rows={shown}
              number={number}
              fields={fields.filter((f) => f !== number)}
              heading={heading}
            />
          )}
        </Told>
      ) : null;
    case "figures":
      return <Figures rows={rows} fields={fields} />;
    case "steps": {
      const ticks = block.fields.find((f) => f.kind === "check");
      return <Steps rows={rows} check={ticks} fields={fields.filter((f) => f !== ticks)} />;
    }
  }
  // Words read with where they come from, as a paragraph's do.
  return (
    <>
      <Told rows={rows} by={of("status", "choice")}>
        {(shown, rest) =>
          block.view === "cards" ? (
            <RowCards rows={shown} fields={rest(fields)} />
          ) : block.view === "table" ? (
            <RowTable block={block} rows={shown} fields={rest(fields)} />
          ) : (
            <RowList rows={shown} fields={rest(fields)} />
          )
        }
      </Told>
      <Sources points={rows} />
    </>
  );
}

/** The fewest rows, and years, a timeline draws an axis over. */
const AXIS_ROWS = 6;
const AXIS_YEARS = 10;

type Placed = { row: Point; span: [number, number] };

/** A timeline's rows with the years they cover, when there are enough of them over enough years. */
function placedOn(rows: Point[], day: Field): Placed[] | null {
  const placed = rows.flatMap((row) => {
    const cell = cellOf(row, day);
    const span = typeof cell === "string" ? years(cell) : null;
    return span ? [{ row, span }] : [];
  });
  if (placed.length < AXIS_ROWS) return null;
  const first = Math.min(...placed.map((p) => p.span[0]));
  const last = Math.max(...placed.map((p) => p.span[1]));
  return last - first >= AXIS_YEARS ? placed : null;
}

/** Round years to mark on an axis from `first` to `last`: about five, a step of 1, 2 or 5 times ten to a power. */
function ticksOf(first: number, last: number): number[] {
  const rough = (last - first) / 5;
  const power = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 5, 10].map((m) => m * power).find((s) => s >= rough) ?? rough;
  const ticks: number[] = [];
  for (let t = Math.ceil(first / step) * step; t <= last; t += step) ticks.push(t);
  return ticks;
}

/**
 * A timeline's years on one axis, a dot an event and a bar a span, so where
 * they cluster shows at a glance; a lane per status or choice when one
 * tells them apart, with how many each.
 */
/** Past this many rows, a timeline under its axis shows the first few, with All. */
const AXIS_LIST_SHOWS = 8;

/** An axis over the years, a lane per choice, which picks the rows the list under it shows. */
function AxisTimeline({
  placed,
  rows,
  by,
  day,
  fields,
}: {
  placed: Placed[];
  rows: Point[];
  by: Field | undefined;
  day: Field;
  fields: Field[];
}) {
  const [lane, setLane] = useState<string | null>(null);
  const [all, setAll] = useState(false);
  const shown = lane === null ? rows : rows.filter((row) => laneWord(row, by) === lane);
  const cut = lane === null && !all && shown.length > AXIS_LIST_SHOWS + 2;
  const listed = cut ? byDay(shown, day).slice(0, AXIS_LIST_SHOWS) : shown;
  return (
    <div className="flex flex-col gap-4">
      <TimeAxis
        placed={placed}
        rows={rows}
        by={by}
        chosen={lane}
        onChoose={(word) => setLane((was) => (was === word ? null : word))}
      />
      {/* Picked, the lane's word would only say it again on every row. */}
      <RowTimeline
        rows={listed}
        day={day}
        fields={lane === null ? fields : fields.filter((f) => f !== by)}
      />
      {(cut || all) && lane === null && (
        <button
          type="button"
          onClick={() => setAll((was) => !was)}
          className="self-start rounded-md px-2 py-1 text-sm text-muted hover:bg-chip hover:text-fg"
        >
          {all ? "Fewer" : `All ${shown.length}`}
        </button>
      )}
    </div>
  );
}

/** The word a row's choice says, or "" without one. */
function laneWord(row: Point, by: Field | undefined): string {
  const cell = by && cellOf(row, by);
  return typeof cell === "string" ? cell : "";
}

function TimeAxis({
  placed,
  rows,
  by,
  chosen,
  onChoose,
}: {
  placed: Placed[];
  rows: Point[];
  by: Field | undefined;
  chosen: string | null;
  onChoose: (lane: string) => void;
}) {
  const first = Math.min(...placed.map((p) => p.span[0]));
  const last = Math.max(...placed.map((p) => p.span[1]));
  const at = (year: number) => ((year - first) / (last - first)) * 100;
  const lanes = by ? [...lanesOf(by, rows), ""] : [""];
  return (
    <div
      title={`${placed.length} events from ${first} to ${last}`}
      className={`grid gap-x-3 gap-y-1 text-xs ${by ? "grid-cols-[max-content_1fr]" : "grid-cols-1"}`}
    >
      {lanes.map((lane, i) => {
        const inLane = placed.filter((p) => laneWord(p.row, by) === lane);
        const off = chosen !== null && chosen !== lane;
        if (inLane.length === 0) return null;
        const colour = by && lane ? laneColour(by, i, lanes.length - 1) : "bg-hue";
        return (
          <Fragment key={lane || "-"}>
            {by && (
              <button
                type="button"
                aria-pressed={chosen === lane}
                onClick={() => onChoose(lane)}
                className={`-mx-1.5 flex items-center gap-1.5 rounded px-1.5 text-left hover:bg-chip ${
                  chosen === lane ? "bg-chip text-fg" : "text-muted"
                } ${off ? "opacity-50" : ""}`}
              >
                {lane || "–"}
                <span className="font-semibold text-fg tabular-nums">{inLane.length}</span>
              </button>
            )}
            <span aria-hidden="true" className={`relative h-4 ${off ? "opacity-30" : ""}`}>
              <span className="absolute inset-x-0 top-1/2 h-px bg-line" />
              {inLane.map(({ row, span: [from, to] }) => (
                <span
                  key={row.id}
                  title={`${from === to ? from : `${from}–${to}`}: ${row.text}`}
                  className={`absolute top-1 h-2 rounded-full ${colour} ${to > from ? "opacity-70" : "-ml-1 w-2"}`}
                  style={
                    to > from
                      ? { left: `${at(from)}%`, width: `max(0.5rem, ${at(to) - at(from)}%)` }
                      : { left: `${at(from)}%` }
                  }
                />
              ))}
            </span>
          </Fragment>
        );
      })}
      {by && <span />}
      <span className="relative h-4 text-faint tabular-nums">
        <span className="absolute left-0">{first}</span>
        {/* Clear of the ends, whose years are always there. */}
        {ticksOf(first, last)
          .filter((t) => at(t) > 20 && at(t) < 80)
          .map((t) => (
            <span key={t} className="absolute -translate-x-1/2" style={{ left: `${at(t)}%` }}>
              {t}
            </span>
          ))}
        <span className="absolute right-0">{last}</span>
      </span>
    </div>
  );
}

/** A picture whose rows a status or a choice tells apart, with how many each over it. */
function Told({
  rows,
  by,
  children,
}: {
  rows: Point[];
  by: Field | undefined;
  /** The picture of the rows a click on the tally picked, its fields through `rest`. */
  children: (shown: Point[], rest: (fields: Field[]) => Field[]) => ReactNode;
}) {
  const [picked, setPicked] = useState<string | null>(null);
  const all = (fields: Field[]) => fields;
  if (!by || rows.length < TALLY_FROM) return children(rows, all);
  const lane = picked !== null && rows.some((row) => cellOf(row, by) === picked) ? picked : null;
  return (
    <div className="flex flex-col gap-3">
      <Tally
        rows={rows}
        field={by}
        chosen={lane}
        onChoose={(word) => setPicked((was) => (was === word ? null : word))}
      />
      {lane === null
        ? children(rows, all)
        : children(
            rows.filter((row) => cellOf(row, by) === lane),
            // Picked, its word would only say it again on every row.
            (fields) => fields.filter((f) => f !== by),
          )}
    </div>
  );
}

/** The fewest rows a tally counts over: under it, the rows are seen at a glance. */
const TALLY_FROM = 4;

/**
 * A line: a number over the days, with the goal its field names, dashed.
 * One measurement is the number alone, on its day.
 */
function Line({
  rows,
  day,
  number,
  heading,
}: {
  rows: Point[];
  day: Field;
  number: Field;
  heading?: string;
}) {
  const measured: Point[] = rows.flatMap((row) => {
    const happened = dayOf(row, day);
    const amount = numberOf(row, number);
    return happened && amount !== null ? [{ ...row, happened, amount, note: number.unit }] : [];
  });
  if (measured.length === 1) {
    const [only] = measured;
    return (
      <p className="flex flex-wrap items-baseline gap-x-2.5">
        <span className="font-semibold text-wordmark tabular-nums">
          {cellWords(number, only?.amount ?? 0)}
        </span>
        <span className="text-xs text-muted">on {dateWords(only?.happened ?? "")}</span>
      </p>
    );
  }
  const goal: Point[] =
    number.goal === null || !measured[0]
      ? []
      : [{ ...measured[0], id: "goal", text: "Goal", happened: "", amount: number.goal }];
  return <TrendSet name={number.name} points={[...measured, ...goal]} heading={heading} />;
}

/**
 * A board: a lane per status, its first still open and its last done, and
 * a card per row in its lane. A row with no status waits in the first. A
 * choice's lanes are only its words, with no step from open to done.
 * A lane is as wide as its cards ask, and a wide one sets them side by
 * side, so one full lane beside two thin ones doesn't run down the page.
 * A card dragged to another lane takes its status or choice.
 */
function Board({ rows, status, fields }: { rows: Point[]; status: Field; fields: Field[] }) {
  const pages = useRowPages(rows);
  const guest = useIsGuest();
  const setCell = useSetCell();
  const [dragged, setDragged] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const steps = status.kind === "status";
  const lanes = lanesOf(status, rows);
  const laneOf = (row: Point) => {
    const cell = cellOf(row, status);
    return typeof cell === "string" ? cell : lanes[0];
  };
  return (
    <div className="flex flex-wrap gap-3">
      {lanes.map((lane, i) => {
        const cards = rows.filter((row) => laneOf(row) === lane);
        const dot = !steps
          ? CHOICE_DOTS[i % CHOICE_DOTS.length]
          : i === 0
            ? "ring-[1.5px] ring-faint ring-inset"
            : i === lanes.length - 1
              ? "bg-ok"
              : "bg-info";
        return (
          <section
            key={lane}
            aria-label={lane}
            onDragOver={(event) => {
              if (!dragged) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              setOver(lane);
            }}
            onDragLeave={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver(null);
            }}
            onDrop={(event) => {
              const row = rows.find((r) => r.id === dragged);
              if (!row) return;
              event.preventDefault();
              if (laneOf(row) !== lane) setCell(row, status, lane);
              setDragged(null);
              setOver(null);
            }}
            className={`-m-1 flex min-w-0 basis-52 flex-col gap-1.5 rounded-lg p-1 ${over === lane ? "bg-hover" : ""}`}
            style={{ flexGrow: Math.max(cards.length, 1) }}
          >
            <h3 className="flex items-center gap-2 text-xs text-muted">
              <span aria-hidden="true" className={`size-2 rounded-full ${dot}`} />
              {lane}
              <span className="text-faint tabular-nums">{cards.length}</span>
            </h3>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(min(13rem,100%),1fr))] gap-1.5">
              {cards.map((row) => {
                const words = rest(fields, row);
                const kept = words ? [] : (pages.get(row.id) ?? []);
                const tint = choiceTint([status], row);
                return (
                  <article
                    key={row.id}
                    draggable={!guest}
                    onDragStart={(event) => {
                      event.dataTransfer.effectAllowed = "move";
                      event.dataTransfer.setData("text/plain", row.text);
                      setDragged(row.id);
                    }}
                    onDragEnd={() => {
                      setDragged(null);
                      setOver(null);
                    }}
                    style={tint}
                    className={`group flex items-start gap-2 rounded-md px-3 py-2 ${tint ? "bg-tinted" : "bg-surface"} ${guest ? "" : "cursor-grab active:cursor-grabbing"} ${dragged === row.id ? "opacity-50" : ""}`}
                  >
                    <NewDot point={row} className="mt-2" />
                    <Thumb row={row} fields={fields} className="size-10" />
                    <div className="flex min-w-0 flex-1 flex-col">
                      <RowName row={row} className="line-clamp-2 break-words" />
                      {words && <span className="truncate text-xs text-faint">{words}</span>}
                      {kept.length > 0 && (
                        <div className="mt-1.5">
                          <PageNames words={kept} />
                        </div>
                      )}
                    </div>
                    <Pills fields={fields} row={row} />
                    <RemovePoint point={row} />
                  </article>
                );
              })}
            </div>
          </section>
        );
      })}
    </div>
  );
}

/** Cards: a card per row, its photo on top, its name, then each field it fills. */
/**
 * Cards: a card a row, its photo over its name, then its cells, each by
 * its field's name; words past a line read under the name without it, and
 * what the row's own page keeps shows as chips.
 */
function RowCards({ rows, fields }: { rows: Point[]; fields: Field[] }) {
  const photo = fields.find((f) => f.kind === "photo");
  const shown = fields.filter((f) => f !== photo);
  const pages = useRowPages(rows);
  const long = (row: Point, field: Field) => {
    const cell = cellOf(row, field);
    return field.kind === "text" && typeof cell === "string" && cell.length > BESIDE_MOST;
  };
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(min(15rem,100%),1fr))] gap-3">
      {rows.map((row) => {
        const pic = photo && cellOf(row, photo);
        const tint = choiceTint(shown, row);
        return (
          <article
            key={row.id}
            style={tint}
            className={`group flex flex-col gap-2.5 rounded-lg p-3 ${tint ? "bg-tinted" : "bg-surface"}`}
          >
            {typeof pic === "string" && (
              <Photo id={pic} className="aspect-[4/3] w-full rounded-md" />
            )}
            <div className="flex items-center gap-2">
              <NewDot point={row} />
              <h3 className="min-w-0 flex-1 truncate font-semibold">
                <RowName row={row} />
              </h3>
              <RemovePoint point={row} />
            </div>
            {shown
              .filter((field) => long(row, field))
              .map((field) => (
                <EditCell key={field.name} row={row} field={field} className="text-muted">
                  {inline(String(cellOf(row, field)))}
                </EditCell>
              ))}
            <dl className="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-xs empty:hidden">
              {shown.map((field) => {
                const cell = cellOf(row, field);
                if (cell === undefined || cell === false || long(row, field)) return null;
                return (
                  <div key={field.name} className="col-span-full grid grid-cols-subgrid">
                    <dt className="text-faint">{field.name}</dt>
                    <dd className="min-w-0 truncate">
                      <EditCell row={row} field={field} className="truncate">
                        {field.kind === "check" ? (
                          <span className="text-ok-ink">✓</span>
                        ) : TOLD_BY_TONE.has(field.kind) && typeof cell === "string" ? (
                          <Pill field={field} value={cell} />
                        ) : (
                          cellWords(field, cell)
                        )}
                      </EditCell>
                    </dd>
                  </div>
                );
              })}
            </dl>
            <PageNames words={pages.get(row.id) ?? []} />
          </article>
        );
      })}
    </div>
  );
}

/** A table: a row per row, its name first, a column per field it fills. Photos stay on cards. */
function RowTable({ block, rows, fields }: { block: Block; rows: Point[]; fields: Field[] }) {
  const columns = fields.filter((f) => f.kind !== "photo");
  // A number's bar is on its column's largest, so the column reads at a glance.
  const most = new Map(
    columns
      .filter((f) => f.kind === "number" && new Set(rows.map((r) => numberOf(r, f))).size > 2)
      .map((f) => [f.name, Math.max(...rows.map((r) => Math.abs(numberOf(r, f) ?? 0)))]),
  );
  // What a column of amounts with a unit comes to, under it; a row that's
  // a part of the one above ("davon …") is in that one's already.
  const sums = new Map(
    columns
      .filter((f) => f.kind === "number" && f.unit && rows.filter((r) => !isPart(r)).length > 1)
      .map((f) => [f.name, sumOf(rows, f)]),
  );
  const cell = "h-[34px] border-line border-r border-b px-3 last:border-r-0";
  const header =
    "h-8 border-line border-r border-b bg-surface px-3 text-left font-semibold text-xs text-muted last:border-r-0";
  return (
    <div className="overflow-x-auto rounded-md ring-1 ring-line ring-inset">
      <table
        aria-label={block.title}
        className="w-full border-collapse whitespace-nowrap tabular-nums [&_tr:last-child>td]:border-b-0"
      >
        <thead>
          <tr>
            <th className={header}>Name</th>
            {columns.map((field) => (
              <th
                key={field.name}
                className={`${header} ${field.kind === "number" ? "text-right" : ""}`}
              >
                {field.name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id} className="group [&>td]:hover:bg-hover">
              <td className={`${cell} max-w-72 truncate font-semibold`}>
                <span className="flex items-center gap-2.5">
                  <Thumb row={row} fields={fields} className="size-6" />
                  <RowName
                    row={row}
                    className={`truncate ${isPart(row) ? "pl-4 font-normal text-muted" : ""}`}
                  />
                  <span className="ml-auto font-normal">
                    <RemovePoint point={row} />
                  </span>
                </span>
              </td>
              {columns.map((field) => {
                const value = cellOf(row, field);
                const words =
                  field.kind === "check" ? (value === true ? "✓" : "") : cellWords(field, value);
                return (
                  <td
                    key={field.name}
                    className={`${cell} ${field.kind === "text" ? "min-w-40 whitespace-normal" : "max-w-64 truncate"} ${field.kind === "number" ? "text-right" : ""} ${field.kind === "check" ? "text-ok-ink" : ""}`}
                  >
                    <EditCell
                      row={row}
                      field={field}
                      className={field.kind === "text" ? "block" : "block truncate"}
                    >
                      {TOLD_BY_TONE.has(field.kind) && typeof value === "string" ? (
                        <Pill field={field} value={value} />
                      ) : typeof value === "number" && most.get(field.name) ? (
                        <span className="flex items-center justify-end gap-2.5">
                          <span
                            aria-hidden="true"
                            className="h-1.5 w-12 shrink-0 overflow-hidden rounded-full bg-chip"
                          >
                            <span
                              className="block h-full rounded-full bg-hue"
                              style={{
                                width: `${(Math.abs(value) / (most.get(field.name) ?? 1)) * 100}%`,
                              }}
                            />
                          </span>
                          {words}
                        </span>
                      ) : (
                        words || <span className="text-faint">–</span>
                      )}
                    </EditCell>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
        {sums.size > 0 && (
          <tfoot>
            <tr className="font-semibold">
              <td className={`${cell} border-t text-muted`}>Total</td>
              {columns.map((field) => {
                const sum = sums.get(field.name);
                return (
                  <td key={field.name} className={`${cell} border-t text-right`}>
                    {sum === undefined ? "" : cellWords(field, Math.round(sum * 100) / 100)}
                  </td>
                );
              })}
            </tr>
          </tfoot>
        )}
      </table>
    </div>
  );
}

/**
 * A timeline: a row a line, oldest first, by its day or span. A day shows
 * as a date, with its year when it isn't this one.
 */
function RowTimeline({ rows, day, fields }: { rows: Point[]; day: Field; fields: Field[] }) {
  const year = today().slice(0, 4);
  return (
    <TimelineRail>
      {byDay(rows, day).map((row) => {
        const cell = cellOf(row, day);
        const at = typeof cell === "string" ? when(cell) : null;
        const iso = dayOf(row, day);
        const label = iso
          ? `${dateWords(iso)}${iso.startsWith(year) ? "" : `, ${iso.slice(0, 4)}`}`
          : (at?.year ?? "");
        const words = rest(fields, row);
        return (
          <TimelineLine key={row.id} year={label} span={at?.span ?? false} seen={row.seen}>
            <div className="flex min-w-0 items-start gap-2 py-[5px]">
              <Thumb row={row} fields={fields} className="size-10" />
              <p className="min-w-0 flex-1">
                {!row.seen && <span className="sr-only">New: </span>}
                <RowName row={row} />
                {words && <span className="text-faint"> · {words}</span>}
              </p>
              <Pills fields={fields} row={row} />
              <RemovePoint point={row} />
            </div>
          </TimelineLine>
        );
      })}
    </TimelineRail>
  );
}

/** How many rows past the week the line under it names. */
const LATER_SHOWS = 3;

/**
 * A week: seven days, from today while any row is still to come, else the
 * seven up to the latest, each with its rows; the next few after it below.
 */
function Week({ rows, day }: { rows: Point[]; day: Field }) {
  const now = today();
  const days = rows.flatMap((row) => dayOf(row, day) ?? []).sort();
  const latest = days.at(-1) ?? now;
  const first = latest >= now ? now : addDays(latest, -6);
  const week = Array.from({ length: 7 }, (_, i) => addDays(first, i));
  const last = week.at(-1) ?? first;
  const later = rows
    .filter((row) => (dayOf(row, day) ?? "") > last)
    .sort((a, b) => (dayOf(a, day) ?? "").localeCompare(dayOf(b, day) ?? ""));
  return (
    <div className="flex flex-col gap-2">
      <div className="grid grid-cols-7 gap-1">
        {week.map((key) => {
          const due = rows.filter((row) => dayOf(row, day) === key);
          return (
            <div
              key={key}
              title={weekdayWords(key)}
              aria-current={key === now ? "date" : undefined}
              className="flex min-h-20 min-w-0 flex-col gap-1.5 rounded-md bg-surface p-2 aria-[current=date]:bg-selected"
            >
              <span className="flex flex-wrap items-baseline gap-x-1.5">
                <span
                  className={`font-semibold text-xl tabular-nums leading-none ${key === now ? "text-accent" : due.length ? "" : "text-faint"}`}
                >
                  {dayOfMonth(key)}
                </span>
                <span className="text-xs text-muted">{weekdayShort(key)}</span>
              </span>
              {due.map((row) => (
                <span
                  key={row.id}
                  className="flex min-w-0 items-center gap-1.5 text-xs"
                  title={row.text}
                >
                  <span aria-hidden="true" className="size-1.5 shrink-0 rounded-full bg-accent" />
                  <span className="truncate">
                    <RowName row={row} />
                  </span>
                </span>
              ))}
              {due.length === 0 && <span className="sr-only">Nothing</span>}
            </div>
          );
        })}
      </div>
      {later.length > 0 && (
        <p className="text-xs text-faint">
          Later:{" "}
          {later
            .slice(0, LATER_SHOWS)
            .map((row) => `${row.text}, ${dateWords(dayOf(row, day) ?? "")}`)
            .join(" · ")}
          {later.length > LATER_SHOWS && ` · ${later.length - LATER_SHOWS} more`}
        </p>
      )}
    </div>
  );
}

/**
 * Bars: a bar per row with a number, on the largest, its other cells
 * beside it. A goal draws as a mark on every bar's track, and a status or
 * a choice colours the bars by its pill.
 */
function RowBars({
  rows,
  number,
  fields,
  heading,
}: {
  rows: Point[];
  number: Field;
  fields: Field[];
  heading?: string;
}) {
  const bars = rows.filter((row) => numberOf(row, number) !== null);
  const goal = number.goal ?? null;
  const most = Math.max(
    ...bars.map((row) => Math.abs(numberOf(row, number) ?? 0)),
    Math.abs(goal ?? 0),
    0,
  );
  const told = fields.find((f) => TOLD_BY_TONE.has(f.kind));
  const fill = (row: Point) => {
    const cell = told && cellOf(row, told);
    if (!told) return "bg-hue";
    // Where a choice colours the bars, the topic's hue steps aside.
    if (typeof cell !== "string") return "bg-faint";
    const at = told.options.indexOf(cell);
    if (told.kind === "choice") return at >= 0 ? CHOICE_DOTS[at % CHOICE_DOTS.length] : "bg-faint";
    return at > 0 && at === told.options.length - 1 ? "bg-ok" : at > 0 ? "bg-info" : "bg-faint";
  };
  const words = (amount: number) =>
    number.unit ? `${amountWords(amount, number.unit)} ${number.unit}` : amountWords(amount);
  return (
    <section className="@container flex flex-col gap-1.5" aria-label={number.name}>
      {(heading || goal !== null) && (
        <h3 className="flex gap-3 text-xs text-muted">
          {heading && <span>{heading}</span>}
          {goal !== null && (
            <span className="flex items-center gap-1.5">
              <span aria-hidden="true" className="h-3 w-[2px] rounded-full bg-warn" />
              Goal {words(goal)}
            </span>
          )}
        </h3>
      )}
      <ul className="grid max-w-5xl grid-cols-[fit-content(45%)_minmax(5rem,1fr)_max-content] gap-x-3.5 @md:grid-cols-[fit-content(32%)_minmax(6rem,1fr)_max-content_fit-content(32%)]">
        {bars.map((row) => {
          const amount = numberOf(row, number) ?? 0;
          const more = rest(fields, row);
          const pills = fields.some(
            (f) => TOLD_BY_TONE.has(f.kind) && cellOf(row, f) !== undefined,
          );
          return (
            <li
              key={row.id}
              className="group col-span-full -mx-2.5 grid min-h-7.5 grid-cols-subgrid items-center rounded-md px-2.5 py-1 hover:bg-hover"
            >
              <span className="flex min-w-0 items-center gap-2">
                <NewDot point={row} />
                <Thumb row={row} fields={fields} className="size-7" />
                <RowName row={row} className="line-clamp-2 min-w-0" />
              </span>
              <span aria-hidden="true" className="relative block h-2.5 rounded-full bg-chip">
                <span
                  className={`block h-full min-w-[3px] rounded-full ${fill(row)}`}
                  style={{ width: `${most ? (Math.abs(amount) / most) * 100 : 0}%` }}
                />
                {goal !== null && most > 0 && (
                  <span
                    className="absolute -top-1 -bottom-1 w-[2px] rounded-full bg-warn"
                    style={{ left: `calc(${(Math.abs(goal) / most) * 100}% - 1px)` }}
                  />
                )}
              </span>
              <span className="text-right font-semibold tabular-nums">{words(amount)}</span>
              {/* Under the bar when the page is narrow, beside it when there's room. */}
              <span
                className={`col-span-full min-w-0 items-center gap-2 @md:col-span-1 @md:flex ${more || pills ? "flex" : "hidden"}`}
              >
                <span
                  title={more || undefined}
                  className="min-w-0 flex-1 truncate text-xs text-faint"
                >
                  {more}
                </span>
                <Pills fields={fields} row={row} />
                <RemovePoint point={row} />
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/**
 * A checklist: a row a line to tick, its name, then its day; what's done
 * goes faint, not away.
 */
function RowChecklist({ rows, check, fields }: { rows: Point[]; check: Field; fields: Field[] }) {
  const rings = useItemReminders();
  return (
    <ul className={`flex flex-col ${LINES}`}>
      {rows.map((row) => {
        const reminder = rings.get(row.id);
        const done = itemDone(row, check, reminder);
        const words = rest(fields, row);
        return (
          <li key={row.id} className="group flex items-baseline gap-3 rounded-md px-2.5 py-1.5">
            <Tick row={row} check={check} reminder={reminder} className="self-center" />
            <NewDot point={row} className="self-center" />
            <Thumb row={row} fields={fields} className="size-8 self-center" />
            <p className={`min-w-0 flex-1 ${done ? "text-faint" : ""}`}>
              <RowName row={row} />
              {words &&
                (words.length > BESIDE_MOST ? (
                  <span className="block text-muted">{inline(words)}</span>
                ) : (
                  <span className="text-muted"> · {words}</span>
                ))}
            </p>
            <Pills fields={fields} row={row} />
            {reminder && <Rings reminder={reminder} />}
            <RemovePoint point={row} />
          </li>
        );
      })}
    </ul>
  );
}

/** The most chips a card names of what its row's page keeps. */
const PAGE_NAMES = 4;

/**
 * What each row's own page keeps, in a word each: its collections' row
 * names. Only where rows open a page (`Opens`).
 */
function useRowPages(rows: Point[]): Map<string, string[]> {
  const opens = useContext(Opens) !== null;
  const blocks = useRecords("Block");
  const points = useRecords("Point");
  return useMemo(() => {
    const kept = new Map<string, string[]>();
    if (!opens) return kept;
    const names = new Map(rows.map((r) => [r.id, r.text.trim().toLowerCase()]));
    const pages = blocks
      .filter((b) => b.row_id && names.has(b.row_id) && b.kind === "collection")
      .sort((a, b) => a.position - b.position);
    for (const page of pages) {
      const row = page.row_id as string;
      const words = points
        .filter((p) => p.block_id === page.id)
        .sort((a, b) => a.position - b.position)
        .map((p) => p.text)
        // A name that only says the row's own again says nothing.
        .filter((w): w is string => !!w && w.trim().toLowerCase() !== names.get(row));
      kept.set(row, [...(kept.get(row) ?? []), ...words]);
    }
    return kept;
  }, [opens, rows, blocks, points]);
}

/** What a row's page keeps, a chip a name, the first few. */
function PageNames({ words }: { words: string[] }) {
  const names = [...new Set(words)];
  if (names.length === 0) return null;
  const more = names.length - PAGE_NAMES;
  return (
    <ul className="flex flex-wrap gap-1 text-xs">
      {names.slice(0, PAGE_NAMES).map((name) => (
        <li key={name} className="max-w-full truncate rounded bg-chip px-1.5 py-0.5 text-muted">
          {name}
        </li>
      ))}
      {more > 0 && <li className="px-1 py-0.5 text-faint tabular-nums">+{more}</li>}
    </ul>
  );
}

/** Past this a row's words go under its name, not beside it: what a term means. */
const BESIDE_MOST = 60;

/** A list: a row a line, its name, then its other cells. */
function RowList({ rows, fields }: { rows: Point[]; fields: Field[] }) {
  return (
    <ul className={`flex flex-col ${LINES}`}>
      {rows.map((row) => {
        const words = rest(fields, row);
        return (
          <li key={row.id} className="group flex items-baseline gap-2 rounded-md px-2.5 py-1.5">
            <NewDot point={row} className="self-center" />
            {!fields.some((f) => f.kind === "photo") && (
              <span
                aria-hidden="true"
                className="size-1.5 shrink-0 translate-y-[-2px] rounded-full bg-hue"
              />
            )}
            <Thumb row={row} fields={fields} className="size-8 self-center" />
            <p className="min-w-0 flex-1">
              {/* A name alone is the line; beside its cells it leads. */}
              <RowName row={row} className={fields.length > 0 ? "font-semibold" : ""} />
              {words &&
                (words.length > BESIDE_MOST ? (
                  <span className="block text-muted">{inline(words)}</span>
                ) : (
                  <span className="text-muted"> · {words}</span>
                ))}
            </p>
            <Pills fields={fields} row={row} />
            <RemovePoint point={row} />
          </li>
        );
      })}
    </ul>
  );
}
