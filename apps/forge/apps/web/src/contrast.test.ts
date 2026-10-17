import { expect, test } from "bun:test";
import { contrast, readable, shadeFor, statusStyle } from "./contrast";
import colors from "./design/color.json";

test("contrast follows WCAG", () => {
  expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
  expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 1);
});

test("colours that are readable stay as they are", () => {
  expect(shadeFor("#5b5bd6", ["#ffffff"], "#000000", 3)).toBe("#5b5bd6");
});

test("pale colours get just dark enough for light mode, and the reverse", () => {
  const yellow = shadeFor("#f2c94c", ["#ffffff", "#ececf0"], "#000000", 3);
  expect(contrast(yellow, "#ececf0")).toBeGreaterThanOrEqual(3);
  expect(contrast(yellow, "#ececf0")).toBeLessThan(3.2);
  expect(readable("#f2c94c")).toStartWith("light-dark(#");
  expect(readable("tomato")).toBe("tomato");
});

const hosts = [
  "bg",
  "panel",
  "surface",
  "hover",
  "selected",
  "dialog",
  "field",
  "field-hover",
  "dialog-field",
  "dialog-field-hover",
] as const;

// Model the browser's translucent sRGB fill over each real host.
function composite(source: string, ground: string): string {
  const opacity = colors.opacity["status-tint"];
  return `#${[1, 3, 5]
    .map((i) =>
      Math.round(
        Number.parseInt(source.slice(i, i + 2), 16) * opacity +
          Number.parseInt(ground.slice(i, i + 2), 16) * (1 - opacity),
      )
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

test("shared status ink is AA on the translucent fill in every supported context", () => {
  for (const mode of ["day", "night"] as const) {
    const theme = colors[mode];
    for (const status of ["ok", "warn", "danger", "info", "neutral"] as const) {
      const source = status === "neutral" ? theme.muted : theme[status];
      for (const host of hosts) {
        expect(
          contrast(theme[`status-${status}-ink`], composite(source, theme[host])),
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  }
});

test("custom status hues stay unchanged while their ink stays readable in both modes", () => {
  for (const hue of ["#ffffff", "#000000", "#f2c94c", "#5b5bd6", "#ff0000", "#00ff00", "#777777"]) {
    const style = statusStyle(hue);
    expect(style["--status-color"]).toBe(hue);
    const inks = style["--status-ink"].match(/#[0-9a-f]{6}/g);
    expect(inks).toHaveLength(2);
    for (const [index, mode] of (["day", "night"] as const).entries()) {
      for (const host of hosts) {
        expect(
          contrast(inks?.[index] ?? "", composite(hue, colors[mode][host])),
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  }
  expect(statusStyle("var(--ok)")["--status-ink"]).toBe("var(--status-ok-ink)");
  expect(statusStyle("invalid")).toEqual(statusStyle());
});

test("dialog fields remain distinct and their placeholders remain AA", () => {
  for (const mode of ["day", "night"] as const) {
    const theme = colors[mode];
    for (const role of ["dialog", "dialog-field", "dialog-field-hover"] as const) {
      expect(contrast(theme.faint, theme[role])).toBeGreaterThanOrEqual(4.5);
    }
    expect(theme["dialog-field"]).not.toBe(theme.dialog);
    expect(theme["dialog-field-hover"]).not.toBe(theme.dialog);
  }
});
