# Blog outline: How much should a coding agent say?

Outline prepared 25 October 2026. Ready for an experience-led draft; local efficiency claims still need measurements.

**Reader:** developers who spend time reviewing coding-agent reports and want less prose without losing the information needed to judge the work.

**Argument:** concise natural English is a practical way to reduce reading effort. Judge it by what the reader understands and can verify, alongside the full cost of completing the task. The research supports testing this approach; it does not establish that this particular policy works best.

**Suggested length:** 1,000–1,400 words. Use three or four studies in the main narrative; retain the others as supporting links.

Research and source limitations: [communication-efficiency note](../agent-communication-efficiency.md). This outline follows its source window: ordinary sources from 25 April 2026 onward; qualitative studies and research papers from 25 October 2025 onward. No local comparison has been run.

## 1. Open with the friction of reading an agent's work

Start with the reported experience: Caveman removed unwanted prose, but its compressed English became difficult to read and was abandoned. That leaves a useful question: what should an agent remove, and what does the reviewer still need?

**Add your material:** an actual report that took too long to read, the decision you needed to make, and the follow-up question it caused. Include the Caveman version only if you can recover it.

Keep this opening grounded in the experience. Save claims about cost and correctness for the research sections.

## 2. Fewer words and lower task cost are different measurements

Explain why narration is only part of an agent session. Code, tool calls, accumulated context, retries, and repair also affect the bill. A style change can reduce one component while the full task remains expensive.

Use [JetBrains' Caveman evaluation, 6 July 2026](https://blog.jetbrains.com/ai/2026/07/speak-to-ai-agents-like-cavemen-tosave-tokens/): 82 paired tasks produced 8.5% fewer output tokens. Raw cost was 11.6% higher in the skill arm because of one long-context outlier. The result illustrates cost variance; it does not show that Caveman generally increases cost. The non-significant task-score difference does not prove equivalent quality, and the study did not measure human comprehension.

Balance this with [CAVEWOMAN, 23 June 2026](https://arxiv.org/html/2606.24083v1), a preprint reporting roughly 1.4–2.4× lower realized cost on most API models under output compression. Its benchmarks use short structured answers, so those savings cannot be transferred to long coding sessions.

**Point to develop:** the workload and denominator determine what a savings percentage means.

## 3. A clear-sounding explanation can still leave the reader wrong

Use [Fernandes et al., *Explaining Too Much?*, 25 May 2026](https://arxiv.org/html/2605.25856v1). In this preregistered preprint with 559 participants, reasoning summaries increased trust without improving performance relative to answer-only responses on LSAT-style tasks.

Keep the limitation beside the finding: these were reasoning questions, not code reviews. The full-trace comparison also changed the model and answer presentation, so it cannot isolate the effect of length.

**Point to develop:** feeling informed is different from being able to detect an error or explain a limitation. A useful report gives the reader something concrete to check.

**Add your material:** a case where confident prose concealed an assumption, or where one precise limitation changed your decision. Use an actual example if available.

## 4. Finishing the task does not establish understanding of the code

Use [Balepur et al., *(Im)Paired Programming*, 29 July 2026](https://arxiv.org/html/2607.26375v1). In a preprint involving 54 CS students, agent users completed the initial website task more successfully but understood their code less well than users of a chatbot workflow. There was no significant overall advantage on a later extension task without agents.

The study changed the coding workflow, not just report style. It does not show that short reports cause poor understanding, and its student sample and website tasks limit generalization.

**Point to develop:** a communication policy needs to preserve enough understanding for the next review, repair, or change. Completion and comprehension deserve separate checks.

If the draft needs a parallel-work example, [ParallelPilot, 27 September 2026](https://arxiv.org/html/2609.33113v1) is optional supporting evidence. Its small study found higher throughput with a bundled supervision tool, but did not measure verification accuracy or implementation understanding. Avoid attributing the throughput gain to summaries alone.

## 5. Show what a useful short report contains

Introduce the communication policy as a working preference informed by this evidence. Focus on grammar, explicit relationships, relevant evidence, and visible limitations. Detail should grow when a consequential tradeoff needs explanation.

Build a before-and-after example around the same facts:

- What changed and why it matters.
- What was checked and what the check showed.
- What remains uncertain or requires a decision.

**Writing prompt:** take the real report from the opening and rewrite it. Remove introductions, repetition, and narration. Keep the triggering condition, consequence, verification result, and material gap. Let the reader compare the versions.

If a real example is unavailable, label the following as fictional:

> Expired sessions are now rejected before workspace data is read. Unit tests passed. API integration tests were not run, so rejection at the API boundary remains unverified.

This is a format illustration, not a report of completed work.

Optional research link: [Li et al., *Thinking Is Not Telling*](https://arxiv.org/html/2602.07796v2), first released 8 February 2026 and revised 8 August 2026. Supplying missing decision-relevant information helped more than verbosity controls in selected simulated service interactions. This suggests checking whether the report supplies necessary constraints and consequences; transfer to human coding supervision remains untested.

## 6. End with the comparison worth running

Describe the next test as a proposal. Keep the model, reviewer, evidence, and reasoning effort fixed while comparing current reports with concise natural-English reports.

Measure reading and follow-up time together with whether the reader can identify changed behavior, failure conditions, verification results, and unresolved limitations. Record consequential omissions, repair, and total execution cost. Ask about preference separately.

**Add your position:** explain which facts a report must preserve before you would accept a time saving. If you later run E1, replace this proposed-test ending with the actual results and limitations.

Finish on that concrete next step. The post can make a defensible argument for the policy without claiming a measured local improvement.

## Drafting notes

- Keep personal observations, published findings, and proposed practice distinguishable in the prose.
- Identify preprints when introducing them. Keep material study limitations near the claim they qualify.
- The source review was targeted, not exhaustive, and results were not independently reproduced. The research note contains the broader evidence assessment.
- If published as Code Villain, follow [the studio voice](../../studio/brand/voice.md): Code Villain speaks in the third person, claims point to inspectable work, and the piece gets at most one flourish.
- A publish-ready draft belongs in `studio/content/blog/`; this file remains an outline.
