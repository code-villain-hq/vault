# Blog seed: Reducing verification cost in spec-driven development

**Status:** editorial background; wait for measurements before drafting claims about efficiency.

## Angle

Use deterministic checks for mechanical constraints and models for behavioral judgment, while measuring the human effort needed to accept software. Incremental spec/code checks are already part of the developer's Forge workflow, with findings entering PM/PO triage. Present this as reported practice, not a capability still to build.

## Material retained from the conversation

- Knip and domain-boundary scripts reduce the need for agents to guess about mechanically checkable rules.
- Commit-based incremental review avoids revisiting unchanged code; checkpoint and coverage correctness still matter.
- Dependency graphs can suggest affected areas but cannot certify requirements as unaffected.
- Drift findings become tracked work for human triage.

The attached conversation's cached assistant messages were placeholders. The seed originated from visible user messages; the current capability description reflects the developer's later clarification.

## Evidence needed before publication

Use the [research roadmap](../README.md) as the experiment priority. A useful post needs actual review time, total cost, validated findings, and missed-defect evidence. Describe graph assistance as parked unless a measured selection problem warrants testing. Avoid claims that fewer tokens, more agents, or a smaller checked scope establish better verification.

Publish-ready drafts belong in `studio/content/blog/`. This seed adds no experiments or product work.
