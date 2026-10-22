// Tailwind arbitrary values and palette colours in markup bypass the tokens
// where stylelint can't see them: `text-[12px]`, `bg-[#111]`, `bg-zinc-800`,
// `text-white`. Scans the TSX/TS of the web kit, Forge, Layer (the app and its server's web app), Glyph and Raven. A
// line that must keep one says why with `design-lint-ignore` in a comment.
import { Glob } from "bun";

const roots: [string, string][] = [
  ["../../../code/kit/web/src", "**/*.{ts,tsx}"],
  ["../../../code/apps/forge/apps/web/src", "**/*.{ts,tsx}"],
  ["../../../code/apps/layer/src", "**/*.{ts,tsx}"],
  ["../../../code/apps/layer/web/src", "**/*.{ts,tsx}"],
  ["../../../code/apps/glyph/apps/web/src", "**/*.{ts,tsx}"],
  ["../../../code/apps/raven/apps/web/src", "**/*.{ts,tsx}"],
];
const skip = [/\/design\//, /\/phone\//, /\.test\.tsx?$/, /\/__mock__\//];
const palette =
  "white|black|slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose";
const prefixes = "bg|text|border|ring|fill|stroke|outline|decoration|divide|from|to|via|shadow|caret|accent";
const rules: [RegExp, string][] = [
  [new RegExp(`\\b(?:${prefixes})-\\[(?:#|rgb|hsl|oklch)`), "a literal colour"],
  [new RegExp(`\\b(?:${prefixes})-(?:${palette})(?:-\\d{2,3})?(?:\\/\\d+)?\\b`), "a Tailwind palette colour"],
  [/\btext-\[\d+(?:\.\d+)?(?:px|rem)\]/, "a literal type size"],
  [/\brounded-\[/, "a literal radius"],
  [/style=(?:\{\{|")[^}"]*#[0-9a-fA-F]{3,8}\b/, "a literal colour in a style"],
];

const found: string[] = [];
for (const [root, glob] of roots) {
  for await (const file of new Glob(glob).scan(root)) {
    const path = `${root}/${file}`;
    if (skip.some((pattern) => pattern.test(path))) continue;
    const lines = (await Bun.file(path).text()).split("\n");
    lines.forEach((line, index) => {
      if (line.includes("design-lint-ignore")) return;
      for (const [pattern, what] of rules) {
        const match = line.match(pattern);
        if (match) found.push(`${path.replace("../../../", "")}:${index + 1}  ${what}: ${match[0]}`);
      }
    });
  }
}
if (found.length) {
  console.error(`${found.join("\n")}\n\n${found.length} class(es) bypass the design tokens. Use a token-backed class, or say why with design-lint-ignore.`);
  process.exit(1);
}
console.log("No arbitrary colours, sizes or radii in the web kit's, Forge's, Layer's, Glyph's or Raven's markup.");
