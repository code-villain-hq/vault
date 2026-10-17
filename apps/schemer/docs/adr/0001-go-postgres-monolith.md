# ADR 0001: One Go service on Postgres

- **Status:** Accepted
- **Date:** 2026-09-25

## Context

Schemer is built in spare time by one engineer who is learning backend development in Go. The product has to feel instant and handle webhooks reliably, but traffic in v1 is small (a few teams).

## Decision

A single Go binary (API, sync, webhook receiver and job workers) backed by a single Postgres database. Postgres also provides the event log, the job queue, full-text search and change notifications (`LISTEN/NOTIFY`).

## Consequences

- One thing to deploy, run, debug and back up.
- Fewer moving parts to learn at once. Every backend concept is visible in one codebase.
- Scaling path when needed: run workers as a separate process of the same binary; add read replicas; move search or the queue out of Postgres only when a measurement shows the need.
- Not allowed in v1 without a new ADR: microservices, Kafka or other brokers, Redis, Kubernetes, ORMs.
