// Bundle the vault's shared design sources for standalone/Docker builds.
import { mkdir } from "node:fs/promises";

const vault = new URL("../../../", import.meta.url);
const destination = new URL("../apps/web/src/design/", import.meta.url);
const files = [
  ["design/tokens/color.resolved.json", "color.json"],
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

await mkdir(new URL("fonts/", destination), { recursive: true });
for (const [source, target] of files) {
  if (!source || !target) throw new Error("missing design source or destination");
  await Bun.write(new URL(target, destination), Bun.file(new URL(source, vault)));
}
await Bun.write(
  new URL("../apps/web/public/icon.svg", import.meta.url),
  Bun.file(new URL("brand/mark/forge.svg", vault)),
);
console.log("Synced shared color/type/shape tokens, licensed fonts, and the Forge mark.");
