# The Expertise–Trust Paradox in AI-Assisted Development

Research note · 24 October 2026 · Research only; no workflow changes proposed for immediate adoption.

**Roadmap:** [E1](README.md#five-experiments-to-prioritize) combines decision quality, concise reports, and code-review allocation. This note owns decision risks and brief content; communication style and full cost accounting live in the communication note. Architectural discovery belongs to E2.

## 1. Problem statement

Human approval may provide less protection against incorrect technical advice when the approver lacks the expertise to challenge its assumptions. In spec-driven development, an architectural error can propagate through the specification, plan, implementation, and tests before a separate model reviews the code.

**Core hypothesis:** limited expertise, time pressure, and costly explanations can make critical evaluation harder. Can a brief reduce the information a person must process while preserving decision quality? “Expertise–trust paradox” is a working title, not an established construct. Greater reliance can also be rational when the model genuinely performs better.

## 2. Current workflow and observations

The developer reports that Opus 5.1 High produces specifications, architecture proposals, and plans, normally offering 3–4 options and recommending one. Decisions mostly concern technical tradeoffs. Estimated selections are: recommended option **75%**, another supplied option **20%**, and a developer-proposed solution **5%**. Trust increases in unfamiliar areas. Sol High independently reviews implementation, potentially after substantial architectural commitment.

The developer would ideally request explanations until the alternatives are understood, but reports accepting recommendations without fully evaluating tradeoffs when under time pressure or faced with walls of text.

These are **personal observations and subjective estimates**, not measurements or evidence of bias. Acceptance rate alone says nothing about decision quality; familiarity, difficulty, model competence, and verification cost may all influence reliance.

## 3. Relevant research and evidence

| Question | Evidence | Applicability and limits |
| --- | --- | --- |
| Can automation induce errors? | Participants followed wrong automated advice and missed unflagged events in a flight simulation. [Skitka et al., 1999](https://doi.org/10.1006/ijhc.1999.0252). | Demonstrates mechanisms, not software error rates. |
| Does expertise protect reviewers? | Less task-expert physicians were particularly susceptible to advice, but experts also accepted inaccurate advice. [Gaube et al., 2021](https://www.nature.com/articles/s41746-021-00385-9). Experienced forecasters' lower reliance on algorithmic advice harmed accuracy in another experiment. [Logg et al., 2019](https://doi.org/10.1016/j.obhdp.2018.12.005). | Expertise is not immunity; less reliance is not necessarily better. These tasks differ from architecture selection. |
| Do explanations reliably improve judgment? | Explanations increased acceptance of both correct and incorrect recommendations without improving complementary performance in [Bansal et al., 2021](https://arxiv.org/abs/2006.14779). Conversely, five maze studies (N=731) found verification costs and benefits affected overreliance. [Vasconcelos et al., 2023](https://arxiv.org/abs/2212.06823). | Explanation content and verification effort matter. Neither directly compares concise architecture briefs with long prose. |
| Is omitting detail safer? | Two studies using graph paths (N=264) and text correction (N=210) found partial explanations reduced overreliance relative to baseline but were less effective than full explanations. [de Jong et al., 2025](https://doi.org/10.1145/3710946). | Counters “less text is always better.” Partial reasoning is not equivalent to a concise, complete decision brief. |
| What changes under time pressure? | Controlled logic-puzzle studies found time pressure changed accuracy–time tradeoffs across assistance types and user reliance patterns. [Swaroop et al., 2024](https://kgajos.seas.harvard.edu/papers/swaroop2024accuracy.pdf). Cognitive forcing reduced overreliance but was less liked in [Buçinca et al., 2021](https://doi.org/10.1145/3449287). | Pressure does not justify assuming uniformly higher trust. Additional deliberation can consume the time it is meant to save; expertise interactions require local testing. |
| Can counterevidence help? | A hypothesis-driven interface presenting supporting and opposing evidence improved decision accuracy and reduced overreliance, with some increased underreliance versus a recommendation-driven interface. [Le et al., 2024](https://doi.org/10.3233/FAIA240571). | Promising adjacent evidence, not a trial of one recommendation plus its strongest objection versus several technical alternatives. |
| Does human approval guarantee improvement? | Across 106 studies, human–AI combinations improved on humans alone on average but underperformed the better of human or AI alone. [Vaccaro et al., 2024](https://www.nature.com/articles/s41562-024-02024-1). | Heterogeneous tasks; not direct architecture evidence. |
| Do additional models improve correctness? | Debate improved selected tasks in [Du et al., 2023 preprint](https://arxiv.org/abs/2305.14325), but did not reliably beat simpler strategies in [Smit et al., 2024](https://proceedings.mlr.press/v235/smit24a.html). | Different model names do not establish independent errors or architecture-review benefits. |
| Is confidence reliable? | One coding study found less secure code and greater perceived security with assistance; another found a small security impact in its C task. [Perry et al., 2023](https://arxiv.org/abs/2211.03622), [Sandoval et al., 2023](https://www.usenix.org/conference/usenixsecurity23/presentation/sandoval). | Older assistants and different tasks limit generalization. |

**Evidence boundary:** no study identified here establishes the optimal length, minimum sufficient content, or best option count for AI architecture briefs. Concision, structure, omitted information, and expertise must be separated experimentally. Risk-based explanations and details on request are plausible designs, not demonstrated safeguards. This is a targeted review, not a systematic review.

## 4. Potential failure modes

The following are **plausible workflow hypotheses**, not measured findings here:

- **Restricted choice:** all proposed options share an incorrect premise or omit a viable approach; choosing among them preserves the error.
- **Anchored approval:** recommendation, option order, or polished prose shapes selection before independent evaluation.
- **Shared assumptions:** reviewers inherit the author's rationale, incomplete spec, or common misconceptions and agree without fresh evidence.
- **Consistent implementation of a bad decision:** passing spec-derived tests verifies conformance while missing a flawed requirement or architectural constraint.
- **Review too late:** useful criticism arrives after expensive implementation, making revision harder.
- **Review noise:** plausible but invalid objections consume scarce human attention or push a sound design toward unnecessary complexity.
- **Lossy compression:** a brief hides a decisive constraint or presents a weak objection as the strongest one; increased fluency is mistaken for understanding. Optional details may never be requested, especially by someone unaware of a knowledge gap.

## 5. Possible mitigation strategies

These are candidates to evaluate, not instructions to change the current workflow.

### Minimum useful content: a proposal to test

Borrow context, decision, and consequences from [Nygard's architecture decision records](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions), a practical approach rather than a controlled validation study. Every candidate brief should expose:

- **Decision and reason:** recommended approach, requirement/constraint served, and decisive tradeoff.
- **Real alternatives:** meaningfully different choices, including the status quo where viable, and when each wins.
- **Strongest objection:** a concrete failure scenario or reason to prefer an alternative.
- **Risks and assumptions:** what must be true, evidence or an explicit “unverified,” and what would overturn the recommendation.
- **Impact and reversibility:** affected systems/data, cost of being wrong, rollback or migration effort.
- **More detail:** direct links or specific follow-up questions for evidence and examples.

This is a candidate information floor, not a guarantee of responsible approval. Unknown assumptions or poorly understood irreversible consequences can make approval premature. Keep decisive risks and uncertainty visible; place background and supporting derivations on request. Use plain language and a concrete failure example rather than packing more jargon into fewer words.

### Three lightweight formats

All retain the content above; word budgets are trial parameters, not evidence-based limits.

| Format | Shape | Potential benefit and risk |
| --- | --- | --- |
| A. Recommendation and challenge | Six labeled lines following the content above; roughly 120–180 words. Name the main alternative and the strongest reason to reject the recommendation. | Fast orientation; the author can still choose weak counterarguments or omit viable options. |
| B. Compact comparison | A 2–3-option table comparing the same decisive constraints, then the recommendation, objection, unverified assumption, impact/rollback, and evidence links; roughly 180–250 words. | Makes tradeoffs comparable; extra rows may distract, and an incomplete table can look exhaustive. |
| C. Impact first, detail on request | Lead with consequences, reversibility, and unresolved assumptions; then give the recommendation, main alternative, objection, and links. Expand the brief when a security boundary, migration, cross-service contract, or unfamiliar assumption is decisive. | Directs attention toward costly mistakes; a wrong risk classification can suppress needed explanation. Audit skipped detail. |

One recommendation plus counterargument could reduce reading, but may anchor more strongly than a neutral comparison. There is no direct evidence here that it is better. Test both using the same underlying evidence. A counterargument generated by the same model is a prompt to check, not independent validation.

For consequential cases, test one reviewer assessing constraints and alternatives before seeing the author's preference. Require concrete evidence, not agreement. Extra review is worthwhile only if expected avoided loss exceeds token cost, delay, human effort, and harm from false objections. Neither added models nor longer explanations guarantee that tradeoff.

## 6. Recommended experiments

Follow [E1 and the shared measurement rules](README.md#five-experiments-to-prioritize). Initially compare current reports with one concise format while keeping Sol High and the evidence fixed. Include sound and flawed decisions, record domain familiarity, and assess actual comprehension and consequential misses; acceptance rate or confidence alone is insufficient.

Only after that comparison, hold the report format fixed when comparing reviewer allocation. Treat the other brief formats, delayed recommendations, and pressure manipulations in this note as deferred options, not additional experiments. Use E2 for fresh-agent architectural discovery.

## 7. Remaining decision

The unresolved local question is whether less review material saves human time without obscuring the assumptions needed for a sound decision. E1 measures this; unfamiliarity is a recorded context variable, not proof of biased approval.

## 8. Sources

Primary studies, an original meta-analysis, and explicitly identified practitioner guidance; dates identify publications, not current model capabilities.

1. Skitka, Mosier & Burdick (1999). [Does automation bias decision-making?](https://doi.org/10.1006/ijhc.1999.0252) *International Journal of Human-Computer Studies*.
2. Gaube et al. (2021). [Do as AI say: susceptibility in deployment of clinical decision-aids](https://www.nature.com/articles/s41746-021-00385-9). *npj Digital Medicine*.
3. Logg, Minson & Moore (2019). [Algorithm appreciation: People prefer algorithmic to human judgment](https://doi.org/10.1016/j.obhdp.2018.12.005). *Organizational Behavior and Human Decision Processes*.
4. Buçinca, Malaya & Gajos (2021). [To Trust or to Think: Cognitive Forcing Functions Can Reduce Overreliance on AI in AI-assisted Decision-making](https://www.eecs.harvard.edu/~kgajos/papers/2021/bucinca21trust.pdf). *PACM HCI / CSCW*, DOI 10.1145/3449287.
5. Vaccaro, Almaatouq & Malone (2024). [When combinations of humans and AI are useful: A systematic review and meta-analysis](https://www.nature.com/articles/s41562-024-02024-1). *Nature Human Behaviour*.
6. Du et al. (2023 preprint). [Improving Factuality and Reasoning in Language Models through Multiagent Debate](https://arxiv.org/abs/2305.14325).
7. Smit et al. (2024). [Should we be going MAD? A Look at Multi-Agent Debate Strategies for LLMs](https://proceedings.mlr.press/v235/smit24a.html). *ICML / PMLR*.
8. Perry et al. (2023). [Do Users Write More Insecure Code with AI Assistants?](https://arxiv.org/abs/2211.03622) *ACM CCS*, DOI 10.1145/3576915.3623157.
9. Sandoval et al. (2023). [Lost at C: A User Study on the Security Implications of Large Language Model Code Assistants](https://www.usenix.org/conference/usenixsecurity23/presentation/sandoval). *USENIX Security*.
10. Bansal et al. (2021). [Does the Whole Exceed its Parts? The Effect of AI Explanations on Complementary Team Performance](https://arxiv.org/abs/2006.14779). *CHI*.
11. Vasconcelos et al. (2023). [Explanations Can Reduce Overreliance on AI Systems During Decision-Making](https://arxiv.org/abs/2212.06823). *PACM HCI / CSCW*.
12. de Jong, Paananen, Tag & van Berkel (2025). [Cognitive Forcing for Better Decision-Making: Reducing Overreliance on AI Systems Through Partial Explanations](https://doi.org/10.1145/3710946). *PACM HCI / CSCW*. The comparison summarized here is from the publisher's abstract.
13. Swaroop, Buçinca, Gajos & Doshi-Velez (2024). [Accuracy-Time Tradeoffs in AI-Assisted Decision Making under Time Pressure](https://kgajos.seas.harvard.edu/papers/swaroop2024accuracy.pdf). *IUI*, DOI 10.1145/3640543.3645206.
14. Le, Miller, Sonenberg & Singh (2024). [Towards the New XAI: A Hypothesis-Driven Approach to Decision Support Using Evidence](https://doi.org/10.3233/FAIA240571). *ECAI*.
15. Nygard (2011). [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions). Practitioner guidance and experience report, not an experiment.
