// "G" then a letter goes to a page, as in Linear; "?" lists every shortcut.
import { type GoTo, type ShortcutSection, Shortcuts } from "@code-villain/web";
import { useNavigate } from "@tanstack/react-router";
import { isDesktop } from "./platform";
import { useWorkspace } from "./workspace";

const GO_TO: Record<string, { to: string; name: string }> = {
  i: { to: "/$workspace/inbox", name: "Inbox" },
  m: { to: "/$workspace/my-issues", name: "My issues" },
  t: { to: "/$workspace/triage", name: "Triage" },
  p: { to: "/$workspace/projects", name: "Projects" },
  n: { to: "/$workspace/initiatives", name: "Initiatives" },
  s: { to: "/$workspace/services", name: "Services" },
  e: { to: "/$workspace/domains", name: "Domains" },
  d: { to: "/$workspace/decisions", name: "Decisions" },
};

/** Forge's own shortcuts; the kit's list adds "Anywhere" and "Menus". */
const SECTIONS: ShortcutSection[] = [
  {
    title: "Issue lists and boards",
    keys: [
      ["J K or ↓ ↑", "Next and previous issue"],
      ["H L or ← →", "Previous and next column (board)"],
      ["⇧ ← →", "Move the issue to the previous or next status (board)"],
      ["→ ←", "Show or hide the issue's sub-issues (list)"],
      ["Enter", "Open the issue"],
      ["X", "Select the issue, to change several at once (list)"],
      ["Esc", "Clear the selection"],
      ["F", "Filter"],
    ],
  },
  {
    title: "An issue",
    keys: [
      ["Esc", "Back to where you came from"],
      ["1 2 3", "In triage: accept, mark as a duplicate, decline"],
      ["⌘ ⇧ .", "Copy the git branch name"],
      ["⌘ Enter", "Send a comment"],
    ],
  },
];

/** The "G" keys, "?" and, in the desktop app, ⌘, for settings. */
export function ForgeShortcuts() {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const params = { workspace: workspace.url_key };
  // `to` varies here, so the router can't check it; every path is a real route.
  const goTo: GoTo[] = Object.entries(GO_TO).map(([key, { to, name }]) => ({
    key,
    name,
    go: () => void navigate({ to, params } as never),
  }));
  return (
    <Shortcuts
      goTo={goTo}
      anywhere={[["C", "New issue"]]}
      sections={SECTIONS}
      onSettings={
        isDesktop ? () => void navigate({ to: "/$workspace/settings/members", params }) : undefined
      }
    />
  );
}
