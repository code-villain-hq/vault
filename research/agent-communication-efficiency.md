# Communication efficiency in AI coding agents

Research updated 25 October 2026. Internet evidence review completed; no local experiments have been run.

**Assessment:** concise natural English remains a reasonable candidate for E1. Recent evidence supports measuring understanding, verification, and full task cost separately. It does not establish an optimal response length or show that compressed grammar improves coding supervision.

## Scope and source rules

This pass covers agent reports, explanation length, human understanding, and communication cost. [E1](README.md#experiments-to-prioritize) owns the local experiment; [the trust note](expertise-trust-paradox.md) covers decision briefs.

Sources must meet these publication windows, measured from 25 October 2026:

- Articles, engineering reports, and other non-paper sources: 25 April–25 October 2026.
- Qualitative studies and research papers, including identified preprints: 25 October 2025–25 October 2026.

Use publication or first public release dates, not crawl dates. Record revisions separately; a new revision does not automatically make old research eligible. Exclude undated material unless a dated version can be established.

Searches covered Caveman evaluations, verbosity benchmarks, explanation visibility, agent disclosure, code comprehension, and supervision. Nine primary sources are assessed below. Full-text pages or PDFs were consulted for eight, focusing on methods, results, and limitations. Zhu and Li was available only through the publisher's abstract, introduction, and section summaries. This is a targeted review, not an exhaustive systematic review. Reported results were not independently reproduced.

The previous readability table has been removed from the evidence base: Morkes and Nielsen (1997), Vasconcelos et al. (2023), de Jong et al. (2 May 2025), and Poddar et al. (July 2025) are outside the one-year window. Undated provider documentation and mutable skill pages are not used as research evidence.

## Goal and local observations

Reduce reading and clarification effort while preserving the information needed to judge a technical result: evidence, risks, assumptions, tradeoffs, and required decisions.

The developer reports that Caveman reduced unwanted prose, but its compressed English became difficult to read and was abandoned. Introductions, repetition, unnecessary explanations, and conclusions add supervision effort. These observations have not been tested in a controlled comparison; the Caveman version used is unknown.

A candidate local measure is **relevant facts correctly understood per minute**, with consequential omissions reported separately. This is a proposed operational measure, not an established standard. Also record task quality, latency, and total human and execution costs.

## Human understanding and disclosure

| Source, date, and status | Finding | Limits and implication for E1 |
| --- | --- | --- |
| [Fernandes et al., *Explaining Too Much?*](https://arxiv.org/html/2605.25856v1), 25 May 2026; preregistered preprint, N=559 | On ten LSAT-style problems, answer-only and summary-trace groups scored 6.18 and 6.11 out of 10; the difference was not significant. Summaries increased trust without improving performance. No format produced calibrated self-assessment. | Full traces scored worse, but that comparison changed both the model and answer presentation. It cannot isolate length. These were reasoning tasks, not code review. Measure comprehension independently of trust or preference. |
| [Zhu and Li, *Over-reliance on AI in information tasks*](https://www.sciencedirect.com/science/article/pii/S0736585326001000), October 2026 issue; journal paper, three experiments, total N=657 | The authors report that visible reasoning and task complexity each increased acceptance of incorrect AI advice across health, environment, and technology tasks. | Only publisher summaries and introductory material were accessible; full methods and effect sizes were not inspected. This is supporting evidence about explanation visibility, not a direct test of concise coding reports. |
| [Balepur et al., *(Im)Paired Programming*](https://arxiv.org/html/2607.26375v1), 29 July 2026; preprint, N=54 CS students | Agents improved initial website completion but reduced code comprehension compared with a chatbot workflow. Agent use did not produce a significant overall advantage on a later extension task without agents. | Two website tasks and a student sample limit transfer to experienced developers. The interface changed how code was produced, not just report style. Associations between interaction habits and comprehension are exploratory. Task completion alone is insufficient for E1. |
| [Long et al., *ParallelPilot*](https://arxiv.org/html/2609.33113v1), 27 September 2026; preprint, formative N=14 and evaluation N=16 | The integrated planning and monitoring tool increased throughput by 63% in short coding tasks and reduced tracking effort. Perceived control and success in redirecting agents did not significantly improve. | Twenty-minute blocks, six seed tasks, and a bundled intervention. Systematic code review was not required; verification accuracy and implementation understanding were not measured. This supports studying supervision effort, not attributing gains to concise summaries or adding a new orchestration system. |
| [Li et al., *Thinking Is Not Telling*](https://arxiv.org/html/2602.07796v2), first released 8 February 2026, revised 8 August 2026; preprint | Across five models and three service scenarios, more internal thinking did not reliably produce better disclosure. Counterfactual interventions that supplied missing decision-relevant information improved the first environment edit more than verbosity controls. | The focused intervention used two models in retail and airline environments with a simulated user and selected failed trajectories. It does not establish human comprehension or end-to-end coding gains. Audit whether a report supplies the constraints and consequences needed for the next decision. |

**Inference:** readability, confidence, task completion, and understanding can diverge. E1 should test whether a reader can identify the decisive fact, explain its consequence, and locate the supporting evidence. Asking whether a report “felt clear” is insufficient.

## Output length and cost

| Source, date, and status | Finding | Limits for this workflow |
| --- | --- | --- |
| [Borisov et al., *YapBench*](https://arxiv.org/html/2601.00624v1), 2 January 2026; preprint | The benchmark evaluates 76 models on 304 English prompts designed to admit short answers. It measures excess visible characters against curated minimal answers and finds substantial model and task variation. | The main results have no explicit correctness gate. Tasks are simple, single-turn requests, not consequential technical reviews. Excess length is a style diagnostic, not a quality or comprehension score. |
| [Zahedzadeh and Bahrak, *The Sufficiency-Conciseness Trade-off*](https://aclanthology.org/2026.lrec-1.167.pdf), LREC, 11–16 May 2026; conference paper | Seven models were evaluated on English and translated Persian ARC-Challenge questions. Some shortening retained answer accuracy, while stronger compression reduced accuracy and explanation sufficiency. | Sufficiency was assessed through model-based procedures, not human review. Results depend on model and language. Model-size claims were not independently verified; no scale inference is used here. The paper offers no transferable safe compression ratio. |
| [Adeyemi, Rossi, and Dernoncourt, *CAVEWOMAN*](https://arxiv.org/html/2606.24083v1), 23 June 2026; preprint | On eight models and five datasets, output compression reduced realized cost by roughly 1.4–2.4× on most API models. Input compression instead increased average cost as models generated longer answers. | All five benchmarks use short structured answers; there is no human readability test. Semantic checks measure surface-text divergence, not proven loss of propositions. Greedy decoding and the small vendor panel limit generalization. Do not transfer these savings to long coding sessions. |
| [Shiryaev, JetBrains Caveman benchmark](https://blog.jetbrains.com/ai/2026/07/speak-to-ai-agents-like-cavemen-tosave-tokens/), 6 July 2026; vendor engineering report | The full run of 82 paired agent tasks reported 8.5% fewer output tokens. Scores were 0.326 versus 0.311, with no significant difference (p=0.82). Raw cost was 11.6% higher in the skill arm: USD 40.60 versus USD 36.39, driven by a long-context outlier. | One Claude Code/model setup, forced activation, no concise-natural-English control, and no human comprehension test. Non-significance does not prove equivalent quality. The claimed savings ceiling is not established for other workloads. The publication date was checked in the article's HTML time element. |

These studies use different workloads and outcomes. Their savings cannot be pooled into an expected local percentage. None directly compares this developer's policy with Caveman while measuring review time, comprehension, consequential omissions, and full coding-task cost.

### Local cost accounting

For each completed task, record provider-reported usage and actual charges across all calls and retries. Sum input charges by cache category, billed output charges, and tool, compute, or service charges. Follow the provider's accounting definitions so that included reasoning or cache usage is not counted twice. Mark unavailable usage explicitly.

Estimate visible narration separately. Also record human review, clarification, false-positive adjudication, and repair time. Use measured charges for API runs; do not translate estimates automatically into subscription or energy savings.

**Illustration, not a measurement:** if narration accounts for 5% of task spend, halving it saves at most 2.5% with everything else fixed. Additional instructions, clarification, retries, and changed history or caching can alter the result.

## Proposed E1 comparison

Keep the reviewer, model version, reasoning effort, underlying evidence, and task requirements fixed. Compare current reports with concise natural-English reports using the [shared measurement rules](README.md#measurements-and-decision-gates).

- Establish decisive facts and material risks from independent review of the task evidence before scoring reports. Include correct results, real flaws, and justified decisions to make no change.
- Measure reading and follow-up time, comprehension, evidence retrieval, valid findings, speculative objections, consequential omissions, and rework. Record confidence and preference separately from correctness.
- Test understanding with questions about the changed behavior, failure conditions, verification performed, and unresolved limitations. Score the reader's answers, not the report's apparent clarity.
- Use matched tasks or counterbalanced report assignments. Avoid showing the same reviewer both versions of one case when memory would make the second easier.
- Set useful time savings and acceptable quality margins in advance. Report important misses individually; a small pilot cannot establish equivalence.
- Include all execution and human costs. Context and reasoning changes belong to E3 and E4 and should remain fixed during this style comparison.

Keep the existing reviewer-allocation comparison after the format comparison. Defer additional style variants until E1 identifies a specific gap.

## Policy candidates and tradeoffs

These are design proposals informed by the review, not validated interventions.

| Approach | Potential value | Main risk |
| --- | --- | --- |
| Concise natural English | Removes repetition while retaining familiar grammar and explicit relationships. | A vague concision instruction may cause the model to omit evidence or qualifications. |
| Optional task templates | Makes findings, checks, and limitations easier to locate. | Required sections create filler on simple tasks. |
| Progressive disclosure | Keeps the result visible and links supporting detail. | Readers may not seek information they do not know is missing. Keep material risks and assumptions in the response. |
| Detail matched to impact and familiarity | Adds explanation where the reader needs it to assess a consequential decision. | The agent may misjudge risk or the reader's knowledge. Audit apparently routine cases. |
| Caveman-style compression | Reduces prose in some evaluated settings. | Reconstruction effort, ambiguity, and lost relationships may outweigh savings. |
| Hard output caps | Limits generated output under a configured API. | Truncation can remove the result or a critical qualification; check the API's token accounting before use. |

Useful optional fields:

- **Implementation:** result and reason; verification and results; material limitation or user action; file links.
- **Review:** finding and location; trigger; consequence; evidence and severity. With no findings, state scope and residual uncertainty briefly.
- **Decision:** recommendation and decisive reason; viable alternative and strongest objection; assumptions and evidence; impact and reversibility.

A small default policy can live in global or repository instructions. Harness settings can adjust available verbosity controls; a Skill can supply optional examples and templates. These mechanisms add context and differ in portability and precedence. Verify loading in each harness before attributing a result to the policy.

### Draft policy for `AGENTS.md`

Draft only; it has not been installed or added to an instruction file.

```markdown
## Communication

- Write concise, natural English for a technically competent reader. Preserve
  grammar and explicit relationships between facts. Use standard technical terms.
- Include facts that change the reader's understanding or next action: relevant
  evidence, assumptions, uncertainty, risks, tradeoffs, and verification results.
- Match detail to complexity, unfamiliarity, impact, and reversibility. Explain a
  consequential tradeoff even when that requires more space.
- Use short paragraphs, lists, or comparison tables when they make review easier.
  Use task-specific fields only when they carry useful information.
- State what was actually checked and any material verification gap. Preserve
  exact commands, identifiers, paths, errors, numbers, units, and negations.
- Keep critical risks and unresolved assumptions in the response. Link supporting
  detail for optional reading; do not substitute a link for a necessary warning.
- Keep progress updates to meaningful findings, blockers, or changes of plan.
  Honor requested detail and applicable reporting requirements.

### Avoid Redundancy

- Do not repeat information already established in the conversation.
- Skip recaps, transitions, and introductions unless necessary for clarity.
- Start directly with new information.
- Do not restate the user's question or summarize previous discussions.
- Every sentence should add information or serve a clear purpose.
- Remove sentences that can be deleted without losing meaning.
- Preserve necessary context, warnings, and technical details.
```

## Decision gate

Adopt a communication change only if E1 shows useful review-time savings while preserving comprehension and visibility of material risks. The current evidence supports running that comparison; it does not validate the draft policy or justify a universal length limit.
