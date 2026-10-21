import { readFileSync } from "node:fs";

/** The version a Cargo.toml gives, for Settings' last line (`__APP_VERSION__`). */
export function cargoVersion(cargoToml: URL): string {
  return /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(cargoToml, "utf8"))?.[1] ?? "0.0.0";
}
