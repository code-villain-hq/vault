# Background: Incremental drift checks in Forge

**Status: existing capability; no active implementation research.** The developer's current workflow already runs incremental spec-to-code checks in Forge and routes findings into PM/PO triage. This is a reported workflow observation, not a controlled evaluation of cost or missed drift.

## Correction to the earlier proposal

The original note proposed commit checkpoints, incremental selection, and reporting as work to add. That framing is superseded. An earlier inspection of the built-in checker and agent-reporting API did not establish what the complete scheduled workflow already provided. It must not be used to infer that Forge lacks incremental checks or needs a new triage process.

Earlier inspected integration points included service repository/path scopes, spec/service associations, and agent-run fields such as `commit_from`, `commit_to`, `full_audit`, planned requirements, and verdicts. Those are implementation context, not a fresh audit of the deployed checker.

## What remains useful

Deterministic guardrails can settle mechanical rules; spec/code review still needs behavioral evidence. An incremental selector can miss runtime wiring, configuration, generated code, or indirect effects. These are general limitations, not demonstrated gaps in the current Forge workflow.

The [research roadmap](README.md#consolidated-and-deprioritized-work) parks further drift work. Reopen only with measured excessive cost, incorrect coverage, or a missed requirement. Then inspect the existing check's range, selection, completion evidence, and related open issue before proposing a bounded correction. Do not assume a new graph, scheduler, checkpoint store, audit cadence, or reporting system is needed.

Any product behavior change follows Forge's existing specs and spec-change process. This document is background, not an implementation backlog.
