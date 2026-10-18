// Bundle the vault's shared design sources into src/design, so the app and
// the phone's page build from the same tokens and fonts as every other app.
// `--check` writes nothing and fails if the bundled copy has drifted.
import { mkdir } from "node:fs/promises";

const vault = new URL("../../../", import.meta.url);
const destination = new URL("../src/design/", import.meta.url);
const files = [
  ["design/tokens/color.resolved.json", "color.json"],
  ["design/tokens/syntax.resolved.json", "syntax.json"],
  ...["color.css", "type.css", "shape.css"].map((name) => [`design/tokens/${name}`, name]),
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

const copies = files.map(([source, target]) => {
  if (!source || !target) throw new Error("missing design source or destination");
  return [new URL(source, vault), new URL(target, destination)] as const;
});

if (process.argv.includes("--check")) {
  const layer = new URL("../", import.meta.url).pathname;
  const stale: string[] = [];
  for (const [source, target] of copies) {
    const copy = Bun.file(target);
    const same =
      (await copy.exists()) &&
      Buffer.from(await copy.arrayBuffer()).equals(Buffer.from(await Bun.file(source).arrayBuffer()));
    if (!same) stale.push(target.pathname.slice(layer.length));
  }
  if (stale.length) {
    console.error(`Out of date with the vault: ${stale.join(", ")}. Run bun run design:sync.`);
    process.exit(1);
  }
} else {
  await mkdir(new URL("fonts/", destination), { recursive: true });
  for (const [source, target] of copies) await Bun.write(target, Bun.file(source));
  console.log("Synced the shared colour, code, type and shape tokens and the licensed fonts.");
}
