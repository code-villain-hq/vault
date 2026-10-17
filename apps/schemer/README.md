# Schemer

*A Code Villain product.*

A product delivery tool that feeds itself. Work items update themselves from GitHub, CI and deploys, and every release is connected to the metric it was meant to move.

> Status: planning. Nothing is built yet. Milestone M0 starts the code.

## Why

Code is cheap now. Tickets, docs and tests can't keep up, because tools like Jira have to be fed by hand. Schemer keeps itself current from the repo, CI and deploys, and shows where the code has run ahead of everything else. Read [docs/VISION.md](docs/VISION.md).

## Docs

| Doc | What's in it |
| --- | --- |
| [VISION.md](docs/VISION.md) | Problem, thesis, the idea-to-impact loop, customer, principles |
| [V1_SCOPE.md](docs/V1_SCOPE.md) | v1 features, acceptance criteria, performance budgets |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Components, stack, repo layout, key flows |
| [DATA_MODEL.md](docs/DATA_MODEL.md) | Events, projections, graph, derived-status rules |
| [API.md](docs/API.md) | REST endpoints and the sync protocol |
| [ROADMAP.md](docs/ROADMAP.md) | Milestones M0–M4 with tasks |
| [adr/](docs/adr/) | Architecture decision records |

## Stack

Go · PostgreSQL · React · TypeScript · Vite · Tailwind CSS · GitHub App

## Getting started

Coming in M0 (`make dev`).
