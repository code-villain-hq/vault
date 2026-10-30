// The one set of blocks every page is made of: paragraphs, bullets, a
// checklist, toggles, a callout, and a collection drawn as the picture its
// fields pick (`collection.ts`): a line, a board, cards, a table, a
// timeline, a week, bars or a list. The same on every topic; nobody
// designs a page's own. Also the parts the page's older kinds share with
// them: a photo, a tick, a timeline's rail, ✕ to take a point off.

import type { Block, Field, Point } from "@almanac/types";
import { type Confirmation, ConfirmDialog, RowAction } from "@code-villain/web";
import { ChevronRight, Circle, CircleCheck, Info, TriangleAlert, X } from "lucide-react";
import { type ReactNode, useState } from "react";
import { TrendSet } from "./charts";
import { byDay, cellOf, cellWords, dayOf, filled, lanesOf, numberOf } from "./collection";
import { addDays, amountWords, dateWords, today, weekdayWords } from "./format";
import { usePhoto } from "./photos";
import { inline } from "./prose";
import { when } from "./timeline";
import { useIsGuest, useWorkspace } from "./workspace";

/** A line's measure, for lines that read across. */
export const LINES = "max-w-[86ch]";

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

/**
 * A box to tick: done once all of `points` are. A list's line has one
 * point; a question, the points of its answer. Guests see it, not tick it.
 */
export function Tick({
  points,
  label,
  className = "",
}: {
  points: Point[];
  label: string;
  className?: string;
}) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const done = points.length > 0 && points.every((p) => p.done);
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
      aria-label={`${label}: ${done ? "done" : "to do"}`}
      onClick={() =>
        sync.transact(
          points.map((p) => ({
            action: "update" as const,
            model: "Point" as const,
            id: p.id,
            data: { done: !done },
          })),
        )
      }
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

/** Bullets: a point a line, in their order. */
export function Bullets({ points }: { points: Point[] }) {
  return (
    <div className={`flex flex-col gap-1.5 ${LINES}`}>
      <ul className="flex flex-col gap-0.5">
        {points.map((point) => (
          <li
            key={point.id}
            className="group relative flex items-baseline gap-2 py-0.5 pl-3.5 before:absolute before:left-0.5 before:text-faint before:content-['·']"
          >
            <NewDot point={point} className="self-center" />
            <span className="min-w-0 flex-1">{inline(point.text)}</span>
            <RemovePoint point={point} />
          </li>
        ))}
      </ul>
      <Sources points={points} />
    </div>
  );
}

/** A checklist: a point a line to tick, in their order; what's done goes faint, not away. */
export function Checklist({ points }: { points: Point[] }) {
  return (
    <ul className={`flex flex-col ${LINES}`}>
      {points.map((point) => (
        <li key={point.id} className="group flex items-baseline gap-3 rounded-md px-2.5 py-1.5">
          <Tick points={[point]} label={point.text} className="self-center" />
          <NewDot point={point} className="self-center" />
          <p className={`min-w-0 flex-1 ${point.done ? "text-faint" : ""}`}>{inline(point.text)}</p>
          <RemovePoint point={point} />
        </li>
      ))}
    </ul>
  );
}

/**
 * Toggles: a line per theme that opens on its points, as bullets, closed
 * until you open it. Points under no theme show as bullets after them.
 */
export function Toggles({ points }: { points: Point[] }) {
  const themes = [...new Set(points.map((p) => p.theme).filter(Boolean))];
  const loose = points.filter((p) => !p.theme);
  return (
    <div className={`flex flex-col gap-1 ${LINES}`}>
      {themes.map((theme) => (
        <Toggle key={theme} name={theme} points={points.filter((p) => p.theme === theme)} />
      ))}
      {loose.length > 0 && <Bullets points={loose} />}
    </div>
  );
}

function Toggle({ name, points }: { name: string; points: Point[] }) {
  const [open, setOpen] = useState(false);
  const fresh = points.some((p) => !p.seen);
  return (
    <section className="flex flex-col">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="group/toggle flex items-center gap-1.5 rounded-md py-1 text-left hover:text-fg"
      >
        <ChevronRight
          size={15}
          strokeWidth={1.75}
          aria-hidden="true"
          className={`shrink-0 text-faint transition-[rotate] group-hover/toggle:text-muted ${open ? "rotate-90" : ""}`}
        />
        <span className="font-medium">{name}</span>
        {fresh && <span role="img" aria-label="New" className="size-1.5 rounded-full bg-accent" />}
        {!open && <span className="text-xs text-faint">{points.length}</span>}
      </button>
      {open && (
        <div className="pt-1 pb-2 pl-5">
          <Bullets points={points} />
        </div>
      )}
    </section>
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
export function Collection({ block, rows }: { block: Block; rows: Point[] }) {
  const fields = filled(block, rows);
  const of = (...kinds: string[]) => fields.find((f) => kinds.includes(f.kind));
  const day = of("day");
  const number = of("number");
  // What's measured, over the picture, unless the block's title says it already.
  const heading = number && number.name !== block.title ? number.name : undefined;
  switch (block.view) {
    case "line":
      return day && number ? (
        <Line rows={rows} day={day} number={number} heading={heading} />
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
    case "cards":
      return <RowCards rows={rows} fields={fields} />;
    case "table":
      return <RowTable block={block} rows={rows} fields={fields} />;
    default:
      return <RowList rows={rows} fields={fields} />;
  }
}

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
        const dot =
          i === 0
            ? "ring-[1.5px] ring-faint ring-inset"
            : i === lanes.length - 1
              ? "bg-ok"
              : "bg-info";
        return (
          <section key={lane} aria-label={lane} className="flex min-w-0 flex-col gap-1.5">
            <h3 className="flex items-center gap-2 text-xs text-muted">
              {steps && <span aria-hidden="true" className={`size-2 rounded-full ${dot}`} />}
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
                    <span className="truncate">{row.text}</span>
                    {words && <span className="truncate text-xs text-faint">{words}</span>}
                  </div>
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
              <h3 className="min-w-0 flex-1 truncate font-semibold">{row.text}</h3>
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
                      {field.kind === "check" ? "✓" : cellWords(field, cell)}
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
              <td className={`${cell} max-w-72 truncate font-semibold`}>{row.text}</td>
              {columns.map((field) => {
                const value = cellOf(row, field);
                const words =
                  field.kind === "check" ? (value === true ? "✓" : "") : cellWords(field, value);
                return (
                  <td
                    key={field.name}
                    className={`${cell} max-w-64 truncate ${field.kind === "number" ? "text-right" : ""}`}
                  >
                    {words || <span className="text-faint">–</span>}
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
                {inline(row.text)}
                {words && <span className="text-faint"> · {words}</span>}
              </p>
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
                  {row.text}
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

/** Bars: a bar per row with a number, on the largest, its other cells beside it. */
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
  const most = Math.max(...bars.map((row) => Math.abs(numberOf(row, number) ?? 0)), 0);
  return (
    <section className="flex flex-col gap-1.5" aria-label={number.name}>
      {heading && <h3 className="text-xs text-muted">{heading}</h3>}
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
                <span className="line-clamp-2 min-w-0">{row.text}</span>
              </span>
              <span aria-hidden="true" className="block h-2.5 rounded-full bg-chip">
                <span
                  className="block h-full min-w-[3px] rounded-full bg-fg/40"
                  style={{ width: `${most ? (Math.abs(amount) / most) * 100 : 0}%` }}
                />
              </span>
              <span className="text-right font-semibold tabular-nums">
                {number.unit ? `${amountWords(amount)} ${number.unit}` : amountWords(amount)}
              </span>
              <span className="flex min-w-0 items-center gap-2">
                <span className="line-clamp-2 min-w-0 flex-1 text-xs text-faint">
                  {rest(fields, row)}
                </span>
                <RemovePoint point={row} />
              </span>
            </li>
          );
        })}
      </ul>
    </section>
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
              <span className="font-semibold">{row.text}</span>
              {words && <span className="text-muted"> · {words}</span>}
            </p>
            <RemovePoint point={row} />
          </li>
        );
      })}
    </ul>
  );
}
