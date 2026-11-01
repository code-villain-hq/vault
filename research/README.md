# Research roadmap

Updated 25 October 2026. The goal is to reduce human supervision, total AI cost, and time to accepted software without reducing correctness. This index sets priorities; linked notes hold supporting evidence. No experiments or workflow changes have been made.

Internet research uses sources published within six months, extended to one year for qualitative studies and research papers. The [communication note](agent-communication-efficiency.md#scope-and-source-rules) was screened on 25 October 2026: ordinary sources must date from 25 April 2026; studies and papers from 25 October 2025. Other linked notes have not yet been screened against this rule.

## Evidence and baseline

**O** means personal observation or a reported capability. **E** means external evidence limited to the cited study's conditions. **H** means an untested hypothesis. There are no controlled local results for these proposals. Neither personal observations nor external benchmarks establish a local optimum.

The working baseline is reported and has not been independently re-audited:

- Opus 5.1 with 512K context and earlier compaction worked well. 128K was insufficient; 256K is untested.
- Opus High handles specs and architecture; Medium/Low implements; Sol High reviews independently. Human-reviewed specs become tasks and verification criteria. This allocation is a reference condition, not a proven optimum.
- Knip, import/domain rules, TypeScript configuration, and scripts enforce structural constraints. Forge incrementally checks code against specs and uses existing PM/PO triage.
- Repository knowledge and task boundaries support harness switching. Layer handles context, integrations, orchestration, and tool visibility. Its workspaces and GitHub PRs support independent parallel work.

## Topics and hypotheses

| Priority | Topic and evidence | Hypothesis to test | Plan |
| --- | --- | --- | --- |
| Very high | Verification and supervision. **O:** verbose reports and speculative findings consume attention. **E:** reasoning summaries can raise trust without improving performance; coding throughput and understanding can diverge. | Evidence-linked, concise reviews reduce adjudication and rework without missing consequential findings. | E1; [communication](agent-communication-efficiency.md), [decision quality](expertise-trust-paradox.md). |
| Very high | Architectural overengineering. **O:** complexity is often relocated; the developer supplies the simpler design. **E:** discovery can lag execution, and repairs can introduce new smells. | A reviewer who has not seen the proposed solution can find a genuinely cheaper design. | E2; [architecture](preventing-architectural-overengineering.md). |
| High | Context and compaction. **O:** 512K with earlier compaction helped; 128K did not. No controlled local comparison exists. | 256K lowers total cost without increasing forgotten constraints, rereading, or repair. | E3 below; no separate note yet. |
| High | Model and effort allocation. **O:** phase-specific allocation works in current use. Existing sources do not establish its optimum. | Lower implementation effort preserves accepted-task quality at lower total cost. | E4 below; initially retain current spec and review allocation. |
| High | Independent cross-model review. **O:** Sol High finds bugs. **E:** multi-model results depend on the task; agreement is not validation. | Useful discoveries justify the cost versus an equal-budget self-review; reviewer freshness may matter more than model family. | Compare reviewers in E1; E2 tests architectural discovery. [Review evidence](expertise-trust-paradox.md). |
| High | Decision quality and information density. **O:** unfamiliarity, pressure, and long prose increase reliance. **E:** disclosing missing decision-relevant information outperformed added verbosity in simulated service tasks; transfer to coding supervision is untested. | Short, natural English preserves comprehension and important risks while reducing review time. | E1; [communication evidence](agent-communication-efficiency.md#human-understanding-and-disclosure), decision briefs from [the trust note](expertise-trust-paradox.md). |
| Medium–high | Testing efficiency. **O:** browser testing appears costly and flaky. **E:** agent browsing, saved tests, and integration tests have different costs and coverage. | Reusing executable checks or testing a real boundary reduces lifecycle cost for selected scenarios. | E5; [testing](ai-driven-testing-efficiency.md). |

## Experiments to prioritize

These ratings are planning judgments. Reuse completed tasks, review artifacts, and available usage records. Establish a baseline, then vary one factor at a time. No new evaluation platform is needed.

| Order | Experiment | Expected impact | Effort | Measurability |
| --- | --- | --- | --- | --- |
| 1 | E1: evidence-based verification | Very high | Low–medium | High for time and false findings; medium for misses |
| 2 | E2: architectural discovery | Very high | Medium | Medium for design judgment and later change effort |
| 3 | E3: 256K versus 512K context | High potential | Low–medium | High for cost; medium for retention and regressions |
| 4 | E4: implementation effort | High potential | Low–medium | High for cost and retries; medium for rework |
| 5 | E5: testing lifecycle cost | Medium–high | Medium | High for cost; medium for fault detection |

### E1 — Review content, then reviewer allocation

First, keep Sol High and evidence fixed. Compare current reports with concise findings that state the trigger, consequence, evidence, and uncertainty. Include decisions, sound cases, and flaws. Measure review and follow-up time, valid findings, speculative objections, comprehension, and consequential misses.

Then keep the format fixed and compare current cross-model review with an equal-budget review by the author model. Use an independent reference assessment.

### E2 — Discover without the answer

Compare author-context, fresh same-model, and fresh cross-model reviews with matched specs, evidence, and budgets. Hide the preferred solution and record proposals before implementation. Assess valid simpler designs, justified decisions to make no change, preserved behavior, and later change effort. Defer deletion cues. The [architecture note](preventing-architectural-overengineering.md) contains the detailed protocol.

### E3 — Context retention and cost

Replay matched long tasks at the observed 512K/earlier-compaction setting and at 256K, with repository knowledge and summary requirements fixed. Record actual context use and exact compaction triggers. Changing both the cap and timing is a combined intervention. Measure total usage, forgotten constraints, rereads, corrections, regressions, and acceptance time. Do not retest 128K without a new reason.

### E4 — Implementation effort

Compare Low and Medium on the same model and version with matched tasks. Fix specs, context, checks, and Sol High review. Count failures and fixes, and measure total spend and elapsed and human time through acceptance. Expand to other phases only if baseline spend justifies it; effort labels do not prove capability equivalence.

### E5 — Reuse and useful boundaries

Compare current browsing with saved Playwright tests on recurring scenarios and historical defects. Try API/database integration only when it exercises the relevant failure. Separate test generation, repetition, diagnosis, and maintenance. Measure unique faults caught and missed, mixed outcomes, assertion preservation, repair time, and total cost. Keep browser-specific assurance.

## Measurements and decision gates

- Define acceptance from reviewed requirements, relevant checks, and independent assessment. Preserve security and failure contracts. Model agreement and passing tests alone are insufficient.
- Record task, spec, repository, model, and harness versions; settings; human and elapsed time; and usage, including retries. Mark missing usage unavailable. Separate subscription charges from estimated API spend.
- Include setup, review, false-positive adjudication, and rework. Use a fixed regression follow-up window and report important misses separately from averages.
- Set useful improvement and quality margins before comparisons. Small pilots cannot establish equivalence. Continue only when net benefit is plausible; keep the baseline when results are inconclusive.

Start with E1 and E2. Run E3 and E4 once cost attribution is usable. Move E5 earlier only if recorded testing time or cost dominates.

## Consolidated and deferred work

| Work | Decision and reason | Reopen when |
| --- | --- | --- |
| Decision briefs, readable concision, less review noise, and attention-cost measurement | Consolidated in E1. The communication note covers style and cost evidence; the trust note covers decision risks. | E1 leaves a specific format question unanswered. |
| Fresh-agent versus cross-model architectural review | Use one E2 protocol. Ordinary code-review allocation stays in E1. | Discovery results justify another variant. |
| Rebuilding incremental drift detection or PM/PO triage | Forge already handles these. The [drift note](forge-incremental-drift-checks.md) is background, not an implementation backlog. | Existing checks show a measured coverage or cost problem. |
| Dependency-graph requirement mapping | [Parked hypothesis](forge-dependency-graphs.md); maintenance cost and false-negative risk lack demonstrated local benefit. | Measured drift-selection failures justify a bounded trial. |
| New orchestration, memory, integration/tool-visibility, or parallel-work systems | Layer, repository knowledge, workspaces, and PRs already address these needs. | A concrete failure is reproducible with the existing tools. |
| Global token minimization, exhaustive model tournaments, or additional review agents | Readability has been a problem; visible token count and agent count are weak standalone objectives. | End-to-end measurements show a specific opportunity. |
| More test layers, generic architecture scores, or wholesale framework rewrites | No demonstrated return; they risk added maintenance and supervision. | Fault replay or change-cost evidence identifies a gap. |
| Blog expansion | [Editorial seeds](blog-posts/README.md) remain separate from experiment priority. | Local measurements support a defensible story. |

Limitations in linked notes are background, not additional queued experiments. Forge specs and the normal spec-change process remain authoritative. This roadmap authorizes no implementation.
