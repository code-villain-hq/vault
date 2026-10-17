# ADR 0003: A small Postgres job queue, built in-house

- **Status:** Accepted
- **Date:** 2026-09-25

## Context

Webhooks must be acknowledged fast and processed reliably. Release contents and outcomes need scheduled, retryable work. Mature libraries exist (for example River for Go), but building a small queue is a core learning goal of this project and a good interview story.

## Decision

A `jobs` table with `FOR UPDATE SKIP LOCKED` claiming, exponential backoff, `max_attempts`, a `dead` state, `dedupe_key` for idempotent enqueueing, and a reaper for stuck jobs. Enqueue inside the same transaction as the write that caused it, so a job exists if and only if its cause was committed.

## Consequences

- Transactional enqueueing, with no dual-write problem between the database and a broker.
- About 300–500 lines of Go that must be well tested (concurrency, retries, crash recovery).
- Revisit if throughput exceeds a few hundred jobs per second, or if the queue code becomes a distraction. Switching to River is the fallback, because it uses the same Postgres.
