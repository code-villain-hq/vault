// How the kit's code is formatted and linted: each app's check runs
// `vp check` here, with its own copy of Vite+.
import { fmt, lint } from "../dev-id/vite-plus";

export default { fmt: fmt(), lint: lint() };
