# ADR 0006: Deploy target

- **Status:** Proposed (decide in week 3, before the first public deploy)
- **Date:** 2026-09-25

## Context

The server is a long-running process with WebSockets, so it needs a container host, not functions. It also needs managed Postgres with backups. Cost matters (it's a side project), and AWS experience is useful for Michael's CV and his day job.

## Options

| Option | Pros | Cons |
| --- | --- | --- |
| Fly.io with managed Postgres | Fastest path to a live URL, cheap, WebSockets just work | Less relevant for the CV |
| AWS: App Runner or ECS Fargate with RDS Postgres | Strong CV signal, same cloud as the day job | More setup and more monthly cost |
| A single VPS (for example Hetzner) with Docker Compose | Cheapest, full control | You run backups, TLS and updates yourself |

## Decision

To be decided. Default suggestion: start on Fly.io to get live in week 3, and write the deploy so the same Docker image can move to AWS later.
