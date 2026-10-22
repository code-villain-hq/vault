// Requirements of a spec written in OpenSpec's convention, and the latest
// verdict a drift check gave each. The server parses requirements the same
// way (`forge_domain::requirements`).
import type { RequirementVerdict, Service, Verdict } from "@forge/types";

export type Requirement = { name: string; text: string };

const HEADING = "### Requirement:";

/** A heading of level 1 to 3, which ends the requirement before it. */
function endsRequirement(line: string): boolean {
  return /^#{1,3} /.test(line);
}

/** Each `### Requirement:` with its sentence and scenarios, in order. */
export function requirements(body: string): Requirement[] {
  const found: Requirement[] = [];
  let current: { name: string; lines: string[] } | null = null;
  let fenced = false;
  const close = () => {
    if (current) found.push({ name: current.name, text: current.lines.join("\n").trimEnd() });
    current = null;
  };
  for (const line of body.split(/\r?\n/)) {
    if (line.trimStart().startsWith("```")) fenced = !fenced;
    const heading = !fenced && endsRequirement(line);
    if (heading) close();
    if (heading && line.startsWith(HEADING)) {
      const name = line.slice(HEADING.length).trim();
      if (name) current = { name, lines: [] };
    }
    current?.lines.push(line);
  }
  close();
  return found;
}

/**
 * The text around a spec's requirements: what comes before the first one
 * (its Purpose, without the "## Requirements" heading) and what follows the
 * last one, so a page can show the requirements in between its own way.
 */
export function aroundRequirements(body: string): { before: string; after: string } {
  const before: string[] = [];
  const after: string[] = [];
  let phase: "before" | "in" | "after" = "before";
  let fenced = false;
  for (const line of body.split(/\r?\n/)) {
    if (line.trimStart().startsWith("```")) fenced = !fenced;
    if (!fenced && line.startsWith(HEADING) && line.slice(HEADING.length).trim()) phase = "in";
    else if (!fenced && phase === "in" && endsRequirement(line)) phase = "after";
    if (phase === "before") before.push(line);
    if (phase === "after") after.push(line);
  }
  const intro = before.join("\n").trimEnd();
  return {
    before: intro.replace(/(^|\n)## Requirements\s*$/, "").trimEnd(),
    after: after.join("\n").trim(),
  };
}

/** A requirement's sentence and scenarios, without its heading. */
export function requirementBody(requirement: Requirement): string {
  return requirement.text.split("\n").slice(1).join("\n").trim();
}

export type RequirementStatus = {
  requirement: Requirement;
  verdict: RequirementVerdict | null;
  /** Why the verdict may no longer hold. */
  stale: "requirement" | "code" | null;
};

/** Each requirement of the spec with its latest verdict, if it has one. */
export function requirementStatuses(
  body: string,
  verdicts: RequirementVerdict[],
  service: Service | undefined,
): RequirementStatus[] {
  return requirements(body).map((requirement) => {
    const verdict = verdicts.find((v) => v.requirement === requirement.name) ?? null;
    let stale: RequirementStatus["stale"] = null;
    if (verdict && verdict.requirement_text.trimEnd() !== requirement.text) stale = "requirement";
    else if (
      verdict &&
      service?.code_changed_at &&
      Date.parse(service.code_changed_at) > Date.parse(verdict.checked_at)
    )
      stale = "code";
    return { requirement, verdict, stale };
  });
}

export const VERDICTS: Record<Verdict, { label: string; color: string }> = {
  met: { label: "Met", color: "var(--muted)" },
  not_met: { label: "Not met", color: "var(--danger)" },
  unclear: { label: "Unclear", color: "var(--warn)" },
};
