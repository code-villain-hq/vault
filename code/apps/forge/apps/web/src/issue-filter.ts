// Narrowing a list of issues by assignee, reviewer, priority, label,
// project or service. Values of one field are alternatives ("Ada or Bob"); different
// fields all have to match.
import type { Issue } from "@forge/types";

export type FilterField = "assignee" | "reviewer" | "priority" | "label" | "project" | "service";
export type Filters = Partial<Record<FilterField, string[]>>;

export const FIELDS: { field: FilterField; name: string }[] = [
  { field: "assignee", name: "Assignee" },
  { field: "reviewer", name: "Reviewer" },
  { field: "priority", name: "Priority" },
  { field: "label", name: "Label" },
  { field: "project", name: "Project" },
  { field: "service", name: "Service" },
];

/** "" stands for none: no assignee, no reviewer, no project, no service. */
function valuesOf(issue: Issue, field: FilterField): string[] {
  switch (field) {
    case "assignee":
      return [issue.assignee_id ?? ""];
    case "reviewer":
      return [issue.reviewer_id ?? ""];
    case "priority":
      return [String(issue.priority)];
    case "label":
      return issue.label_ids;
    case "project":
      return [issue.project_id ?? ""];
    case "service":
      return [issue.service_id ?? ""];
  }
}

export function applyFilters(issues: Issue[], filters: Filters): Issue[] {
  const active = FIELDS.filter(({ field }) => (filters[field]?.length ?? 0) > 0);
  if (active.length === 0) return issues;
  return issues.filter((issue) =>
    active.every(({ field }) => valuesOf(issue, field).some((v) => filters[field]?.includes(v))),
  );
}

export function toggle(filters: Filters, field: FilterField, value: string): Filters {
  const current = filters[field] ?? [];
  const next = current.includes(value) ? current.filter((v) => v !== value) : [...current, value];
  return { ...filters, [field]: next };
}
