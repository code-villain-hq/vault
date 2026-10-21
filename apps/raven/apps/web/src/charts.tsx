import { type ReactNode, useEffect, useRef, useState } from "react";
import { clock, ms } from "./format";
import type { Point } from "./types";

const HEIGHT = 160;
const PAD = { top: 12, right: 40, bottom: 22, left: 52 };

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

/** Round numbers for the axis: 0 and up to three steps above. */
function ticks(max: number): number[] {
  if (max <= 0) return [0, 1];
  const raw = max / 3;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * power).find((s) => s >= raw) ?? raw;
  const top = Math.ceil(max / step) * step;
  const out: number[] = [];
  for (let v = 0; v <= top + step / 2; v += step) out.push(v);
  return out;
}

type Frame = {
  width: number;
  x: (i: number) => number;
  y: (v: number) => number;
  top: number;
  slot: number;
};

function frame(width: number, count: number, max: number): Frame {
  const inner = Math.max(1, width - PAD.left - PAD.right);
  const slot = inner / Math.max(1, count);
  return {
    width,
    x: (i) => PAD.left + slot * (i + 0.5),
    y: (v) => PAD.top + (HEIGHT - PAD.top - PAD.bottom) * (1 - v / max),
    top: max,
    slot,
  };
}

/** The recessive grid and the time labels shared by both charts. */
function Axes({ f, points, label }: { f: Frame; points: Point[]; label: (v: number) => string }) {
  const marks = ticks(f.top);
  const every = Math.max(1, Math.ceil(points.length / 6));
  return (
    <g className="text-xs">
      {marks.map((v) => (
        <g key={v}>
          <line
            x1={PAD.left}
            x2={f.width - PAD.right}
            y1={f.y(v)}
            y2={f.y(v)}
            className={v === 0 ? "stroke-line-strong" : "stroke-line"}
          />
          <text x={PAD.left - 8} y={f.y(v)} dy="0.32em" textAnchor="end" className="fill-faint">
            {label(v)}
          </text>
        </g>
      ))}
      {points.map((p, i) =>
        i % every === 0 ? (
          <text
            key={p.at}
            x={f.x(i)}
            y={HEIGHT - 4}
            textAnchor="middle"
            className="fill-faint tabular-nums"
          >
            {clock(p.at)}
          </text>
        ) : null,
      )}
    </g>
  );
}

/** The hovered bucket's figures, beside the pointer. */
function Tooltip({ f, index, children }: { f: Frame; index: number; children: ReactNode }) {
  const left = f.x(index);
  const flip = left > f.width / 2;
  return (
    <div
      className="ui-menu pointer-events-none absolute top-2 z-10 flex min-w-36 flex-col gap-0.5 px-3 py-2 text-xs"
      style={flip ? { right: f.width - left + 12 } : { left: left + 12 }}
    >
      {children}
    </div>
  );
}

function useHover(f: Frame | null, count: number) {
  const [index, setIndex] = useState<number | null>(null);
  const onMove = (event: React.MouseEvent<SVGSVGElement>) => {
    if (!f) return;
    const box = event.currentTarget.getBoundingClientRect();
    const i = Math.floor((event.clientX - box.left - PAD.left) / f.slot);
    setIndex(i >= 0 && i < count ? i : null);
  };
  return { index, onMove, onLeave: () => setIndex(null) };
}

/**
 * How long calls took, bucket by bucket: the slow end (p95) as the line that
 * matters, the typical call (p50) quieter and dashed beneath it.
 */
export function DurationChart({ points }: { points: Point[] }) {
  const { ref, width } = useWidth();
  const timed = points.filter((p) => p.calls > 0);
  const max = Math.max(1, ...timed.map((p) => p.p95_ms)) * 1.1;
  const f = width ? frame(width, points.length, ticks(max).at(-1) ?? max) : null;
  const hover = useHover(f, points.length);
  // A bucket without calls breaks the line rather than drawing it down to zero.
  const line = (key: "p50_ms" | "p95_ms") => {
    if (!f) return "";
    let path = "";
    let drawing = false;
    points.forEach((p, i) => {
      if (p.calls === 0) {
        drawing = false;
        return;
      }
      path += `${drawing ? "L" : "M"}${f.x(i)},${f.y(p[key])}`;
      if (!drawing) path += "h0.01";
      drawing = true;
    });
    return path;
  };
  const lastIndex = points.findLastIndex((p) => p.calls > 0);
  const last = points[lastIndex];
  const hovered = hover.index != null ? points[hover.index] : null;
  return (
    <figure className="flex flex-col gap-2">
      <figcaption className="flex items-center gap-4 text-xs text-muted">
        <span className="flex items-center gap-1.5">
          <span aria-hidden="true" className="h-0.5 w-4 rounded-full bg-fg" />
          Slow end (p95)
        </span>
        <span className="flex items-center gap-1.5">
          <span aria-hidden="true" className="h-0 w-4 border-t-2 border-dashed border-muted" />
          Typical (p50)
        </span>
      </figcaption>
      <div ref={ref} className="relative h-40">
        {f && (
          <svg
            width={f.width}
            height={HEIGHT}
            onMouseMove={hover.onMove}
            onMouseLeave={hover.onLeave}
            aria-hidden="true"
          >
            <Axes f={f} points={points} label={(v) => (v === 0 ? "0" : ms(v))} />
            {hover.index != null && (
              <line
                x1={f.x(hover.index)}
                x2={f.x(hover.index)}
                y1={PAD.top}
                y2={HEIGHT - PAD.bottom}
                className="stroke-line-strong"
              />
            )}
            <path
              d={line("p50_ms")}
              fill="none"
              strokeWidth={2}
              strokeDasharray="4 4"
              strokeLinecap="round"
              className="stroke-muted"
            />
            <path
              d={line("p95_ms")}
              fill="none"
              strokeWidth={2}
              strokeLinejoin="round"
              strokeLinecap="round"
              className="stroke-fg"
            />
            {hovered && hovered.calls > 0 && hover.index != null && (
              <>
                <circle
                  cx={f.x(hover.index)}
                  cy={f.y(hovered.p50_ms)}
                  r={4}
                  strokeWidth={2}
                  className="fill-muted stroke-panel"
                />
                <circle
                  cx={f.x(hover.index)}
                  cy={f.y(hovered.p95_ms)}
                  r={4}
                  strokeWidth={2}
                  className="fill-fg stroke-panel"
                />
              </>
            )}
            {last && (
              <g className="text-xs">
                <text x={f.x(lastIndex) + 8} y={f.y(last.p95_ms)} dy="0.32em" className="fill-fg">
                  p95
                </text>
                <text
                  x={f.x(lastIndex) + 8}
                  y={f.y(last.p50_ms)}
                  dy={Math.abs(f.y(last.p50_ms) - f.y(last.p95_ms)) < 12 ? "1.2em" : "0.32em"}
                  className="fill-muted"
                >
                  p50
                </text>
              </g>
            )}
          </svg>
        )}
        {f && hovered && hover.index != null && (
          <Tooltip f={f} index={hover.index}>
            <span className="text-muted">{clock(hovered.at)}</span>
            {hovered.calls > 0 ? (
              <>
                <span className="flex justify-between gap-4">
                  Slow end <span className="tabular-nums">{ms(hovered.p95_ms)}</span>
                </span>
                <span className="flex justify-between gap-4 text-muted">
                  Typical <span className="tabular-nums">{ms(hovered.p50_ms)}</span>
                </span>
              </>
            ) : (
              <span className="text-muted">No calls</span>
            )}
          </Tooltip>
        )}
      </div>
    </figure>
  );
}

/** Calls per bucket; the failed part of each bar in danger, the rest quiet. */
export function CallsChart({ points }: { points: Point[] }) {
  const { ref, width } = useWidth();
  const max = Math.max(1, ...points.map((p) => p.calls));
  const f = width ? frame(width, points.length, ticks(max).at(-1) ?? max) : null;
  const hover = useHover(f, points.length);
  const hovered = hover.index != null ? points[hover.index] : null;
  const failed = points.some((p) => p.errors > 0);
  return (
    <figure className="flex flex-col gap-2">
      <figcaption className="flex items-center gap-4 text-xs text-muted">
        <span className="flex items-center gap-1.5">
          <span aria-hidden="true" className="size-2.5 rounded-sm bg-fg/25" />
          Calls
        </span>
        {failed && (
          <span className="flex items-center gap-1.5">
            <span aria-hidden="true" className="size-2.5 rounded-sm bg-danger" />
            Failed
          </span>
        )}
      </figcaption>
      <div ref={ref} className="relative h-40">
        {f && (
          <svg
            width={f.width}
            height={HEIGHT}
            onMouseMove={hover.onMove}
            onMouseLeave={hover.onLeave}
            aria-hidden="true"
          >
            <Axes f={f} points={points} label={(v) => String(v)} />
            {points.map((p, i) => {
              if (p.calls === 0) return null;
              // A 2px gap between bars, and between the failed part and the rest.
              const w = Math.max(1, f.slot - 2);
              const x = f.x(i) - w / 2;
              const base = f.y(0);
              const okTop = f.y(p.calls - p.errors);
              const top = f.y(p.calls);
              const r = Math.min(2, w / 2);
              const dim = hover.index != null && hover.index !== i ? "opacity-60" : "";
              return (
                <g key={p.at} className={dim}>
                  {p.calls > p.errors && (
                    <rect
                      x={x}
                      y={okTop}
                      width={w}
                      height={Math.max(1, base - okTop)}
                      rx={p.errors ? 0 : r}
                      className="fill-fg/25"
                    />
                  )}
                  {p.errors > 0 && (
                    <rect
                      x={x}
                      y={top}
                      width={w}
                      height={Math.max(1, okTop - top - (p.calls > p.errors ? 2 : 0))}
                      rx={r}
                      className="fill-danger"
                    />
                  )}
                </g>
              );
            })}
          </svg>
        )}
        {f && hovered && hover.index != null && (
          <Tooltip f={f} index={hover.index}>
            <span className="text-muted">{clock(hovered.at)}</span>
            <span className="flex justify-between gap-4">
              Calls <span className="tabular-nums">{hovered.calls}</span>
            </span>
            {hovered.errors > 0 && (
              <span className="flex justify-between gap-4 text-danger-ink">
                Failed <span className="tabular-nums">{hovered.errors}</span>
              </span>
            )}
          </Tooltip>
        )}
      </div>
    </figure>
  );
}
