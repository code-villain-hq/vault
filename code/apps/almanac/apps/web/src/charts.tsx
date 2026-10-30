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

/**
 * A trend: what it measures, the latest as a number to read at a glance,
 * how it moved since the first, and a line through the measurements by
 * day, on one scale with the mark it aims at, dashed.
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
  const words = (amount: number) => (unit ? `${amountWords(amount)} ${unit}` : amountWords(amount));
  const change = (latest.amount ?? 0) - (first.amount ?? 0);

  const amounts = measurements.map((p) => p.amount ?? 0);
  if (goal?.amount != null) amounts.push(goal.amount);
  const marks = ticks(Math.min(...amounts), Math.max(...amounts));
  const [lo = 0, hi = 1] = [marks[0], marks.at(-1)];
  // Two lines are enough to read it by: the lowest and the highest.
  const grid = marks.length > 3 ? [lo, hi] : marks;
  const [t0, t1] = [fromKey(first.happened).getTime(), fromKey(latest.happened).getTime()];
  const inner = Math.max(1, width - PAD.left - PAD.right);
  const x = (t: number) =>
    t1 === t0 ? PAD.left + inner / 2 : PAD.left + (inner * (t - t0)) / (t1 - t0);
  const y = (v: number) => PAD.top + (HEIGHT - PAD.top - PAD.bottom) * (1 - (v - lo) / (hi - lo));
  const at = (p: Point) => x(fromKey(p.happened).getTime());
  const line = measurements.map((p, i) => `${i ? "L" : "M"}${at(p)},${y(p.amount ?? 0)}`).join(" ");
  const shown = hovered != null ? measurements[hovered] : undefined;
  const onMove = (event: React.MouseEvent<SVGSVGElement>) => {
    const left = event.clientX - event.currentTarget.getBoundingClientRect().left;
    let nearest = 0;
    measurements.forEach((p, i) => {
      if (Math.abs(at(p) - left) < Math.abs(at(measurements[nearest] ?? p) - left)) nearest = i;
    });
    setHovered(nearest);
  };

  return (
    <section className="flex flex-col gap-1" aria-label={name}>
      {heading && <h3 className="text-xs text-muted">{heading}</h3>}
      <p className="flex flex-wrap items-baseline gap-x-2.5">
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
                    x2={width - PAD.right}
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
                  left < PAD.left + 16 ? "start" : left > width - PAD.right - 16 ? "end" : "middle";
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
                    x2={width - PAD.right}
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
            <path
              d={line}
              fill="none"
              strokeWidth={2}
              strokeLinejoin="round"
              strokeLinecap="round"
              className="stroke-fg"
            />
            {measurements.map((p) => (
              <circle
                key={p.id}
                cx={at(p)}
                cy={y(p.amount ?? 0)}
                r={p === shown ? 5 : 4}
                strokeWidth={2}
                className="fill-fg stroke-panel"
              />
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

/** The steps of the ink a whole's parts take, the largest darkest. */
const INK = [78, 56, 38, 24, 14];

interface Part {
  label: string;
  amount: number;
  /** None for Other, the parts past the fifth folded together. */
  point?: Point;
}

/** A whole's parts, largest first: five at most, the rest folded into Other. */
export function partsOf(points: Point[]): Part[] {
  const parts = points
    .filter((p) => p.amount !== null && p.amount > 0)
    .map((p) => ({ label: p.text, amount: p.amount ?? 0, point: p }))
    .sort((a, b) => b.amount - a.amount);
  if (parts.length <= INK.length) return parts;
  const rest = parts.slice(INK.length - 1).reduce((sum, p) => sum + p.amount, 0);
  return [...parts.slice(0, INK.length - 1), { label: "Other", amount: rest }];
}

/**
 * A whole and its parts: a ring with the total in its middle, and beside it
 * a legend with each part's count and share, largest first. Pointing at a
 * part, in the ring or the legend, shows it in the middle and quiets the rest.
 */
export function PartsSet({
  name,
  points,
  heading,
  remove,
}: {
  name: string;
  points: Point[];
  heading?: ReactNode;
  /** On a section's own page: a way to take a part off. */
  remove?: (point: Point) => ReactNode;
}) {
  const [active, setActive] = useState<number | null>(null);
  const parts = partsOf(points);
  const total = parts.reduce((sum, p) => sum + p.amount, 0);
  if (total <= 0) return null;
  // What they count, as the first point's note says it: "ads".
  const counts = points[0]?.note ?? "";
  const R = 55;
  const around = 2 * Math.PI * R;
  // A 2px gap of the panel between parts, none when there's one.
  const gap = parts.length > 1 ? 2 : 0;
  let start = 0;
  const arcs = parts.map((part) => {
    const length = (part.amount / total) * around;
    const arc = { dash: Math.max(0.5, length - gap), offset: -(start + gap / 2) };
    start += length;
    return arc;
  });
  const share = (amount: number) => `${Math.round((amount / total) * 100)} %`;
  const middle = active != null ? parts[active] : undefined;
  const ink = (i: number) => `color-mix(in srgb, var(--fg) ${INK[i] ?? 14}%, transparent)`;

  return (
    <section className="flex flex-col gap-2" aria-label={name}>
      {heading && <h3 className="text-xs text-muted">{heading}</h3>}
      <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
        <div className="relative size-32 shrink-0">
          <svg
            width={128}
            height={128}
            viewBox="0 0 128 128"
            aria-hidden="true"
            onMouseMove={(event) => {
              const part = (event.target as Element).getAttribute("data-part");
              setActive(part === null ? null : Number(part));
            }}
            onMouseLeave={() => setActive(null)}
          >
            {parts.map((part, i) => (
              <circle
                key={part.label}
                cx={64}
                cy={64}
                r={R}
                fill="none"
                strokeWidth={18}
                strokeDasharray={`${arcs[i]?.dash} ${around}`}
                strokeDashoffset={arcs[i]?.offset}
                transform="rotate(-90 64 64)"
                style={{ stroke: ink(i) }}
                data-part={i}
                className={`transition-opacity ${active != null && active !== i ? "opacity-40" : ""}`}
              />
            ))}
          </svg>
          <div className="pointer-events-none absolute inset-0 grid place-content-center px-6 text-center">
            <b className="font-semibold text-wordmark leading-none tabular-nums">
              {amountWords(middle?.amount ?? total)}
            </b>
            <span className="line-clamp-2 text-faint text-xs">
              {middle ? middle.label : counts}
            </span>
          </div>
        </div>
        <ul className="flex min-w-44 flex-1 flex-col gap-0.5" onMouseLeave={() => setActive(null)}>
          {parts.map((part, i) => (
            <li
              key={part.label}
              onMouseEnter={() => setActive(i)}
              className={`group -mx-2 grid min-h-7 items-center gap-2.5 rounded-md px-2 py-1 ${remove ? "grid-cols-[0.625rem_minmax(0,1fr)_auto_3.2em_auto]" : "grid-cols-[0.625rem_minmax(0,1fr)_auto_3.2em]"} ${active === i ? "bg-hover" : ""}`}
            >
              <span
                aria-hidden="true"
                className="size-2.5 rounded-sm"
                style={{ background: ink(i) }}
              />
              <span className="min-w-0 break-words">{part.label}</span>
              <span className="text-right font-semibold tabular-nums">
                {amountWords(part.amount)}
              </span>
              <span className="text-right text-faint text-xs tabular-nums">
                {share(part.amount)}
              </span>
              {remove && (
                <span className="flex w-6 justify-end">{part.point && remove(part.point)}</span>
              )}
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
