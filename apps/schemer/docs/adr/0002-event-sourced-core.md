# ADR 0002: Event-sourced core with synchronous projections

- **Status:** Accepted
- **Date:** 2026-09-25

## Context

The product's promise is "why did we build this, and did it work?" That needs a complete, ordered history of everything that happened to every node: who or what changed it, when, and why. The timeline UI, undo, derived status and future analytics all need the same history.

## Decision

- Every state change is an event appended to `events` (append-only, ordered by `seq`).
- Projections (`work_items`, `changes`, ...) are updated **in the same transaction** as the event.
- Projections are rebuildable by replaying events through the same `Apply` functions.
- Retries are made safe with `idempotency_key` on events.

## Consequences

- Full history and a timeline for free; undo is a compensating event.
- No eventual consistency to reason about in v1, because a read after a write sees the write.
- Every write path must go through the event store. Direct `UPDATE`s on projections are a bug. Enforce this in code review and with a test that replays the log and compares projections.
- Event payloads become a contract. Evolve them additively (new optional fields); never change the meaning of an existing field.
