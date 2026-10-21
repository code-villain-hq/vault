/**
 * Code colours for the Diff tab (DIFF-9): the TextMate grammars VS Code and
 * Cursor use, read through Shiki, with the Code Villain theme's scopes from
 * the vault (studio/design/tokens/syntax). The colours are CSS variables, so every
 * theme, and a theme preview drawn inside another, paints its own without
 * colouring again: theme.css fills `--syntax-*` for each.
 */
import { createHighlighterCore, type HighlighterCore, type LanguageRegistration } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import code from "../design/syntax.json";
import { hunkSides, type DiffLine } from "./diff";

/** One run of a line's code in one colour. */
export interface Token {
  content: string;
  color?: string;
  fontStyle?: number;
}

type Grammar = () => Promise<{ default: LanguageRegistration[] }>;

// The languages Layer colours, each loaded the first time a file needs it.
// Listed one by one so the build carries these grammars and no others.
const GRAMMARS: Record<string, Grammar> = {
  typescript: () => import("shiki/langs/typescript.mjs"),
  tsx: () => import("shiki/langs/tsx.mjs"),
  javascript: () => import("shiki/langs/javascript.mjs"),
  jsx: () => import("shiki/langs/jsx.mjs"),
  json: () => import("shiki/langs/json.mjs"),
  jsonc: () => import("shiki/langs/jsonc.mjs"),
  rust: () => import("shiki/langs/rust.mjs"),
  python: () => import("shiki/langs/python.mjs"),
  go: () => import("shiki/langs/go.mjs"),
  java: () => import("shiki/langs/java.mjs"),
  kotlin: () => import("shiki/langs/kotlin.mjs"),
  swift: () => import("shiki/langs/swift.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  scss: () => import("shiki/langs/scss.mjs"),
  html: () => import("shiki/langs/html.mjs"),
  markdown: () => import("shiki/langs/markdown.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
  toml: () => import("shiki/langs/toml.mjs"),
  sql: () => import("shiki/langs/sql.mjs"),
  shellscript: () => import("shiki/langs/shellscript.mjs"),
  dockerfile: () => import("shiki/langs/dockerfile.mjs"),
  ruby: () => import("shiki/langs/ruby.mjs"),
  php: () => import("shiki/langs/php.mjs"),
  c: () => import("shiki/langs/c.mjs"),
  cpp: () => import("shiki/langs/cpp.mjs"),
  csharp: () => import("shiki/langs/csharp.mjs"),
  xml: () => import("shiki/langs/xml.mjs"),
  graphql: () => import("shiki/langs/graphql.mjs"),
  vue: () => import("shiki/langs/vue.mjs"),
  svelte: () => import("shiki/langs/svelte.mjs"),
  make: () => import("shiki/langs/make.mjs"),
  lua: () => import("shiki/langs/lua.mjs"),
  ini: () => import("shiki/langs/ini.mjs"),
  dotenv: () => import("shiki/langs/dotenv.mjs"),
};

const BY_EXTENSION: Record<string, string> = {
  ts: "typescript", mts: "typescript", cts: "typescript", tsx: "tsx",
  js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "jsx",
  json: "json", jsonc: "jsonc", rs: "rust", py: "python", go: "go",
  java: "java", kt: "kotlin", kts: "kotlin", swift: "swift",
  css: "css", scss: "scss", html: "html", htm: "html",
  md: "markdown", markdown: "markdown", yml: "yaml", yaml: "yaml", toml: "toml",
  sql: "sql", sh: "shellscript", bash: "shellscript", zsh: "shellscript",
  rb: "ruby", php: "php", c: "c", h: "c", cc: "cpp", cpp: "cpp", hpp: "cpp",
  cs: "csharp", xml: "xml", svg: "xml", plist: "xml", graphql: "graphql", gql: "graphql",
  vue: "vue", svelte: "svelte", lua: "lua", ini: "ini", cfg: "ini",
};

const BY_NAME: Record<string, string> = {
  Makefile: "make", makefile: "make", GNUmakefile: "make",
  ".zshrc": "shellscript", ".bashrc": "shellscript", ".zprofile": "shellscript",
};

/** The grammar for a file, by its name; null where Layer bundles none. */
export function languageOf(path: string): string | null {
  const name = path.split("/").pop() ?? "";
  if (BY_NAME[name]) return BY_NAME[name];
  if (name.startsWith(".env")) return "dotenv";
  if (name === "Dockerfile" || name.startsWith("Dockerfile.")) return "dockerfile";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? (BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? null) : null;
}

// The Code Villain theme as the editor themes are built (code/apps/theme): each
// colour role's scopes, here naming the role's CSS variable.
const STYLES: Record<string, string> = code.styles;
const THEME = {
  name: "code-villain",
  type: "dark" as const,
  colors: { "editor.foreground": "var(--syntax-text)", "editor.background": "transparent" },
  settings: [
    { settings: { foreground: "var(--syntax-text)", background: "transparent" } },
    ...Object.entries(code.scopes as Record<string, string[]>).map(([role, scope]) => ({
      scope,
      settings: { foreground: `var(--syntax-${role})`, ...(STYLES[role] ? { fontStyle: STYLES[role] } : {}) },
    })),
  ],
};

let highlighter: Promise<HighlighterCore> | null = null;
function core(): Promise<HighlighterCore> {
  // Forgiving: a grammar pattern the JavaScript engine cannot run is skipped
  // rather than failing the whole file.
  highlighter ??= createHighlighterCore({
    themes: [THEME],
    langs: [],
    engine: createJavaScriptRegexEngine({ forgiving: true }),
  });
  return highlighter;
}

// Past these, a patch is drawn uncoloured: colouring it would hold the window
// longer than reading it in plain text costs.
const MOST_LINES = 3000;
const LONGEST_LINE = 1000;

/**
 * Each line's coloured runs, by index into `lines`, for a patch of `path`.
 * Null when the language is not bundled or the patch is too big; a line
 * outside every hunk (the header) is never coloured.
 */
export async function colourDiff(lines: DiffLine[], path: string): Promise<(Token[] | undefined)[] | null> {
  const lang = languageOf(path);
  if (!lang || lines.length > MOST_LINES) return null;
  const h = await core();
  if (!h.getLoadedLanguages().includes(lang)) await h.loadLanguage((await GRAMMARS[lang]()).default);
  const out: (Token[] | undefined)[] = new Array(lines.length);
  let since = performance.now();
  for (const side of hunkSides(lines).flatMap((s) => [s.old, s.new])) {
    const text = side.map((i) => lines[i].text.slice(1));
    if (!text.length || text.some((t) => t.length > LONGEST_LINE)) continue;
    const runs = h.codeToTokensBase(text.join("\n"), { lang, theme: THEME.name });
    // A context line is in both sides and takes the new one's colours, which
    // come second; a removed line only ever has the old side's.
    side.forEach((i, k) => { out[i] = runs[k]?.map(({ content, color, fontStyle }) => ({ content, color, fontStyle })); });
    // Let the window breathe between sides on a long patch.
    if (performance.now() - since > 12) {
      await new Promise((resume) => setTimeout(resume, 0));
      since = performance.now();
    }
  }
  return out;
}
