import { expect, test } from "bun:test";
import type { RequirementVerdict, Service } from "@forge/types";
import { aroundRequirements, requirementBody, requirementStatuses, requirements } from "./verdicts";

const SPEC = `## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429

\`\`\`md
### Requirement: Not one, it's in a code block
\`\`\`

### Requirement:   Limits reset  
The limit MUST reset every minute.

## Notes

Not part of a requirement.
`;

test("requirements are found with their scenarios, as the server finds them", () => {
  const found = requirements(SPEC);
  expect(found.map((r) => r.name)).toEqual(["Tokens are limited", "Limits reset"]);
  expect(found[0]?.text.startsWith("### Requirement: Tokens are limited")).toBe(true);
  expect(found[0]?.text).toContain("#### Scenario: Too many");
  expect(found[0]?.text).toContain("Not one, it's in a code block");
  expect(found[1]?.text.endsWith("reset every minute.")).toBe(true);
  expect(requirements("# Overview\n\nJust prose.\n### Not a requirement\n")).toEqual([]);
});

test("a verdict goes stale when its requirement or the service's code changes", () => {
  const [limited, reset] = requirements(SPEC);
  const verdict = (requirement: string, text: string) =>
    ({
      requirement,
      verdict: "met",
      requirement_text: text,
      checked_at: "2026-10-14T10:00:00Z",
    }) as RequirementVerdict;
  const verdicts = [
    verdict("Tokens are limited", limited?.text ?? ""),
    verdict("Limits reset", "### Requirement: Limits reset\nThe limit MUST reset every hour."),
    verdict("Renamed away", "whatever"),
  ];
  const service = (code_changed_at: string | null) => ({ code_changed_at }) as Service;

  const now = requirementStatuses(SPEC, verdicts, service(null));
  expect(now.map((s) => [s.requirement.name, s.verdict?.verdict, s.stale])).toEqual([
    ["Tokens are limited", "met", null],
    ["Limits reset", "met", "requirement"],
  ]);
  expect(reset?.name).toBe("Limits reset");

  const pushed = requirementStatuses(SPEC, verdicts, service("2026-10-14T10:00:01.5Z"));
  expect(pushed[0]?.stale).toBe("code");
  const before = requirementStatuses(SPEC, verdicts, service("2026-10-14T09:59:59.123456Z"));
  expect(before[0]?.stale).toBe(null);
  expect(requirementStatuses(SPEC, [], undefined)[0]?.verdict).toBe(null);
});

test("a spec splits around its requirements, each shown without its heading", () => {
  const { before, after } = aroundRequirements(SPEC);
  expect(before).toBe("## Purpose\n\nRate limits.");
  expect(after).toBe("## Notes\n\nNot part of a requirement.");
  const [limited] = requirements(SPEC);
  expect(limited && requirementBody(limited)).toStartWith(
    "The API SHALL allow 100 requests a minute per token.",
  );
  expect(limited && requirementBody(limited)).toContain("#### Scenario: Too many");
  expect(aroundRequirements("# Overview\n\nJust prose.")).toEqual({
    before: "# Overview\n\nJust prose.",
    after: "",
  });
});
