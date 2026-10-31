// Trend and Parts: a number kept each time it's measured, and a whole split
// into its parts. They draw as the studio's charts do (Raven's charts.tsx):
// one axis, a 2px line in the ink, a hairline grid, a tooltip on hover,
// and the parts in steps of the ink, so there are no colours to learn.

import type { Point } from "@almanac/types";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { amountWords, dateWords, fromKey, today } from "./format";

const HEIGHT = 140;
const PAD = { top: 12, right: 8, bottom: 22, left: 36 };
const DAY = /^\d{4}-\d{2}-\d{2}$/;

/** The chart's width in pixels, following its box. */
function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const box = ref.current;
    if (!box) return;
    const observer = new ResizeObserver(([entry]) => entry && setWidth(entry.contentRect.width));
    observer.observe(box);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/** Round numbers from below `lo` to above `hi`, about three steps apart: 3.2, 3.4, 3.6. */
function ticks(lo: number, hi: number): number[] {
  if (hi - lo < 1e-9) {
    const pad = Math.abs(lo) * 0.1 || 1;
    return ticks(lo - pad, hi + pad);
  }
  const raw = (hi - lo) / 3;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * power).find((s) => s >= raw) ?? raw;
  const first = Math.floor(lo / step);
  const last = Math.ceil(hi / step);
  return Array.from({ length: last - first + 1 }, (_, i) =>
    Number(((first + i) * step).toPrecision(12)),
  );
}

/** A day as the chart says it: "Sep 24", with its year when it isn't this one's. */
function dayWords(key: string): string {
  return key.slice(0, 4) === today().slice(0, 4)
    ? dateWords(key)
    : `${dateWords(key)}, ${key.slice(0, 4)}`;
}

/** Where the time axis is labelled: the first and last day, or each month's or year's start. */
function timeMarks(first: string, last: string): { at: number; label: string }[] {
  const [from, to] = [fromKey(first), fromKey(last)];
  const days = (to.getTime() - from.getTime()) / 86_400_000;
  if (days <= 45) {
    return first === last
      ? [{ at: from.getTime(), label: dayWords(first) }]
      : [
          { at: from.getTime(), label: dateWords(first) },
          { at: to.getTime(), label: dateWords(last) },
        ];
  }
  const marks: { at: number; label: string }[] = [];
  const byYear = days > 548;
  const at = new Date(from.getFullYear(), byYear ? 0 : from.getMonth(), 1);
  while (at <= to) {
    if (at >= from) {
      marks.push({
        at: at.getTime(),
        label: byYear ? String(at.getFullYear()) : at.toLocaleDateString("en", { month: "short" }),
      });
    }
    if (byYear) at.setFullYear(at.getFullYear() + 1);
    else at.setMonth(at.getMonth() + 1);
  }
  const every = Math.ceil(marks.length / 5);
  return marks.filter((_, i) => i % every === 0);
}

/** A trend's measurements, oldest first, and the mark it aims at: its point with no day. */
export function measured(points: Point[]): { measurements: Point[]; goal?: Point } {
  const measurements = points
    .filter((p) => p.amount !== null && DAY.test(p.happened))
    .sort((a, b) => a.happened.localeCompare(b.happened) || a.position - b.position);
  const goal = points.find((p) => p.amount !== null && !p.happened);
  return { measurements, goal };
}

/** The width a line's name takes at its end, past the last day. */
const NAMES = 88;

/**
 * Where each line's name goes at its end: beside its last point, nudged
 * apart so two never overlap.
 */
function nameMarks(ends: { name: string; y: number }[]): { name: string; y: number }[] {
  const marks = [...ends].sort((a, b) => a.y - b.y);
  marks.forEach((mark, i) => {
    const above = marks[i - 1];
    if (above && mark.y - above.y < 13) mark.y = above.y + 13;
  });
  return marks;
}

/**
 * A trend: what it measures, the latest as a number to read at a glance,
 * how it moved since the first, and a line through the measurements by
 * day, on one scale with the mark it aims at, dashed. When a day has
 * several measurements, each name is a line of its own, named at its end
 * (a shop's price, day by day), and there's no one latest to lead with.
 */
export function TrendSet({
  name,
  points,
  heading,
  below,
  listed,
}: {
  name: string;
  points: Point[];
  /** Above it, naming what's measured. */
  heading?: ReactNode;
  /** Under it, such as the reminder to measure again. */
  below?: ReactNode;
  /** On a section's own page: each measurement, a line each, newest first. */
  listed?: (point: Point) => ReactNode;
}) {
  const { ref, width } = useWidth();
  const [hovered, setHovered] = useState<number | null>(null);
  const { measurements, goal } = measured(points);
  const first = measurements[0];
  const latest = measurements.at(-1);
  if (!first || !latest) return null;
  const unit = latest.note || first.note;
  const words = (amount: number) =>
    unit ? `${amountWords(amount, unit)} ${unit}` : amountWords(amount);
  const change = (latest.amount ?? 0) - (first.amount ?? 0);
  const lines = new Map<string, Point[]>();
  if (new Set(measurements.map((p) => p.happened)).size < measurements.length) {
    for (const p of measurements) lines.set(p.text, [...(lines.get(p.text) ?? []), p]);
  }
  const series = lines.size > 0;
  const right = series ? NAMES : PAD.right;

  const amounts = measurements.map((p) => p.amount ?? 0);
  if (goal?.amount != null) amounts.push(goal.amount);
  const marks = ticks(Math.min(...amounts), Math.max(...amounts));
  const [lo = 0, hi = 1] = [marks[0], marks.at(-1)];
  // Two lines are enough to read it by: the lowest and the highest.
  const grid = marks.length > 3 ? [lo, hi] : marks;
  const [t0, t1] = [fromKey(first.happened).getTime(), fromKey(latest.happened).getTime()];
  const inner = Math.max(1, width - PAD.left - right);
  const x = (t: number) =>
    t1 === t0 ? PAD.left + inner / 2 : PAD.left + (inner * (t - t0)) / (t1 - t0);
  const y = (v: number) => PAD.top + (HEIGHT - PAD.top - PAD.bottom) * (1 - (v - lo) / (hi - lo));
  const at = (p: Point) => x(fromKey(p.happened).getTime());
  const path = (points: Point[]) =>
    points.map((p, i) => `${i ? "L" : "M"}${at(p)},${y(p.amount ?? 0)}`).join(" ");
  const shown = hovered != null ? measurements[hovered] : undefined;
  // A line each, in steps of the ink; the one hovered in the ink itself.
  const inks = ["stroke-fg fill-fg", "stroke-muted fill-muted", "stroke-faint fill-faint"];
  const drawn = series
    ? [...lines].map(([name, points], i) => ({
        name,
        points,
        ink: shown
          ? shown.text === name
            ? "stroke-fg fill-fg"
            : "stroke-line-strong fill-line-strong"
          : (inks[i % 3] ?? ""),
      }))
    : [{ name, points: measurements, ink: "stroke-fg fill-fg" }];
  const onMove = (event: React.MouseEvent<SVGSVGElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const [left, top] = [event.clientX - box.left, event.clientY - box.top];
    // The nearest by day, and among a day's by height.
    const away = (p: Point) => Math.abs(at(p) - left) * 4 + Math.abs(y(p.amount ?? 0) - top);
    let nearest = 0;
    measurements.forEach((p, i) => {
      if (away(p) < away(measurements[nearest] ?? p)) nearest = i;
    });
    setHovered(nearest);
  };

  return (
    <section className="flex flex-col gap-1" aria-label={name}>
      {heading && <h3 className="text-xs text-muted">{heading}</h3>}
      <p className={series ? "sr-only" : "flex flex-wrap items-baseline gap-x-2.5"}>
        <span className="font-semibold text-wordmark tabular-nums">
          {words(latest.amount ?? 0)}
        </span>
        {measurements.length > 1 && (
          <span className="text-muted text-xs">
            {change === 0
              ? `the same since ${dayWords(first.happened)}`
              : `${change < 0 ? "−" : "+"}${words(Math.abs(change))} since ${dayWords(first.happened)}`}
          </span>
        )}
      </p>
      <div ref={ref} className="relative mt-1.5 h-[140px]">
        {width > 0 && (
          <svg
            width={width}
            height={HEIGHT}
            onMouseMove={onMove}
            onMouseLeave={() => setHovered(null)}
            aria-hidden="true"
            className="overflow-visible"
          >
            <g className="text-xs">
              {grid.map((v) => (
                <g key={v}>
                  <line
                    x1={PAD.left}
                    x2={width - right}
                    y1={y(v)}
                    y2={y(v)}
                    className="stroke-line"
                  />
                  <text
                    x={PAD.left - 8}
                    y={y(v)}
                    dy="0.32em"
                    textAnchor="end"
                    className="fill-faint tabular-nums"
                  >
                    {amountWords(v)}
                  </text>
                </g>
              ))}
              {timeMarks(first.happened, latest.happened).map((mark) => {
                const left = x(mark.at);
                const anchor =
                  left < PAD.left + 16 ? "start" : left > width - right - 16 ? "end" : "middle";
                return (
                  <text
                    key={mark.at}
                    x={left}
                    y={HEIGHT - 4}
                    textAnchor={anchor}
                    className="fill-faint"
                  >
                    {mark.label}
                  </text>
                );
              })}
              {goal?.amount != null && (
                <>
                  <line
                    x1={PAD.left}
                    x2={width - right}
                    y1={y(goal.amount)}
                    y2={y(goal.amount)}
                    strokeWidth={1.5}
                    strokeDasharray="4 4"
                    className="stroke-muted"
                  />
                  <text x={PAD.left + 6} y={y(goal.amount) - 5} className="fill-faint">
                    {goal.text || "Goal"} {amountWords(goal.amount)}
                  </text>
                </>
              )}
            </g>
            {shown && (
              <line
                x1={at(shown)}
                x2={at(shown)}
                y1={PAD.top}
                y2={HEIGHT - PAD.bottom}
                className="stroke-line-strong"
              />
            )}
            {drawn.map((line) => (
              <g key={line.name}>
                <path
                  d={path(line.points)}
                  fill="none"
                  strokeWidth={2}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                  className={line.ink.split(" ")[0]}
                />
                {line.points.map((p) => (
                  <circle
                    key={p.id}
                    cx={at(p)}
                    cy={y(p.amount ?? 0)}
                    r={p === shown ? 5 : 4}
                    strokeWidth={2}
                    className={`${line.ink.split(" ")[1]} stroke-panel`}
                  />
                ))}
              </g>
            ))}
            {series &&
              nameMarks(
                drawn.flatMap((line) => {
                  const last = line.points.at(-1);
                  return last ? [{ name: line.name, y: y(last.amount ?? 0) }] : [];
                }),
              ).map((mark) => (
                <text
                  key={mark.name}
                  x={width - right + 10}
                  y={mark.y}
                  dy="0.32em"
                  className={`text-xs ${shown && shown.text !== mark.name ? "fill-faint" : "fill-muted"}`}
                >
                  {mark.name.length > 12 ? `${mark.name.slice(0, 11)}…` : mark.name}
                </text>
              ))}
          </svg>
        )}
        {shown && (
          <div
            className="ui-menu pointer-events-none absolute -top-1.5 z-10 flex flex-col px-3 py-2 text-xs"
            style={
              at(shown) > width / 2 ? { right: width - at(shown) + 12 } : { left: at(shown) + 12 }
            }
          >
            <span className="font-semibold tabular-nums">{words(shown.amount ?? 0)}</span>
            <span className="text-muted">{dayWords(shown.happened)}</span>
            {shown.text && <span className="max-w-56 text-muted">{shown.text}</span>}
          </div>
        )}
      </div>
      <ul className={listed ? "mt-2 flex flex-col gap-0.5" : "sr-only"}>
        {[...measurements].reverse().map((p) => (
          <li
            key={p.id}
            className="group -mx-2.5 flex min-h-7.5 items-center gap-3 rounded-md px-2.5 py-1 hover:bg-hover"
          >
            <span className="w-24 shrink-0 text-muted text-xs">{dayWords(p.happened)}</span>
            <span className="shrink-0 font-semibold tabular-nums">{words(p.amount ?? 0)}</span>
            <span className="min-w-0 flex-1 truncate text-faint text-xs">{p.text}</span>
            {listed?.(p)}
          </li>
        ))}
      </ul>
      {below}
    </section>
  );
}
