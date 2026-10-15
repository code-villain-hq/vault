// Whether a behaviour spec follows OpenSpec's convention: the same structural
// checks as `openspec validate`. Problems are warnings; the spec is saved
// anyway. The server checks the same way (`forge_domain::convention_warnings`)
// and tells agents on `save_spec`.
import { requirements } from "./verdicts";

/** A heading of at most `level`, e.g. `## Purpose` for 2. */
function isHeading(line: string, level: number): boolean {
  return new RegExp(`^#{1,${level}} `).test(line);
}

/** A requirement's statement (before its first `####` heading) and its scenario count. */
function parts(text: string): { statement: string; scenarios: number } {
  let statement = "";
  let scenarios = 0;
  let fenced = false;
  let inStatement = true;
  // The first line is the requirement's heading.
  for (const line of text.split(/\r?\n/).slice(1)) {
    if (line.trimStart().startsWith("```")) fenced = !fenced;
    if (!fenced && line.startsWith("#### ")) {
      inStatement = false;
      if (line.startsWith("#### Scenario:")) scenarios++;
    } else if (inStatement) statement += `${line}\n`;
  }
  return { statement, scenarios };
}

function saysShallOrMust(text: string): boolean {
  return text.split(/[^\p{L}\p{N}]/u).some((word) => word === "SHALL" || word === "MUST");
}

/**
 * What keeps the spec from following the convention, in plain sentences.
 * Only specs with a `## Requirements` section are behaviour specs; prose gets none.
 */
export function conventionWarnings(body: string): string[] {
  let fenced = false;
  let behaviour = false;
  let purpose: string | null = null;
  let inPurpose = false;
  for (const line of body.split(/\r?\n/)) {
    if (line.trimStart().startsWith("```")) fenced = !fenced;
    if (!fenced && isHeading(line, 2)) {
      const title = line.replace(/^#+/, "").trim();
      inPurpose = title === "Purpose";
      if (inPurpose) purpose ??= "";
      if (title === "Requirements") behaviour = true;
    } else if (inPurpose && purpose !== null) purpose += line;
  }
  if (!behaviour) return [];

  const warnings: string[] = [];
  if (purpose === null) warnings.push('It has no "## Purpose" section.');
  else if (purpose.trim() === "") warnings.push('Its "## Purpose" section is empty.');
  const found = requirements(body);
  if (found.length === 0)
    warnings.push('It has no requirements: add one as "### Requirement: <name>".');
  found.forEach(({ name, text }, i) => {
    const { statement, scenarios } = parts(text);
    if (!saysShallOrMust(statement))
      warnings.push(`Requirement "${name}" doesn't say SHALL or MUST.`);
    if (scenarios === 0)
      warnings.push(`Requirement "${name}" has no scenario: add one as "#### Scenario: <name>".`);
    if (found.slice(0, i).filter((r) => r.name === name).length === 1)
      warnings.push(`Two requirements are named "${name}"; each needs its own name.`);
  });
  return warnings;
}
