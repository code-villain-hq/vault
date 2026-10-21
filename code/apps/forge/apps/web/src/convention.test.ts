import { expect, test } from "bun:test";
import { conventionWarnings } from "./convention";

// The same cases as the server's (`forge_domain::convention_warnings`), so
// agents and the spec editor get the same warnings.

const GOOD = `## Purpose

Rate limits.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429
`;

test("a spec that follows the convention has no warnings", () => {
  expect(conventionWarnings(GOOD)).toEqual([]);
});

test("prose is left alone", () => {
  expect(conventionWarnings("# Overview\n\nJust prose, no purpose.\n")).toEqual([]);
  expect(conventionWarnings("Text.\n```md\n## Requirements\n```\n")).toEqual([]);
});

test("names each problem", () => {
  const body = `## Requirements

### Requirement: Tokens are limited
The API should allow 100 requests a minute.

#### Scenario: Too many
- **WHEN** a token sends a 101st request
- **THEN** it gets 429

### Requirement: Limits reset
The limit MUST reset every minute.

#### Not a scenario

### Requirement: Tokens are limited
The API SHALL limit tokens.

#### Scenario: Again
- **THEN** it's limited
`;
  expect(conventionWarnings(body)).toEqual([
    'It has no "## Purpose" section.',
    `Requirement "Tokens are limited" doesn't say SHALL or MUST.`,
    'Requirement "Limits reset" has no scenario: add one as "#### Scenario: <name>".',
    'Two requirements are named "Tokens are limited"; each needs its own name.',
  ]);
});

test("an empty purpose and no requirements", () => {
  expect(conventionWarnings("## Purpose\n\n## Requirements\n\nNone yet.\n")).toEqual([
    'Its "## Purpose" section is empty.',
    'It has no requirements: add one as "### Requirement: <name>".',
  ]);
});

test("SHALL counts only as a word in the statement", () => {
  const body = GOOD.replace("SHALL allow", "allows (MARSHALL)");
  expect(conventionWarnings(body)).toEqual([
    `Requirement "Tokens are limited" doesn't say SHALL or MUST.`,
  ]);
  expect(conventionWarnings(body.replace("it gets 429", "it MUST get 429"))).toHaveLength(1);
});
