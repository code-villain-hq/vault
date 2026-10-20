// Bundle the vault's shared design sources for standalone/Docker builds.
import { mkdir } from "node:fs/promises";

const vault = new URL("../../../", import.meta.url);
const destination = new URL("../apps/web/src/design/", import.meta.url);
const files = [
  ["design/tokens/color.resolved.json", "color.json"],
  ...["color.css", "type.css", "shape.css"].map((name) => [`design/tokens/${name}`, name]),
  ["design/components.css", "components.css"],
  ...[
    "fonts.css",
    "grenze-gotisch.ttf",
    "grenze-gotisch-OFL.txt",
    "atkinson-hyperlegible-next.ttf",
    "atkinson-hyperlegible-next-italic.ttf",
    "atkinson-hyperlegible-next-OFL.txt",
    "atkinson-hyperlegible-mono.ttf",
    "atkinson-hyperlegible-mono-OFL.txt",
  ].map((name) => [`apps/site/assets/fonts/${name}`, `fonts/${name}`]),
];

const copies = [
  ...files.map(([source, target]) => {
    if (!source || !target) throw new Error("missing design source or destination");
    return [new URL(source, vault), new URL(target, destination)] as const;
  }),
  [
    new URL("brand/mark/glyph.svg", vault),
    new URL("../apps/web/public/icon.svg", import.meta.url),
  ] as const,
];

// `--check` writes nothing and fails if the bundled copy has drifted from the vault.
if (process.argv.includes("--check")) {
  const glyph = new URL("../", import.meta.url).pathname;
  const stale: string[] = [];
  for (const [source, target] of copies) {
    const copy = Bun.file(target);
    const same =
      (await copy.exists()) &&
      Buffer.from(await copy.arrayBuffer()).equals(
        Buffer.from(await Bun.file(source).arrayBuffer()),
      );
    if (!same) stale.push(target.pathname.slice(glyph.length));
  }
  if (stale.length) {
    console.error(`Out of date with the vault: ${stale.join(", ")}. Run bun run design:sync.`);
    process.exit(1);
  }
} else {
  await mkdir(new URL("fonts/", destination), { recursive: true });
  for (const [source, target] of copies) await Bun.write(target, Bun.file(source));
  console.log("Synced shared color/type/shape tokens, licensed fonts, and the Glyph mark.");
}
