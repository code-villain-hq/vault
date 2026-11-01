# Preventing Architectural Overengineering in AI-Generated Code

Research note · 24 October 2026 · Research only; no code, architecture, or instruction changes.

**Roadmap:** [E2](README.md#five-experiments-to-prioritize) owns the architectural-discovery pilot. This note retains its detailed design and evidence. Prevention-versus-refactoring, deletion cues, and new guardrail trials are deferred variants, not separate queued work.

## 1. Problem statement and observations

The developer reports overloaded functions/classes, speculative abstractions, excessive fallback logic, repeated validation, unnecessarily permissive TypeScript types, and simplifications that move complexity elsewhere. Local cleanup sometimes succeeds after one or two attempts; architectural simplification often requires the developer to supply the design. These are personal observations, not measured model characteristics.

**Hypothesis:** coding agents can improve a local metric or file while preserving or increasing the complexity of the surrounding system. The target is lower cost of understanding, verifying, and changing the required behavior, subject to correctness, reliability, security, and performance constraints. Fewer lines, classes, or layers alone do not establish improvement.

Related notes cover [independent review and trust](expertise-trust-paradox.md), [dependency graphs](forge-dependency-graphs.md), and [testing effectiveness](ai-driven-testing-efficiency.md). This note focuses on architectural discovery, ownership, and net simplification.

## 2. Research findings with sources

| Evidence | Relevant finding | Limitation or counterpoint |
| --- | --- | --- |
| [Thillen et al., CodeTaste, ICML 2026](https://proceedings.mlr.press/v306/thillen26a.html), multi-file refactoring benchmark | Agents implement detailed refactoring instructions more successfully than they discover human-selected refactorings from a focus area. Proposing before implementing improves alignment. | Directly relevant to discovery versus execution, but agreement with a historical human choice is not proof of the only good design. It does not isolate fresh context or cross-model review. |
| [Dinu et al., SmellBench, 2026 preprint v2](https://arxiv.org/abs/2605.07001v2), 65 detected architectural smells in scikit-learn | Expert validation classified 63.1% of detections as false positives. The most aggressive agent introduced 140 new smells, illustrating the need to inspect net impact. | One project and detector; introduced smells are detector findings, not automatically validated defects. Avoid treating smell removal count as an objective. |
| [Orlanski et al., SlopCodeBench, 2026 preprint v2](https://arxiv.org/abs/2603.24755v2), iterative implementation benchmark | Tracks redundant code and concentration of control-flow complexity across evolving requirements. Quality guidance improves initial scores without stopping later degradation in the reported evaluation. | Static proxies and benchmark tasks do not establish total architectural quality or why a model chose a design. Prompting alone is not a demonstrated long-term solution. |
| [Cai et al., ICSE 2025](https://research.google/pubs/understanding-architectural-complexity-maintenance-burden-and-developer-sentiment-a-large-scale-study/), 1,200+ Google C++/Java projects and 7,200 survey responses | Higher propagation cost and architectural anti-patterns correlated with a greater share of code changes spent on bug fixes rather than features. | Observational association, not evidence that optimizing a metric causes lower maintenance cost. Different languages and organizational setting. |
| [Cotroneo et al., ISSRE 2025](https://arxiv.org/abs/2508.21634), human/AI code comparison | AI samples were generally structurally simpler and more repetitive than human samples. | Contradicts a blanket claim that AI always produces more complex code. Isolated generated samples differ from repository evolution; local simplicity and system simplicity are different outcomes. |
| [Oueslati et al., RefAgent, preprint v2, 2026](https://arxiv.org/abs/2511.03153v2), eight Java projects | A specialized multi-agent refactoring pipeline improved compilation/test outcomes over its single-agent comparison and identified useful opportunities. | Positive evidence for a particular pipeline, not proof that a fresh cross-model critic is better. Planning, tools, extra work, and review are bundled; passing unit tests does not prove preserved architecture or behavior. |
| [Adams et al., Nature 2021](https://pubmed.ncbi.nlm.nih.gov/33828317/), eight human experiments | Participants overlooked useful subtractive changes more often without subtraction cues and under higher cognitive load. | Motivates a deletion-first experiment; it is evidence about humans, not a demonstrated LLM bias or remedy. |

[Parnas's modularization paper](https://doi.org/10.1145/361598.361623) gives a design argument for hiding changeable decisions behind module boundaries. [Fowler's YAGNI guidance](https://martinfowler.com/bliki/Yagni.html) argues against speculative capability while preserving the ability to change code. These are engineering principles, not controlled proof that adding or deleting a layer is correct in a particular system.

**Evidence boundary:** architectural simplification is possible in some studied settings, but no source here establishes that an unguided fresh agent consistently finds a simpler architecture, or that prevention always beats later refactoring. Published configurations and task distributions limit transfer across models and harnesses.

## 3. Possible explanations for model behavior

The following are hypotheses to investigate, not established causal accounts:

- **Local success criteria:** tests reward implemented behavior, while duplicated policy, indirection, and future change cost remain unmeasured. Extracting a helper can lower one function's complexity without reducing branching or responsibilities overall.
- **Uncertainty represented as code:** incomplete knowledge of inputs or failure contracts encourages optional fields, repeated checks, catches, and fallback paths instead of resolving the uncertainty at its owner.
- **Patch preservation:** extending an existing structure can appear cheaper than reconsidering it. Later repairs accumulate compatibility paths and translations around earlier decisions.
- **Pattern imitation and speculative requirements:** familiar design patterns or vague “extensible” requirements may encourage infrastructure whose current consumer or variability is unclear. Their prevalence in training data is not measured here.
- **Lost system context:** a reviewer sees a problematic file but misses callers, trust boundaries, state transitions, or operational requirements that determine whether a simpler design is valid.

Relocation occurs when the same coordination, policy decisions, or invalid states survive behind a new adapter, utility, configuration layer, or caller obligation. Conversely, moving complexity to the component that owns it can improve architecture without deleting it. Evaluate the resulting ownership and change cost.

## 4. Prevention versus post-implementation simplification

**Prevention candidate:** during planning, state required behavior, non-goals, invariants, trust boundaries, failure semantics, and concrete extension needs. Compare the existing design, the most direct implementation, and an abstraction only when it solves a current constraint. Ask which responsibility or changeable decision each boundary owns. Avoid prescribing a pattern merely because it sounds maintainable.

**Refactoring candidate:** use actual callers, incident history, and real change requests to challenge the design. Retain the option to make no change. Deletion-first means first considering removal of unneeded capability, redundant representations, or pass-through coordination; it does not mean a deletion quota or stripping necessary failure handling.

Early review may avoid implementation sunk cost, but also risks speculative analysis before requirements are understood. Later review has better evidence but pays migration and regression costs. SlopCodeBench supports concern about accumulation, not a universal cost ratio or proof that upfront design prevents it. Compare lifecycle costs experimentally.

## 5. Deterministic checks versus semantic architectural review

| Check | Mechanically detectable | Requires semantic judgment |
| --- | --- | --- |
| TypeScript compiler | `strict` checks; [exact optional-property semantics](https://www.typescriptlang.org/tsconfig/exactOptionalPropertyTypes.html); [possibly absent indexed values](https://www.typescriptlang.org/tsconfig/noUncheckedIndexedAccess.html). The latter two are separate settings, not implied by `strict`. | Whether a property should be optional, the correct domain states, or the right invariant owner. More accurate types can legitimately require additional checks. |
| [Type-aware linting](https://typescript-eslint.io/rules/no-unnecessary-condition/) | Conditions unnecessary according to known types; related rules detect unsafe operations or redundant assertions. | Declared types can be wrong at runtime. A lint finding cannot justify removing boundary validation without inspecting its contract. |
| [Knip](https://knip.dev/guides/handling-issues) | Unreachable files, unused exports/dependencies, subject to configured entry points and plugins. | Dynamic entry points and public consumers can be missed. Reachability does not establish business necessity. |
| [Dependency-cruiser](https://github.com/sverweij/dependency-cruiser) or equivalent boundary checks | Cycles and violations of declared import rules. | Whether boundaries are appropriate or an acyclic design has too much indirection. See the separate graph research. |
| AST/clone and complexity analysis | Repeated syntax, branch counts, large functions, growing public surface. | Semantic duplication, multiple responsibilities, useful defensive checks, and whether an abstraction earns its cost. Treat these as review signals. |

**TypeScript nuance:** [`unknown` is safer than `any`](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-3-0.html) for genuinely untrusted values and requires narrowing before normal use. The concern is unnecessarily broad internal contracts or repeated decoding after a trusted boundary. Banning `unknown` would remove a useful safety mechanism.

**Invariant ownership:** identify the authoritative business rule and the layer allowed to change its state. Similar-looking checks may serve different purposes: client feedback, server authorization, and database integrity can all be necessary. Consolidating rule definitions can help; deleting enforcement across separate trust or concurrency boundaries can introduce defects. A fallback is justified by an explicit availability/compatibility requirement and observable failure semantics, not simply by the possibility of an exception.

### Indicators of net simplification

Use the same affected subsystem and representative scenarios before and after the change; inspect callers, configuration, tests, and runtime state as well as edited files.

| Indicator | Useful signal | Limitation |
| --- | --- | --- |
| Dependency cycles and propagation/reachability | Potential change impact and coupling. | Dynamic dependencies and ubiquitous utility modules complicate interpretation. |
| Owners and representations of each invariant/state | Duplicate policy authorities, conversions, synchronization obligations. | Requires a reviewed semantic map; not a trustworthy automatic count. |
| Function-complexity distribution and interface surface | Concentrated branching, expanding contracts, displaced complexity. | Splitting methods or inlining interfaces can game counts. |
| Change amplification | Files/components and human/model effort needed for a real subsequent change. | Change difficulty varies; compare matched tasks. |
| Correctness, diagnosis, and operational outcomes | Preserved behavior, failure visibility, latency/resource cost, and regressions. | Tests and finite measurements do not prove equivalence. |

No single “simplicity score” should authorize a rewrite. More modules can lower coordination cost; fewer modules can create a monolith. Evaluate cost and constraints alongside structural indicators.

## 6. Proposed experiment: independent architectural discovery

Use [E2 and the shared measurement rules](README.md#five-experiments-to-prioritize). Prepare cases with specs, repository revisions, contracts, operational constraints, and representative later changes. Include overengineered cases and cases where complexity is justified. Hide the developer's preferred solution and historical refactoring; avoid issue titles or prompts that reveal the answer.

Compare author-context review, fresh context on the same model, and fresh context on a different model with matched evidence and total budgets. Keep an unchanged reference. A solution-informed arm measures execution ability, not independent discovery; a generic deletion cue is a later variant only if justified. Fresh context reduces conversational anchoring but does not establish independent model errors.

Lock a short proposal before implementation: obligation/cost, candidate design, what disappears, remaining owner, preserved contracts, strongest counterexample, and evidence needed. Include justified no-change decisions. Blind qualified reviewers to the arm and accept multiple sound designs rather than scoring exact agreement with the developer.

In a later authorized sandbox trial, implement locked proposals under a common budget, assess behavior and operational constraints, and perform a held-out realistic extension or diagnosis task. Score discovery separately from execution. Measure valid proposals without hints, false simplifications, net changes across callers/configuration/tests, preserved invariants, regressions, follow-up effort, human corrections, and full cost. Lower smell counts or weakened tests cannot establish success.

Planning prevention and static guardrail efficacy remain plausible approaches, but existing executable rules already provide value in the reported workflow. Reopen a targeted comparison only when E2 or observed failures identify a concrete gap.

## 7. Practical recommendations

The first candidate worth evaluating is **one fresh, solution-blind proposal pass on costly or recurrent architectural problems**, not a standing committee of agents. Ask it to demonstrate reduced obligations or lower future change cost while preserving requirements. CodeTaste suggests discovery is the hard step; measure that step before paying for implementation.

Use deterministic checks for known rules and reserve architectural review for ownership, necessity, and consequences. Require each abstraction, repeated validation, and fallback to cite the current constraint it serves. Permit justified complexity and explicit uncertainty. Reject claimed simplifications supported only by moved files, fewer classes, or prettier local metrics.

Keep the review artifact compact and evidence-linked. Stop when no validated architectural benefit is identified; repeated unconstrained “simplify again” requests can consume supervision and degrade a working design. These are research recommendations to test, not new workflow rules.

## 8. Remaining decision

E2 must establish whether a fresh reviewer independently finds a design that lowers real change cost while preserving justified complexity. Model family, context freshness, and extra compute must not be credited interchangeably.
