// How studio/design's code and shared CSS are formatted and linted. The design
// lint runs `vp check` here (package.json).
import { fmt, lint } from "../../code/kit/dev-id/vite-plus";

// Written by tokens/build.py, or by the tools that drew the pets.
const generated = [
  "tokens/*.css",
  "tokens/*.resolved.json",
  "pets/**",
  "pocket-id/kit/**",
  "vaultwarden/kit/**",
];

export default { fmt: fmt(generated), lint: { ...lint(), ignorePatterns: generated } };
