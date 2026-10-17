# ADR 0004: Server-authoritative sync with row upserts

- **Status:** Accepted
- **Date:** 2026-09-25

## Context

The UI must feel instant and show changes by others (people and GitHub) live. Full local-first sync with offline writes and conflict-free merging is powerful but a large project on its own.

## Decision

- **Server-authoritative.** The server decides the truth, and the client keeps a cache.
- The client cache lives in IndexedDB (Dexie). The UI reads only from the cache.
- Bootstrap snapshot, then a WebSocket stream of **row upserts and removals** in `seq` order, with catch-up from a cursor.
- Writes go over REST with a client mutation id; they're applied optimistically and confirmed or rolled back when the echo arrives.
- Fan-out through Postgres `LISTEN/NOTIFY` on one channel (`schemer_events`, payload `<workspace_id>:<seq>`), with a per-workspace advisory lock on writes so a cursor never skips a row.

## Consequences

- Instant reads, live updates, simple reasoning: last write wins on the server, per field.
- Projection logic exists only in Go; the client just stores rows.
- Offline **writes** are out of scope for v1. A queue of pending mutations can be added later on top of the same protocol.
- If `NOTIFY` becomes a bottleneck with many server instances, move fan-out to a dedicated channel. Not expected in v1.
