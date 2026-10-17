// Keeping colours people choose (statuses, labels, services, domains) readable: the
// nearest shade of the same colour with enough contrast (WCAG) on Forge's
// backgrounds, separately for light and dark mode.

import type { CSSProperties } from "react";
import colors from "./design/color.json";

/** Include selection as well as neutral surfaces when shading user-chosen colors. */
const grounds = (theme: typeof colors.night) => [
  theme.bg,
  theme.panel,
  theme.surface,
  theme.hover,
  theme.selected,
  theme.dialog,
  theme.field,
  theme["field-hover"],
  theme["dialog-field"],
  theme["dialog-field-hover"],
];
const LIGHT = grounds(colors.day);
const DARK = grounds(colors.night);

function channels(hex: string): number[] {
  return [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16));
}

function luminance(hex: string): number {
  const [r = 0, g = 0, b = 0] = channels(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio, 1 to 21. */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return ((x ?? 0) + 0.05) / ((y ?? 0) + 0.05);
}

function mix(hex: string, toward: string, t: number): string {
  const from = channels(hex);
  const to = channels(toward);
  return `#${from
    .map((c, i) =>
      Math.round(c + ((to[i] ?? c) - c) * t)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

/** `hex`, darkened or lightened just enough to reach `minimum` on every background. */
export function shadeFor(hex: string, grounds: string[], toward: string, minimum: number): string {
  for (let step = 0; step <= 100; step++) {
    const shade = mix(hex, toward, step / 100);
    if (grounds.every((g) => contrast(shade, g) >= minimum)) return shade;
  }
  return toward;
}

/**
 * A CSS colour that is `hex` where it's readable enough and a nearby shade
 * where it isn't, in both themes. 3:1 by default, enough for icons; text
 * needs 4.5:1.
 */
export function readable(hex: string, minimum = 3): string {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) return hex;
  const light = shadeFor(hex, LIGHT, "#000000", minimum);
  const dark = shadeFor(hex, DARK, "#ffffff", minimum);
  return `light-dark(${light}, ${dark})`;
}

/** Translucent status fill, with ink checked against the resulting composite. */
export function statusStyle(color = "var(--muted)"): CSSProperties & {
  "--status-color": string;
  "--status-ink": string;
} {
  const semantic = /^var\(--(ok|warn|danger|info|muted|faint)\)$/.exec(color)?.[1];
  if (semantic) {
    const name = semantic === "muted" || semantic === "faint" ? "neutral" : semantic;
    return {
      "--status-color": name === "neutral" ? "var(--muted)" : color,
      "--status-ink": `var(--status-${name}-ink)`,
    };
  }
  if (!/^#[0-9a-f]{6}$/i.test(color)) return statusStyle();
  const ink = (grounds: string[], toward: string) =>
    shadeFor(
      color,
      grounds.map((ground) => mix(ground, color, colors.opacity["status-tint"])),
      toward,
      colors.contrast.text,
    );
  return {
    "--status-color": color,
    "--status-ink": `light-dark(${ink(LIGHT, "#000000")}, ${ink(DARK, "#ffffff")})`,
  };
}
