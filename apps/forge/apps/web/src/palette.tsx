// The colours people pick from for labels, statuses and services.
import { Popover } from "@base-ui/react/popover";
import { Check } from "lucide-react";
import { type PointerEvent as ReactPointerEvent, useRef, useState } from "react";
import { contrast } from "./contrast";

/** Eight hues at one OKLCH lightness (0.76), so none jumps out or disappears.
 * Teal, amber, red and blue are the brand's done, waiting, error and information
 * colours. Same order as the earlier palette, so a past pick maps by position. */
export const COLORS = [
  "#9fa9ff", // indigo
  "#5bc8a6", // teal
  "#e0a53b", // amber
  "#f2994a", // orange
  "#ff8b7c", // red
  "#d691f0", // violet
  "#7fb3fb", // blue
  "#b0b0bd", // grey
];

/** A row of swatches to pick one of the colours. */
export function ColorChoice({
  value,
  disabled = false,
  large = false,
  onChange,
}: {
  value: string;
  disabled?: boolean;
  /** Bigger swatches, for a picker that's about choosing a colour. */
  large?: boolean;
  onChange: (color: string) => void;
}) {
  return (
    <span className={`flex ${large ? "flex-1 justify-between" : "gap-1"}`}>
      {COLORS.map((color) => (
        <button
          key={color}
          type="button"
          aria-label={`Colour ${color}`}
          aria-pressed={color === value}
          disabled={disabled}
          onClick={() => onChange(color)}
          className={`${large ? "size-6" : "size-3.5"} rounded-full outline-offset-2 disabled:cursor-default ${color === value ? "outline-2 outline-fg" : ""}`}
          style={{ background: color }}
        />
      ))}
    </span>
  );
}

/** The swatch that stands for "any colour". */
export const RAINBOW =
  "conic-gradient(#f2994a, #e0a53b, #5bc8a6, #7fb3fb, #9fa9ff, #d691f0, #ff8b7c, #f2994a)";

type Hsv = { h: number; s: number; v: number };

function hexToHsv(hex: string): Hsv {
  const n = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => c / 255) as [
    number,
    number,
    number,
  ];
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  let h = 0;
  if (d) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
  }
  return { h: (h * 60 + 360) % 360, s: max ? d / max : 0, v: max };
}

function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return `#${[f(5), f(3), f(1)]
    .map((c) =>
      Math.round(c * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

const clamp = (n: number) => Math.min(1, Math.max(0, n));

/** Calls `onMove` with where the pointer is in the element, 0 to 1 each way, while dragging. */
function drag(
  event: ReactPointerEvent<HTMLElement>,
  onMove: (x: number, y: number) => void,
  onEnd: () => void,
) {
  const box = event.currentTarget.getBoundingClientRect();
  const move = (e: { clientX: number; clientY: number }) =>
    onMove(clamp((e.clientX - box.left) / box.width), clamp((e.clientY - box.top) / box.height));
  move(event);
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    onEnd();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/**
 * Any colour: a hex field, a square for saturation and brightness and a hue
 * slider. `onChange` gets the colour when a drag ends or a valid hex is typed;
 * the check goes back to the swatches.
 */
export function CustomColor({
  value,
  onChange,
  onDone,
}: {
  value: string;
  onChange: (color: string) => void;
  onDone: () => void;
}) {
  const [hsv, setHsv] = useState(() => hexToHsv(value));
  const [typed, setTyped] = useState<string | null>(null);
  const color = hsvToHex(hsv);
  const latest = useRef(color);
  latest.current = color;
  const commit = () => onChange(latest.current);
  const nudge = (change: Partial<Hsv>) => {
    const next = { ...hsv, ...change };
    setHsv(next);
    onChange(hsvToHex(next));
  };
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <button
          type="button"
          aria-label="Done"
          onClick={onDone}
          className="flex size-6 items-center justify-center rounded-full"
          // The swatch is the chosen colour; its tick takes whichever ink reads on it.
          style={{
            background: color,
            color: contrast(color, "#000000") >= contrast(color, "#ffffff") ? "#000000" : "#ffffff",
          }}
        >
          <Check size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
        <label className="flex flex-1 items-center gap-2 text-muted">
          <span className="text-xs">HEX</span>
          <input
            aria-label="Hex colour"
            value={typed ?? color}
            onChange={(e) => {
              const text = e.target.value.trim();
              setTyped(text);
              const hex = text.startsWith("#") ? text : `#${text}`;
              if (/^#[0-9a-fA-F]{6}$/.test(hex)) {
                setHsv(hexToHsv(hex.toLowerCase()));
                onChange(hex.toLowerCase());
              }
            }}
            onBlur={() => setTyped(null)}
            className="w-24 bg-transparent font-mono text-fg outline-none"
          />
        </label>
        <button
          type="button"
          aria-label="Back to the colours"
          onClick={onDone}
          className="size-6 rounded-full outline-2 outline-offset-2 outline-fg"
          style={{ background: RAINBOW }}
        />
      </div>
      <div className="flex gap-3">
        <div
          role="slider"
          tabIndex={0}
          aria-label="Saturation and brightness"
          aria-valuetext={`${Math.round(hsv.s * 100)}% saturation, ${Math.round(hsv.v * 100)}% brightness`}
          aria-valuenow={Math.round(hsv.s * 100)}
          onPointerDown={(event) =>
            drag(event, (x, y) => setHsv((c) => ({ ...c, s: x, v: 1 - y })), commit)
          }
          onKeyDown={(event) => {
            const step = 0.02;
            const moves: Record<string, Partial<Hsv>> = {
              ArrowLeft: { s: clamp(hsv.s - step) },
              ArrowRight: { s: clamp(hsv.s + step) },
              ArrowUp: { v: clamp(hsv.v + step) },
              ArrowDown: { v: clamp(hsv.v - step) },
            };
            const move = moves[event.key];
            if (move) {
              event.preventDefault();
              nudge(move);
            }
          }}
          className="relative h-28 flex-1 cursor-crosshair touch-none rounded-md outline-none focus-visible:ring-2 focus-visible:ring-accent"
          style={{
            background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${hsv.h} 100% 50%))`,
          }}
        >
          <span
            className="pointer-events-none absolute size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white"
            style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, background: color }}
          />
        </div>
        <div
          role="slider"
          tabIndex={0}
          aria-label="Hue"
          aria-orientation="vertical"
          aria-valuemin={0}
          aria-valuemax={360}
          aria-valuenow={Math.round(hsv.h)}
          onPointerDown={(event) =>
            drag(event, (_, y) => setHsv((c) => ({ ...c, h: y * 360 })), commit)
          }
          onKeyDown={(event) => {
            const delta = event.key === "ArrowDown" ? 5 : event.key === "ArrowUp" ? -5 : 0;
            if (delta) {
              event.preventDefault();
              nudge({ h: (hsv.h + delta + 360) % 360 });
            }
          }}
          className="relative w-3 cursor-pointer touch-none rounded-full outline-none focus-visible:ring-2 focus-visible:ring-accent"
          style={{
            background: "linear-gradient(to bottom, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)",
          }}
        >
          <span
            className="pointer-events-none absolute left-1/2 h-1.5 w-4 -translate-x-1/2 -translate-y-1/2 rounded-sm border-2 border-white"
            style={{ top: `${(hsv.h / 360) * 100}%` }}
          />
        </div>
      </div>
    </div>
  );
}

/**
 * A thing's colour as one dot; clicking it opens the palette. Lists show the
 * colour each thing has, not every colour it could have.
 */
export function ColorButton({
  value,
  label,
  disabled = false,
  onChange,
}: {
  value: string;
  label: string;
  disabled?: boolean;
  onChange: (color: string) => void;
}) {
  if (disabled) {
    return <span className="size-3.5 shrink-0 rounded-full" style={{ background: value }} />;
  }
  return (
    <Popover.Root>
      <Popover.Trigger
        aria-label={label}
        title={label}
        className="flex size-7 shrink-0 items-center justify-center rounded-md outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover"
      >
        <span className="size-3.5 rounded-full" style={{ background: value }} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner className="z-50 outline-none" sideOffset={4} align="end">
          <Popover.Popup className="app-menu w-80 rounded-lg bg-surface p-4 outline-none">
            <ColorChoice large value={value} onChange={onChange} />
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
