# Parked hypothesis: Dependency graphs for Forge drift checks

**Status: deprioritized.** Forge already performs incremental drift checks. No local measurement currently demonstrates that an additional dependency graph or requirement-to-module map would reduce total cost or improve coverage.

## Retained finding

A code graph can suggest areas affected by changed files, including reverse dependencies. It cannot prove that a requirement is implemented or that an unselected requirement remains correct. Service/path ownership and spec/service associations were present in the earlier inspection; that does not establish a missing capability in today's full workflow.

A graph's usefulness for narrowing review is a hypothesis. Missing dynamic edges and stale mappings can create false confidence. Manually maintained requirement links add maintenance cost; an automatically generated graph still needs scope and freshness checks. Graph traversal alone is not behavioral validation.

## Reopening criterion

Follow the [roadmap](README.md#consolidated-and-deprioritized-work): revisit only after existing drift records show a concrete selection-cost or missed-impact problem. Inspect how Forge already selects work before designing another mechanism.

If justified, compare the current selector with a bounded graph-assisted candidate on the same historical changes. Use independent reference checks to measure consequential misses, useful findings, total human/model effort, and graph maintenance. Widen uncertain cases rather than silently treating absent edges as coverage. A lower number of requirements checked is not sufficient evidence of improvement.

No graph implementation, separate experiment queue, or Forge spec change is proposed here.
