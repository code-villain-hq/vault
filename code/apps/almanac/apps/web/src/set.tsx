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
  createContext,
  type ReactNode,
  useContext,
  useMemo,
  useState,
} from "react";
import { TrendSet } from "./charts";
import { byDay, cellOf, cellWords, checkOf, dayOf, filled, lanesOf, numberOf } from "./collection";
import {
  addDays,
  amountWords,
  dateWords,
  dueWords,
  rhythmWords,
  today,
  weekdayWords,
} from "./format";
import { usePhoto } from "./photos";
import { inline } from "./prose";
import { doneFor, isDue, markDone, markNotDone } from "./reminders";
import { when } from "./timeline";
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
    <span className="flex items-center gap-2 text-xs text-faint tabular-nums">
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
    .filter((field) => !TOLD_BY_TONE.has(field.kind))
    .flatMap((field) => {
      const cell = cellOf(row, field);
      if (field.kind === "check") return cell === true ? [`✓ ${field.name}`] : [];
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

/** A choice's options told apart by a dot each, in its order. */
const CHOICE_DOTS = ["bg-info", "bg-accent", "bg-warn", "bg-signal", "bg-ok", "bg-faint"];

/**
 * How a status's or a choice's word looks: a status still open at first
 * (its first option), done at its last and under way between, as a
 * board's lanes; a choice's options a dot each.
 */
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
  const dot = at >= 0 ? CHOICE_DOTS[at % CHOICE_DOTS.length] : "bg-faint";
  return (
    <span className="inline-flex max-w-full shrink-0 items-center gap-1.5 truncate rounded-full bg-chip px-2 py-px text-xs">
      <span aria-hidden="true" className={`size-1.5 shrink-0 rounded-full ${dot}`} />
      {value}
    </span>
  );
}

/** A row's statuses and choices, a pill each. */
function Pills({ fields, row }: { fields: Field[]; row: Point }) {
  const pills = fields.flatMap((field) => {
    const cell = cellOf(row, field);
    return TOLD_BY_TONE.has(field.kind) && typeof cell === "string"
      ? [<Pill key={field.name} field={field} value={cell} />]
      : [];
  });
  return pills.length > 0 ? (
    <span className="flex shrink-0 gap-1.5 self-center">{pills}</span>
  ) : null;
}

/**
 * How many rows each status or choice has, over a collection's picture: a
 * figure each, and one bar of them all, open first and done last.
 */
export function Tally({ rows, field }: { rows: Point[]; field: Field }) {
  const lanes = lanesOf(field, rows);
  const counts = lanes.map((lane) => rows.filter((row) => cellOf(row, field) === lane).length);
  if (counts.filter((n) => n > 0).length < 2) return null;
  return (
    <div className="flex flex-col gap-2">
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(8rem,100%),1fr))] gap-2">
        {lanes.map((lane, i) => (
          <div key={lane} className="flex flex-col gap-0.5 rounded-lg bg-surface px-3.5 py-2.5">
            <span className="font-semibold text-2xl tabular-nums leading-tight">{counts[i]}</span>
            <Pill field={field} value={lane} />
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Figures: a tile a row, its value large over its name, as a dashboard's
 * numbers. The value is its number, or else the first cell it has.
 */
function Figures({ rows, fields }: { rows: Point[]; fields: Field[] }) {
  const shown = fields.filter((f) => f.kind !== "photo" && f.kind !== "check");
  const value =
    shown.find((f) => f.kind === "number") ??
    shown.find((f) => !TOLD_BY_TONE.has(f.kind)) ??
    shown[0];
  const others = shown.filter((f) => f !== value);
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(min(10rem,100%),1fr))] gap-2.5">
      {rows.map((row) => {
        const cell = value && cellOf(row, value);
        const words = value ? cellWords(value, cell) : "";
        const more = rest(others, row);
        return (
          <article
            key={row.id}
            className="group relative flex min-w-0 flex-col gap-1 rounded-lg bg-surface px-4 py-3"
          >
            <span className="flex items-start gap-2">
              <span
                className={`min-w-0 flex-1 font-semibold leading-tight tabular-nums ${
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
            : "bg-surface text-muted ring-1 ring-line ring-inset";
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

export function Collection({ block, rows }: { block: Block; rows: Point[] }) {
  const fields = filled(block, rows);
  const of = (...kinds: string[]) => fields.find((f) => kinds.includes(f.kind));
  const day = of("day");
  const number = of("number");
  // What's measured, over the picture, unless the block's title says it already.
  const heading = number && number.name !== block.title ? number.name : undefined;
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
    case "timeline":
      return day ? (
        <RowTimeline rows={rows} day={day} fields={fields.filter((f) => f !== day)} />
      ) : null;
    case "week":
      return day ? <Week rows={rows} day={day} /> : null;
    case "bars":
      return number ? (
        <RowBars
          rows={rows}
          number={number}
          fields={fields.filter((f) => f !== number)}
          heading={heading}
        />
      ) : null;
    case "figures":
      return <Figures rows={rows} fields={fields} />;
    case "steps": {
      const ticks = block.fields.find((f) => f.kind === "check");
      return <Steps rows={rows} check={ticks} fields={fields.filter((f) => f !== ticks)} />;
    }
  }
  // Rows told apart by a status or a choice count each over the picture.
  const told = of("status", "choice");
  const picture =
    block.view === "cards" ? (
      <RowCards rows={rows} fields={fields} />
    ) : block.view === "table" ? (
      <RowTable block={block} rows={rows} fields={fields} />
    ) : (
      <RowList rows={rows} fields={fields} />
    );
  if (!told || rows.length < TALLY_FROM) return picture;
  return (
    <div className="flex flex-col gap-3">
      <Tally rows={rows} field={told} />
      {picture}
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
 */
function Board({ rows, status, fields }: { rows: Point[]; status: Field; fields: Field[] }) {
  const steps = status.kind === "status";
  const lanes = lanesOf(status, rows);
  const laneOf = (row: Point) => {
    const cell = cellOf(row, status);
    return typeof cell === "string" ? cell : lanes[0];
  };
  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(min(13rem,100%),1fr))] gap-3">
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
          <section key={lane} aria-label={lane} className="flex min-w-0 flex-col gap-1.5">
            <h3 className="flex items-center gap-2 text-xs text-muted">
              <span aria-hidden="true" className={`size-2 rounded-full ${dot}`} />
              {lane}
              <span className="text-faint tabular-nums">{cards.length}</span>
            </h3>
            {cards.map((row) => {
              const words = rest(fields, row);
              return (
                <article
                  key={row.id}
                  className="group flex items-start gap-2 rounded-md bg-surface px-3 py-2"
                >
                  <NewDot point={row} className="mt-2" />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <RowName row={row} className="truncate" />
                    {words && <span className="truncate text-xs text-faint">{words}</span>}
                  </div>
                  <Pills fields={fields} row={row} />
                  <RemovePoint point={row} />
                </article>
              );
            })}
          </section>
        );
      })}
    </div>
  );
}

/** Cards: a card per row, its photo on top, its name, then each field it fills. */
function RowCards({ rows, fields }: { rows: Point[]; fields: Field[] }) {
  const photo = fields.find((f) => f.kind === "photo");
  const shown = fields.filter((f) => f !== photo);
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(min(15rem,100%),1fr))] gap-3">
      {rows.map((row) => {
        const pic = photo && cellOf(row, photo);
        return (
          <article key={row.id} className="group flex flex-col gap-2.5 rounded-lg bg-surface p-3">
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
            <dl className="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-xs">
              {shown.map((field) => {
                const cell = cellOf(row, field);
                if (cell === undefined || cell === false) return null;
                return (
                  <div key={field.name} className="col-span-full grid grid-cols-subgrid">
                    <dt className="text-faint">{field.name}</dt>
                    <dd className="min-w-0 truncate">
                      {field.kind === "check" ? (
                        <span className="text-ok-ink">✓</span>
                      ) : TOLD_BY_TONE.has(field.kind) && typeof cell === "string" ? (
                        <Pill field={field} value={cell} />
                      ) : (
                        cellWords(field, cell)
                      )}
                    </dd>
                  </div>
                );
              })}
            </dl>
          </article>
        );
      })}
    </div>
  );
}

/** A table: a row per row, its name first, a column per field it fills. Photos stay on cards. */
function RowTable({ block, rows, fields }: { block: Block; rows: Point[]; fields: Field[] }) {
  const columns = fields.filter((f) => f.kind !== "photo");
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
            <tr key={row.id} className="[&>td]:hover:bg-hover">
              <td className={`${cell} max-w-72 truncate font-semibold`}>
                <RowName row={row} />
              </td>
              {columns.map((field) => {
                const value = cellOf(row, field);
                const words =
                  field.kind === "check" ? (value === true ? "✓" : "") : cellWords(field, value);
                return (
                  <td
                    key={field.name}
                    className={`${cell} max-w-64 truncate ${field.kind === "number" ? "text-right" : ""} ${field.kind === "check" ? "text-ok-ink" : ""}`}
                  >
                    {TOLD_BY_TONE.has(field.kind) && typeof value === "string" ? (
                      <Pill field={field} value={value} />
                    ) : (
                      words || <span className="text-faint">–</span>
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
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
              aria-current={key === now ? "date" : undefined}
              className="flex min-h-16 min-w-0 flex-col gap-1 rounded-md bg-surface p-2 aria-[current=date]:bg-selected"
            >
              <span className="text-xs text-muted">{weekdayWords(key)}</span>
              {due.map((row) => (
                <span key={row.id} className="truncate text-xs" title={row.text}>
                  <RowName row={row} />
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
    if (!told || typeof cell !== "string") return "bg-fg/40";
    const at = told.options.indexOf(cell);
    if (told.kind === "choice") return at >= 0 ? CHOICE_DOTS[at % CHOICE_DOTS.length] : "bg-faint";
    return at > 0 && at === told.options.length - 1 ? "bg-ok" : at > 0 ? "bg-info" : "bg-faint";
  };
  const words = (amount: number) =>
    number.unit ? `${amountWords(amount)} ${number.unit}` : amountWords(amount);
  return (
    <section className="flex flex-col gap-1.5" aria-label={number.name}>
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
      <ul className="grid max-w-5xl grid-cols-[minmax(0,max-content)_minmax(4rem,1fr)_max-content_minmax(0,max-content)] gap-x-3.5">
        {bars.map((row) => {
          const amount = numberOf(row, number) ?? 0;
          return (
            <li
              key={row.id}
              className="group col-span-full -mx-2.5 grid min-h-7.5 grid-cols-subgrid items-center rounded-md px-2.5 py-1 hover:bg-hover"
            >
              <span className="flex min-w-0 items-center gap-2">
                <NewDot point={row} />
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
              <span className="flex min-w-0 items-center gap-2">
                <span className="line-clamp-2 min-w-0 flex-1 text-xs text-faint">
                  {rest(fields, row)}
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
            <p className={`min-w-0 flex-1 ${done ? "text-faint" : ""}`}>
              <RowName row={row} />
              {words && <span className="text-muted"> · {words}</span>}
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

/** A list: a row a line, its name, then its other cells. */
function RowList({ rows, fields }: { rows: Point[]; fields: Field[] }) {
  return (
    <ul className={`flex flex-col ${LINES}`}>
      {rows.map((row) => {
        const words = rest(fields, row);
        return (
          <li key={row.id} className="group flex items-baseline gap-2 rounded-md px-2.5 py-1.5">
            <NewDot point={row} className="self-center" />
            <p className="min-w-0 flex-1">
              {/* A name alone is the line; beside its cells it leads. */}
              <RowName row={row} className={fields.length > 0 ? "font-semibold" : ""} />
              {words && <span className="text-muted"> · {words}</span>}
            </p>
            <Pills fields={fields} row={row} />
            <RemovePoint point={row} />
          </li>
        );
      })}
    </ul>
  );
}
