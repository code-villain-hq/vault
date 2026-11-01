# Optimizing AI-Driven Testing — Playwright, Integration Tests, and Cost

Research note · 24 October 2026 · Research only; no tests or workflow changes.

**Roadmap:** [E5](README.md#five-experiments-to-prioritize) is the single testing pilot. It is medium–high priority and follows the first verification/architecture pilots unless current records show testing dominates delay. Interface tuning and extra test layers remain conditional on measured bottlenecks.

## 1. Current observations and hypotheses

The developer reports mostly AI-generated unit tests, fewer integration tests, slow browser-driven testing, and Playwright tests that sometimes become flaky or costly to maintain. These are personal observations; attribution to Playwright, the harness, generated code, or test strategy remains unresolved.

Separate three workflows:

- **Agent browsing through MCP:** the model repeatedly observes the page and chooses actions.
- **Agent browsing through a CLI:** the model still chooses actions, using shell commands instead of MCP tools.
- **Saved Playwright Test files:** a conventional runner executes assertions; routine execution needs no LLM, although generation and failure investigation may use one.

The last two are different meanings of “CLI testing.” Moving interactive browsing to a CLI does not eliminate the agent loop. The working hypothesis is that repeatable scenarios can amortize generation cost through saved tests, while exploratory browsing remains useful for discovery. That hypothesis needs local measurement.

## 2. Research findings with sources

| Source and evidence type | Finding | Limit or contradictory evidence |
| --- | --- | --- |
| [Microsoft Playwright MCP](https://github.com/microsoft/playwright-mcp) and [Playwright CLI](https://github.com/microsoft/playwright-cli), official implementation guidance | Microsoft describes CLI/skills as potentially more token-efficient for coding agents by avoiding large tool schemas and page observations; MCP retains rich interactive inspection. | This is architectural guidance, not a universal measured speedup. Both interfaces can drive Playwright. |
| [WebdriverIO browser-agent benchmark, 9 October 2026 run](https://github.com/webdriverio/benchmark/blob/main/results/2026-10-09-run-37883072636/report.md), maintainer-run empirical benchmark | In its DeepSeek run, Playwright MCP versus browser CLI had median costs of $0.028 versus $0.029/task and times of 123 versus 149 seconds; success was 48/100 versus 57/100. | Contradicts a guaranteed CLI saving. Live-site tasks, one run/task, changing environments, and automated judging limit inference. A competing tool's maintainers run it. It does not compare saved regression tests or measure bug detection. |
| [Moretti et al., ESEM 2026, GenE2E](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ESEM.2026.88), academic feasibility study | A specification-to-scenarios-to-Playwright pipeline supported test generation on one smart-city application. Spec quality, page-object completeness, and scenario dependencies affected quality. | Supports separating planning from execution, but not full autonomy. Batch generation reduced model calls/time in that setup; timing was a single hardware-dependent run, not a general efficiency result. |
| [Luo et al., FSE 2014](https://experts.illinois.edu/en/publications/an-empirical-analysis-of-flaky-tests/), empirical analysis of 201 likely flakiness-fix commits across 51 projects | Classified causes including asynchronous waits, concurrency, and test-order dependency. | Older, broader software sample; neither Playwright-specific nor a comparison of AI and human authorship. These mechanisms can affect integration tests too. |
| [Diffie lab benchmark, April 2026](https://diffie.ai/blog/diffie-lab-benchmark-2026), vendor's own generation data | Reports 70.8% first-run passes among 387 tests with outcomes, from 893 total records; 490 records completed generation. Cleaned median generation time was about 3.6 minutes. | Missing outcomes and excluded runs over one hour constrain interpretation. First-run failure is not a flakiness rate, and passing does not demonstrate useful assertions or bug detection. |
| [Playwright best practices](https://playwright.dev/docs/best-practices), official guidance | Recommends isolated tests, controlled dependencies/data, stable locators, and retrying assertions. Advises against recording traces on every routine test because of overhead. | Supports concrete engineering changes to investigate; it does not quantify savings for this workflow. |
| [Wacker, Google Testing Blog, 2015](https://testing.googleblog.com/2015/04/just-say-no-to-more-end-to-end-tests.html), practitioner experience | Describes slow diagnosis and environmental coupling in E2E-heavy strategies; focused integration tests can expose interaction defects with less scope. | A composite experience report, not a controlled comparison or justification for a fixed test pyramid. E2E tests also catch real user-visible failures. |

**Evidence boundary:** no reliable universal cost multiplier or token-cost ranking was established for the requested workflows. Current evidence does not show that AI authorship itself causes flakiness. Documentation describes capabilities; benchmark success, coverage, and first-pass rate do not substitute for independently verified fault detection.

## 3. Comparison of testing approaches

| Approach | Valuable when | Cost and coverage tradeoff |
| --- | --- | --- |
| Unit tests | A rule or transformation can be exercised in isolation. | Small scope and focused diagnosis; mocked collaborators may hide boundary errors. |
| API/server integration tests with real dependencies | The requirement concerns permissions, persistence, transactions, serialization, or service interactions. | Can remove browser observation/setup from the check; database/services still cost setup and isolation effort. Does not verify UI wiring. |
| Component/browser tests | Rendering, local interactions, or browser APIs are the likely failure site. | Smaller application scope; mocked services do not prove full integration. |
| Saved Playwright E2E tests | A critical journey must work across UI, server, and persistence. | Reusable regression evidence without per-action inference; browser startup, environments, and maintenance remain. |
| Agent browsing via MCP or CLI | Discovering scenarios, inspecting unfamiliar UI, or investigating an unexplained failure. | Adaptive, but model calls, repeated observations, and variable paths add cost. A successful exploration is not automatically a reproducible regression test. |

**When integration may win:** the same requirement failure can be demonstrated at a real boundary without browser behavior being essential. For example, an API/database test can verify a forbidden write and unchanged persisted state; a UI journey is needed to verify the user sees the rejection correctly. Add a second test only for distinct assurance, not to satisfy a layer quota.

## 4. Main cost and reliability bottlenecks

| Bottleneck | What to investigate |
| --- | --- |
| Repeated observations | Full accessibility/DOM snapshots, screenshots, logs, and repeated history can inflate model input. Screenshots also carry image cost; a huge text tree can be more expensive than a focused image. Count provider usage rather than ranking modalities by intuition. |
| Interaction and debugging loops | Each inspect–act–wait–inspect cycle adds latency and context. Repeating setup, guessing selectors, or exploring after every passing test can dominate a short browser operation. Attribute calls to discovery, generation, execution supervision, and diagnosis. |
| Runtime infrastructure | Builds, server/database startup, browser installation, network dependencies, resource contention, traces, and browser matrices can dominate wall time without consuming many LLM tokens. Parallelism can reduce latency while increasing compute or contention. |
| Weak tests and repair churn | A test can pass while asserting only that a button exists. Repeated retries or agent repairs can conceal defects, weaken assertions, and consume more human review than initial generation. |

**Flakiness versus maintenance:** mixed pass/fail outcomes under equivalent code, data, and environment indicate nondeterminism. A selector that consistently breaks after a UI change is maintenance, not necessarily flakiness. A race exposed by a test may be a product bug. Classify these separately before blaming the framework or test author.

Plausible generated-test problems include fixed sleeps, missing awaits, ambiguous or positional selectors, shared accounts/data, reliance on test order, uncontrolled time/network responses, and assertions derived from buggy implementation behavior. These are mechanisms to audit, not measured prevalence claims about the developer's tests.

## 5. Potential optimizations

1. **Plan from requirements, execute with code.** Record scenario, requirement/version, preconditions, expected outcome, and likely failure boundary. Let the agent generate and review tests, then use a normal runner for recurring checks. Playwright's official [planner/generator/healer workflow](https://playwright.dev/docs/test-agents) demonstrates this separation; it does not prove that using all three agents saves cost. Treat healing as a reviewed change, especially when expected results change.
2. **Stabilize synchronization and locators.** Prefer scoped roles/labels or explicit test contracts; avoid DOM-position selectors unless intentional. Use condition-based, web-first assertions instead of fixed sleeps. [Auto-waiting](https://playwright.dev/docs/actionability) checks actionability, not every application-specific readiness condition; waiting for a durable result may still be necessary.
3. **Reuse isolated setup.** [Fixtures](https://playwright.dev/docs/test-fixtures) can encapsulate setup/teardown. Give tests deterministic seed data and unique resources, with cleanup and controlled clock/randomness where relevant. Browser-context isolation alone does not reset server state. [Authentication guidance](https://playwright.dev/docs/auth) warns against shared mutable accounts across parallel tests; reuse login state only when appropriate and retain dedicated authentication coverage.
4. **Use APIs for unrelated setup.** [Playwright API testing](https://playwright.dev/docs/api-testing) supports seeding state and checking server postconditions without clicking through the UI. Do not bypass the specific behavior being tested. Mock controlled third parties where useful, but record which real integration assurance the mock removes.
5. **Abstract repeated semantics selectively.** Small [page objects](https://playwright.dev/docs/pom) can centralize stable interactions and locators. Avoid a general framework that hides assertions or turns every change into shared-helper maintenance. GenE2E supports investigating good application-specific scaffolding, not mandatory abstraction for every test.
6. **Diagnose failures from artifacts.** Keep a compact runner result and inspect the failing assertion, relevant network/log evidence, and [trace](https://playwright.dev/docs/trace-viewer) before launching broad exploration. Capture expensive artifacts selectively. Preserve first-attempt outcomes: [retries](https://playwright.dev/docs/test-retries) can expose intermittent behavior but do not fix its cause.

Possible complements, only where a measured gap justifies them: existing native API/database tests; [Playwright component tests](https://playwright.dev/docs/test-components) or [Vitest Browser Mode](https://vitest.dev/guide/browser/) for isolated browser behavior; [Pact](https://docs.pact.io/) for verified consumer/provider message contracts. Contracts do not certify a complete journey. WebdriverIO or other browser interfaces merit a matched pilot only if harness overhead is the bottleneck; the benchmark above does not justify migrating a working suite.

Risk-based selection should follow consequences and failure modes: more boundary cases for authorization/data loss, browser coverage for browser-dependent behavior, and limited broad exploration for uncertain changes. A dependency-based selector can miss indirect effects; audit skipped cases. Avoid a blanket percentage allocation between unit, integration, and E2E tests.

## 6. Proposed experiments with measurable outcomes

Use [E5 and the shared measurement rules](README.md#five-experiments-to-prioritize). On recurring scenarios, compare current browsing with saved-test generation and repeated execution. Trial a real integration boundary only where it exercises the relevant historical defect; retain browser-specific assurance.

Separate discovery/generation, routine execution, diagnosis, and maintenance. Classify product defects, infrastructure failures, nondeterministic tests, and deterministic breakage after a change. Measure unique faults caught/missed, first-attempt outcomes, assertion preservation, and full human/machine cost. Do not infer useful coverage from pass rate or line coverage.

Only compare MCP versus interactive CLI observations if stage-level accounting identifies agent-interface overhead as material. Fixture, locator, and artifact changes are candidate responses to observed failures, not parallel experiment queues. Reuse the cost-accounting method in [communication research](agent-communication-efficiency.md).

## 7. Recommendations and open questions

**First candidate to investigate:** keep repeated checks in saved executable tests and reserve model-driven browsing for discovery and failure diagnosis. This avoids paying for an adaptive decision at every routine action, but its lifecycle advantage depends on reuse and maintenance. Measure before changing the workflow.

Prioritize isolation, strong spec-derived assertions, and failure classification before switching tools. Trial a targeted real-boundary integration test only where it catches a meaningful gap or demonstrates the same regression more cheaply. Preserve E2E journeys that provide distinct assurance about UI-to-server behavior. Do not purchase speed by weakening assertions, bypassing the behavior under test, or hiding failures behind retries.

The remaining decision is which measured bottleneck E5 should address first. Existing records should establish whether repeated interaction, runtime setup, or test repair dominates before introducing a tool or test layer.
