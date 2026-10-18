// A specimen of the Code Villain theme on the vault's own code, night and day, for
// reviewing a palette change. Writes specimen.html (self-contained, so it can
// be shared as is). Run `bun run build` first: it reads the VS Code themes.
import { createHighlighter } from "shiki";

const vault = new URL("../../", import.meta.url);
const here = new URL("./", import.meta.url);
const text = (path: string) => Bun.file(new URL(path, vault)).text();
const lines = async (path: string, from: number, to: number) =>
  (await text(path)).split("\n").slice(from - 1, to).join("\n");
const theme = async (mode: string) => JSON.parse(await Bun.file(new URL(`vscode/themes/code-villain-${mode}.json`, here)).text());
const syntax = JSON.parse(await text("design/tokens/syntax.resolved.json"));
const color = JSON.parse(await text("design/tokens/color.resolved.json"));

/** A sample: a slice of a real file, found by the line it starts with. */
async function slice(path: string, start: string, count: number) {
  const all = (await text(path)).split("\n");
  const at = all.findIndex((l) => l.startsWith(start));
  if (at < 0) throw new Error(`${path} no longer has a line starting ${start}`);
  return all.slice(at, at + count).join("\n");
}

const samples = [
  { title: "TypeScript", file: "apps/layer/src/store.ts", lang: "ts", code: await slice("apps/layer/src/store.ts", "import { keep, only", 40) },
  { title: "TypeScript", file: "apps/layer/src/lib/diff.ts", lang: "ts", code: await slice("apps/layer/src/lib/diff.ts", "/** Parse a unified patch", 33) },
  { title: "Rust", file: "apps/layer/src-tauri/src/git.rs", lang: "rust", code: await slice("apps/layer/src-tauri/src/git.rs", "pub fn baseline", 31) },
  { title: "Python", file: "design/tokens/theme.py", lang: "python", code: await slice("design/tokens/theme.py", "def mix(", 26) },
  { title: "Markdown", file: "brand/color.md", lang: "markdown", code: await lines("brand/color.md", 1, 24) },
];

// A diff as Layer shows it: the code coloured, the lines tinted, the changed words marked.
const before = `export function contrast(a: string, b: string): number {
  const [low, high] = [luminance(a), luminance(b)].sort();
  return (high + 0.05) / (low + 0.05);
}`;
const after = `/** WCAG contrast of two sRGB colours, 1 to 21. */
export function contrast(a: string, b: string): number {
  const [low, high] = [luminance(a), luminance(b)].sort((x, y) => x - y);
  return (high + 0.05) / (low + 0.05);
}`;
/** Where the changed words sit in a line, as Layer's word diff would find them. */
const span = (code: string, line: number, words: string): [number, number] => {
  const at = code.split("\n")[line].lastIndexOf(words);
  return [at, at + words.length];
};
const rows: { kind: "ctx" | "add" | "del"; side: "old" | "new"; line: number; mark?: [number, number] }[] = [
  { kind: "add", side: "new", line: 0 },
  { kind: "ctx", side: "new", line: 1 },
  { kind: "del", side: "old", line: 1, mark: span(before, 1, "()") },
  { kind: "add", side: "new", line: 2, mark: span(after, 2, "((x, y) => x - y)") },
  { kind: "ctx", side: "new", line: 3 },
  { kind: "ctx", side: "new", line: 4 },
];

const highlighter = await createHighlighter({
  themes: [await theme("night"), await theme("day")],
  langs: ["ts", "rust", "python", "markdown", "diff"],
});
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const style = (t: { color?: string; fontStyle?: number }) => {
  const css = [`color:${t.color}`];
  if (t.fontStyle && t.fontStyle & 1) css.push("font-style:italic");
  if (t.fontStyle && t.fontStyle & 2) css.push("font-weight:700");
  if (t.fontStyle && t.fontStyle & 4) css.push("text-decoration:underline");
  if (t.fontStyle && t.fontStyle & 8) css.push("text-decoration:line-through");
  return css.join(";");
};

function block(code: string, lang: string, mode: string, extra?: (line: number) => string) {
  const { tokens } = highlighter.codeToTokens(code, { lang, theme: `Code Villain ${mode === "night" ? "Night" : "Day"}` });
  return tokens
    .map((line, i) => {
      const body = line.map((t) => `<span style="${style(t)}">${esc(t.content)}</span>`).join("");
      return `<div class="row ${extra?.(i) ?? ""}"><span class="n">${i + 1}</span><span class="c">${body || " "}</span></div>`;
    })
    .join("");
}

function diffBlock(mode: string) {
  const name = `Code Villain ${mode === "night" ? "Night" : "Day"}`;
  const old = highlighter.codeToTokens(before, { lang: "ts", theme: name }).tokens;
  const neu = highlighter.codeToTokens(after, { lang: "ts", theme: name }).tokens;
  return rows
    .map((r) => {
      const line = (r.side === "old" ? old : neu)[r.line];
      let at = 0;
      const body = line
        .flatMap((t) => {
          // Split a token where the changed words begin and end.
          const parts: string[] = [];
          const cuts = [0, t.content.length];
          if (r.mark) for (const c of r.mark) if (c > at && c < at + t.content.length) cuts.push(c - at);
          cuts.sort((a, b) => a - b);
          for (let i = 0; i < cuts.length - 1; i++) {
            const piece = t.content.slice(cuts[i], cuts[i + 1]);
            const start = at + cuts[i];
            const marked = r.mark && start >= r.mark[0] && start < r.mark[1];
            parts.push(`<span class="${marked ? "w" : ""}" style="${style(t)}">${esc(piece)}</span>`);
          }
          at += t.content.length;
          return parts;
        })
        .join("");
      const sign = r.kind === "add" ? "+" : r.kind === "del" ? "−" : " ";
      return `<div class="row ${r.kind}"><span class="n">${sign}</span><span class="c">${body}</span></div>`;
    })
    .join("");
}

// The editor's own moments: cursor line, selection, search matches, a matching bracket.
function editorBlock(mode: string) {
  const code = `const ground = theme.panel;\nconst ink = readable(theme.fg, [ground], "night", 4.6);\nreturn contrast(ink, ground) >= 4.6;`;
  const html = block(code, "ts", mode, (i) => (i === 1 ? "cursorline" : ""));
  return html
    .replace(/(<span[^>]*>)(ground)(<\/span>)/, `$1<mark class="cur">$2</mark>$3`)
    .replace(/(<span[^>]*>)(ground)(<\/span>)(?![\s\S]*cur)/, "$1$2$3")
    .replace(/>contrast</, '><span class="sel">contrast</span><')
    .replace(/(\[)(<\/span><span[^>]*>)(ground)/, '<span class="match">$1</span>$2<mark class="hit">$3</mark>');
}

const swatches = (mode: string) => {
  const s = syntax[mode].syntax;
  const roles = ["text", "keyword", "property", "string", "constant", "function", "special", "bracket", "punctuation"];
  const seen = new Set<string>();
  return roles
    .filter((r) => !seen.has(s[r]) && seen.add(s[r]))
    .map((r) => {
      const name = { text: "names · types", property: "property · operator · built-in type", special: "this · self · tags", punctuation: "punctuation · comment" }[r] ?? r;
      return `<div class="sw"><i style="background:${s[r]}"></i><b>${name}</b><code>${s[r]}</code></div>`;
    })
    .join("");
};

const font = Buffer.from(await Bun.file(new URL("apps/site/assets/fonts/atkinson-hyperlegible-mono.ttf", vault)).arrayBuffer()).toString("base64");
const reading = Buffer.from(await Bun.file(new URL("apps/site/assets/fonts/atkinson-hyperlegible-next.ttf", vault)).arrayBuffer()).toString("base64");

const vars = (mode: string) => {
  const t = color[mode];
  const u = syntax[mode].ui;
  return [
    `--bg:${t.bg}`, `--panel:${u.background}`, `--fg:${t.fg}`, `--muted:${t.muted}`, `--faint:${t.faint}`,
    `--n:${u["line-number"]}`, `--cl:${u.cursorline}`, `--sel:${u.selection}`, `--hit:${u.search}`,
    `--cur:${u["search-current"]}`, `--on-cur:${u["on-search-current"]}`, `--match:${u.match}`, `--cursor:${u.cursor}`,
    `--add:${u["diff-added"]}`, `--add-w:${u["diff-added-text"]}`, `--del:${u["diff-removed"]}`, `--del-w:${u["diff-removed-text"]}`,
    `--ok:${u.ok}`, `--danger:${u.danger}`,
  ].join(";");
};

const patch = await Bun.file(new URL("specimen.diff", here)).text();
const column = (mode: string) => `
<section class="mode" style="${vars(mode)}">
  <h2>${mode === "night" ? "Night" : "Day"}</h2>
  <div class="swatches">${swatches(mode)}</div>
  <h3>Editor</h3><pre class="code">${editorBlock(mode)}</pre>
  <p class="note">Cursor line, a selection, search matches with the current one in poison, the bracket under the cursor in cloak.</p>
  <h3>Diff, as Layer draws it</h3><pre class="code">${diffBlock(mode)}</pre>
  ${samples.map((s) => `<h3>${s.title} <span>${s.file}</span></h3><pre class="code">${block(s.code, s.lang, mode)}</pre>`).join("")}
  <h3>Diff file <span>git diff</span></h3><pre class="code">${block(patch, "diff", mode)}</pre>
</section>`;

const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Code Villain theme</title>
<style>
@font-face { font-family: "Atkinson Hyperlegible Mono"; src: url(data:font/ttf;base64,${font}); }
@font-face { font-family: "Atkinson Hyperlegible Next"; src: url(data:font/ttf;base64,${reading}); font-weight: 200 800; }
* { box-sizing: border-box; }
body { margin: 0; background: ${color.night.bg}; color: ${color.night.fg}; font: 15px/1.5 "Atkinson Hyperlegible Next", system-ui, sans-serif; }
header { padding: 32px 24px 8px; max-width: 1400px; margin: 0 auto; }
header h1 { margin: 0 0 6px; font-size: 22px; }
header p { margin: 0; color: ${color.night.muted}; max-width: 70ch; }
main { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 620px), 1fr)); max-width: 1400px; margin: 0 auto; }
.mode { background: var(--bg); color: var(--fg); padding: 20px 24px 32px; min-width: 0; }
h2 { margin: 0 0 12px; font-size: 18px; }
h3 { margin: 22px 0 8px; font-size: 13px; font-weight: 700; color: var(--muted); }
h3 span { font-weight: 400; color: var(--faint); margin-left: 6px; }
.note { margin: 6px 0 0; font-size: 13px; color: var(--muted); }
.swatches { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 6px 14px; font-size: 13px; }
.sw { display: flex; align-items: center; gap: 8px; }
.sw i { width: 14px; height: 14px; border-radius: 4px; flex: none; }
.sw b { font-weight: 400; }
.sw code { color: var(--faint); font: 12px "Atkinson Hyperlegible Mono", monospace; margin-left: auto; }
pre.code { margin: 0; background: var(--panel); border-radius: 10px; padding: 10px 0; overflow-x: auto; font: 13px/1.6 "Atkinson Hyperlegible Mono", ui-monospace, monospace; }
.row { display: flex; padding-right: 16px; width: max-content; min-width: 100%; }
.n { width: 3.2em; flex: none; text-align: right; padding-right: 1.2em; color: var(--n); user-select: none; }
.c { white-space: pre; }
.cursorline { background: var(--cl); }
.cursorline .n { color: var(--fg); }
.sel { background: var(--sel); }
mark { color: inherit; background: var(--hit); }
mark.cur { background: var(--cur); color: var(--on-cur) !important; }
.match { outline: 1px solid var(--match); outline-offset: -1px; }
.add { background: var(--add); } .add .n { color: var(--ok); }
.del { background: var(--del); } .del .n { color: var(--danger); }
.add .w { background: var(--add-w); } .del .w { background: var(--del-w); }
.cursorline .c::before { content: ""; }
</style></head>
<body>
<header><h1>Code Villain theme</h1>
<p>Six hues at one lightness carry what you scan for, the way tokyonight chooses: keywords, properties and operators, strings, constants, functions, and the language’s own names. Names and types stay in the text colour; grey steps back for brackets, then punctuation and comments. Poison is only the search match; cloak is the cursor and focus.</p></header>
<main>${column("night")}${column("day")}</main>
</body></html>
`;

await Bun.write(new URL("specimen.html", here), html);
console.log("Wrote specimen.html");
