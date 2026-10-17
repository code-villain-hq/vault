# ADR 0005: Integrate through a GitHub App, not an OAuth app

- **Status:** Accepted
- **Date:** 2026-09-25

## Context

Schemer needs pull request, check run and deployment webhooks for selected repositories, plus read access to compare commits. It also needs to sign users in with GitHub.

## Decision

- **Sign-in:** GitHub OAuth (user identity only).
- **Repository access:** a GitHub App installed per account or organisation on the repos the team selects. Webhooks are configured once at the app level. API calls use short-lived installation tokens.

## Consequences

- Fine-grained, read-only permissions per repository, which teams are more comfortable granting.
- Two credentials to manage (OAuth client, and the app's private key plus webhook secret). Keep both in environment variables, never in the repo.
- GitHub doesn't redeliver failed webhooks automatically, so a recovery job redelivers failed deliveries through the app's webhook deliveries API.
- Local development needs a webhook tunnel (for example `smee.io` or a similar forwarding tool) or recorded payload fixtures. Prefer fixtures in tests.
