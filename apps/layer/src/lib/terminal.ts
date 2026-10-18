import type { ITheme } from "@xterm/xterm";
import { isLight, type Theme } from "./theme";
import code from "../design/syntax.json";

/**
 * Nightshade's terminal in one mode, from the vault's code palette
 * (design/tokens/theme.md, Code). The terminal sits in the content panel,
 * which is the code ground, so it is drawn in that, with the code's text,
 * cursor and selection. The sixteen colours are the code palette's: the
 * syntax hues at one lightness, kept apart from the brand's poison. By day
 * "white" is a grey: programs print it meaning "readable", assuming a dark
 * background, as every light terminal's palette does.
 */
function nightshade(mode: typeof code.night): ITheme {
  const [black, red, green, yellow, blue, magenta, cyan, white] = mode.ansi.normal;
  const [brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite] =
    mode.ansi.bright;
  return {
    background: mode.ui.background,
    foreground: mode.syntax.text,
    cursor: mode.ui.cursor,
    cursorAccent: mode.ui.background,
    selectionBackground: mode.ui.selection,
    black, red, green, yellow, blue, magenta, cyan, white,
    brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite,
  };
}

/** What every terminal in the app draws with, per theme: the window's and the phone's (SET-5). */
const PALETTES: Record<Theme, ITheme> = {
  night: nightshade(code.night),
  day: nightshade(code.day),
  "tokyo-night": {
    background: "#1f2335",
    foreground: "#c0caf5",
    cursor: "#c0caf5",
    cursorAccent: "#1f2335",
    selectionBackground: "#283457",
    black: "#15161e", red: "#f7768e", green: "#9ece6a", yellow: "#e0af68",
    blue: "#7aa2f7", magenta: "#bb9af7", cyan: "#7dcfff", white: "#a9b1d6",
    brightBlack: "#414868", brightRed: "#f7768e", brightGreen: "#9ece6a",
    brightYellow: "#e0af68", brightBlue: "#7aa2f7", brightMagenta: "#bb9af7",
    brightCyan: "#7dcfff", brightWhite: "#c0caf5",
  },
  // Cursor's own terminal colours (theme-cursor), on the page's --bg rather
  // than its panel's, and opaque where Cursor's are see-through.
  "cursor-dark": {
    background: "#181818",
    foreground: "#f0f0f0",
    cursor: "#f0f0f0",
    cursorAccent: "#181818",
    selectionBackground: "#3b3b3b",
    black: "#242424", red: "#fc6b83", green: "#3fa266", yellow: "#d2943e",
    blue: "#81a1c1", magenta: "#b48ead", cyan: "#88c0d0", white: "#f0f0f0",
    brightBlack: "#9a9a9a", brightRed: "#fc6b83", brightGreen: "#70b489",
    brightYellow: "#f1b467", brightBlue: "#87a6c4", brightMagenta: "#b48ead",
    brightCyan: "#88c0d0", brightWhite: "#ffffff",
  },
  "cursor-light": {
    background: "#fcfcfc",
    foreground: "#141414",
    cursor: "#141414",
    cursorAccent: "#fcfcfc",
    selectionBackground: "#dbdbdb",
    black: "#141414", red: "#be1744", green: "#007041", yellow: "#8b5700",
    blue: "#0064b0", magenta: "#92156a", cyan: "#176c74", white: "#6c6c6c",
    brightBlack: "#505050", brightRed: "#ce405b", brightGreen: "#00854c",
    brightYellow: "#a46700", brightBlue: "#2778c1", brightMagenta: "#b54e90",
    brightCyan: "#3b7e84", brightWhite: "#949494",
  },
  // Ayu's terminal colours as its extension publishes them, on the page's
  // --bg, with its accent for a cursor as its editor has.
  "ayu-dark": {
    background: "#10141c",
    foreground: "#bfbdb6",
    cursor: "#e6b450",
    cursorAccent: "#10141c",
    selectionBackground: "#193155",
    black: "#1b1f29", red: "#f06b73", green: "#70bf56", yellow: "#fdb04c",
    blue: "#4fbfff", magenta: "#d0a1ff", cyan: "#93e2c8", white: "#c7c7c7",
    brightBlack: "#686868", brightRed: "#f07178", brightGreen: "#aad94c",
    brightYellow: "#ffb454", brightBlue: "#59c2ff", brightMagenta: "#d2a6ff",
    brightCyan: "#95e6cb", brightWhite: "#ffffff",
  },
  "ayu-light": {
    background: "#fcfcfc",
    foreground: "#5c6166",
    cursor: "#f29718",
    cursorAccent: "#fcfcfc",
    selectionBackground: "#d7e4f6",
    black: "#000000", red: "#f06b6c", green: "#6cbf43", yellow: "#e7a100",
    blue: "#21a1e2", magenta: "#a176cb", cyan: "#4abc96", white: "#c7c7c7",
    brightBlack: "#686868", brightRed: "#f07171", brightGreen: "#86b300",
    brightYellow: "#eba400", brightBlue: "#22a4e6", brightMagenta: "#a37acc",
    brightCyan: "#4cbf99", brightWhite: "#d1d1d1",
  },
};

/**
 * A terminal's options for `theme`. An agent picks its own colours, often as
 * exact RGB for a dark background, and xterm darkens any too faint to read
 * on a light one. Nothing changes on a dark one, where agents expect to be.
 */
export function terminalTheme(theme: Theme): { theme: ITheme; minimumContrastRatio: number } {
  return { theme: PALETTES[theme], minimumContrastRatio: isLight(theme) ? 4.5 : 1 };
}

export const TERMINAL_FONT = '"Atkinson Hyperlegible Mono", ui-monospace, Menlo, monospace';

/**
 * The studio's faces, loaded before the first render. xterm measures its
 * cell from the font it is given when it opens; measured on the fallback, a
 * terminal kept the fallback's cell after the bundled mono arrived. Never
 * waits long: past a second the page draws with what it has.
 */
export function fontsReady(): Promise<unknown> {
  const faces = ['13px "Atkinson Hyperlegible Mono"', '14px "Atkinson Hyperlegible Next"', '700 28px "Grenze Gotisch"'];
  return Promise.race([
    Promise.all(faces.map((f) => document.fonts.load(f))).catch(() => {}),
    new Promise((done) => setTimeout(done, 1000)),
  ]);
}

/** A pane's output as the backend sends it, base64, back to bytes. */
export function decode(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
