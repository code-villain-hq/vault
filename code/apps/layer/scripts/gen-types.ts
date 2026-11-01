// Regenerates packages/types from the Rust models in crates/domain: ts-rs
// writes one file per type when the domain tests run, then this writes the
// index. The sync protocol comes from the studio's sync client, named with
// Layer's models in the hand-written `sync.ts`.
import { genDomainTypes } from "../../../kit/dev-id/check";

const root = new URL("..", import.meta.url).pathname;
await genDomainTypes(root, "layer-domain");
