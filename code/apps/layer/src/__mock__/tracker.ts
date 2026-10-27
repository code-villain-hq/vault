/**
 * `&tracker=github`: the workspace takes its tickets from GitHub Issues
 * (TKT-15), so the same world's tickets read as issues: `acme/web#123`,
 * open or closed, an epic being an issue with sub-issues (TKT-17).
 */
import type { Ticket } from "../lib/types";
import type { World } from "./world";

const asIssue = (key: string | null) => (key ? `acme/web#${key.replace(/^\D+-/, "")}` : null);

function issue(t: Ticket): Ticket {
  const key = asIssue(t.key) ?? t.key;
  const closed = t.status_category === "done";
  return {
    ...t,
    key,
    status: closed ? "Closed" : "Open",
    status_id: closed ? "closed" : "open",
    status_category: closed ? "done" : t.assignee ? "indeterminate" : "new",
    issue_type: t.issue_type === "Epic" ? "Epic" : "Issue",
    priority: null,
    components: [],
    epic_key: asIssue(t.epic_key),
    url: `https://github.com/${key.replace("#", "/issues/")}`,
  };
}

export function githubTracker(world: World) {
  world.settings.tracker = "github";
  world.settings.tracker_chosen = "github";
  world.issues = world.issues.map(issue);
  world.elsewhere = world.elsewhere.map(issue);
  world.issueTypes = [
    { id: "Issue", name: "Issue", subtask: false, hierarchy_level: 0, icon: null },
    { id: "Epic", name: "Epic", subtask: false, hierarchy_level: 1, icon: null },
  ];
  world.requiredFields = [];
  for (const t of world.tasks) {
    t.issue_key = asIssue(t.issue_key);
    t.issue_url = t.issue_key ? `https://github.com/${t.issue_key.replace("#", "/issues/")}` : null;
  }
}
