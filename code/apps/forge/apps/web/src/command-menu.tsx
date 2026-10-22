import { type Command, CommandMenu, matching } from "@code-villain/web";
import { useNavigate, useParams } from "@tanstack/react-router";
import {
  Bot,
  Box,
  CircleUserRound,
  Folders,
  Funnel,
  GitMerge,
  GitPullRequest,
  Inbox,
  Layers,
  Mountain,
  Scale,
  Server,
  SquarePen,
  Tag,
  UserRound,
  UsersRound,
} from "lucide-react";
import { useMemo } from "react";
import {
  branchName,
  byId,
  identifier,
  PRIORITY_LABELS,
  useIssueByIdentifier,
  useTeamStates,
  useTeams,
} from "./data";
import { NavIcon, PriorityIcon, ProjectIcon, StateIcon } from "./icons";
import { useNewIssue } from "./new-issue";
import { placeName } from "./server";
import { signOut, useSession, workspaceEntries } from "./session";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";

/** ⌘K in Forge: act on the open issue, create, go to, settings, workspaces, and search. */
export function ForgeCommandMenu() {
  return (
    <CommandMenu
      useCommands={useForgeCommands}
      placeholder="Type a command, or search issues, specs and more…"
    />
  );
}

function useForgeCommands(q: string): Command[] {
  const { sync, place, me, workspace } = useWorkspace();
  const session = useSession();
  const entries = useMemo(
    () => (session.status === "signed-in" ? workspaceEntries(session.places) : []),
    [session],
  );
  const navigate = useNavigate();
  const newIssue = useNewIssue();
  const teams = useTeams();
  const guest = useIsGuest();
  const teamsById = useMemo(() => byId(teams), [teams]);
  const issues = useRecords("Issue");
  const projects = useRecords("Project");
  const params = useParams({ strict: false }) as { identifier?: string };
  const current = useIssueByIdentifier(params.identifier ?? "");
  const currentStates = useTeamStates(current?.team_id);

  const commands = useMemo(() => {
    const ws = { workspace: workspace.url_key };
    // `to` varies here, so the router can't check it; every path is a real route.
    const go =
      (to: string, extra: Record<string, string> = {}) =>
      () =>
        void navigate({ to, params: { ...ws, ...extra } } as never);
    const list: Command[] = [];

    if (current) {
      for (const state of currentStates) {
        list.push({
          id: `state-${state.id}`,
          group: identifier(current, teamsById),
          label: `Set status: ${state.name}`,
          icon: <StateIcon state={state} />,
          run: () => sync.update("Issue", current.id, { state_id: state.id }),
        });
      }
      PRIORITY_LABELS.forEach((label, priority) => {
        list.push({
          id: `priority-${priority}`,
          group: identifier(current, teamsById),
          label: `Set priority: ${label}`,
          icon: <PriorityIcon priority={priority} />,
          run: () => sync.update("Issue", current.id, { priority }),
        });
      });
      list.push({
        id: "copy-branch",
        group: identifier(current, teamsById),
        label: "Copy git branch name",
        hint: "⌘⇧.",
        run: () => void navigator.clipboard.writeText(branchName(current, teamsById)),
      });
      list.push({
        id: "assign-me",
        group: identifier(current, teamsById),
        label: current.assignee_id === me.user.id ? "Unassign me" : "Assign to me",
        run: () =>
          sync.update("Issue", current.id, {
            assignee_id: current.assignee_id === me.user.id ? null : me.user.id,
          }),
      });
    }

    list.push(
      {
        id: "new",
        group: "Actions",
        label: "Create issue",
        hint: "C",
        icon: <NavIcon icon={SquarePen} />,
        run: () => newIssue(),
      },
      // With the sidebar's icons, and the G shortcut that goes there too.
      ...(
        [
          ["inbox", "Inbox", Inbox, "G I", "/$workspace/inbox"],
          ["my", "My issues", CircleUserRound, "G M", "/$workspace/my-issues"],
          ["triage", "Triage", Funnel, "G T", "/$workspace/triage"],
          // A guest has none of the workspace's plans and knowledge.
          ...(guest
            ? []
            : ([
                ["projects", "Projects", Box, "G P", "/$workspace/projects"],
                ["initiatives", "Initiatives", Mountain, "G N", "/$workspace/initiatives"],
                ["services", "Services", Server, "G S", "/$workspace/services"],
                ["domains", "Domains", Folders, "G E", "/$workspace/domains"],
                ["decisions", "Decisions", Scale, "G D", "/$workspace/decisions"],
              ] as const)),
        ] as const
      ).map(([id, label, icon, hint, to]) => ({
        id,
        group: "Go to",
        label,
        hint,
        icon: <NavIcon icon={icon} />,
        run: go(to),
      })),
      ...teams.map((team) => ({
        id: `team-${team.id}`,
        group: "Go to",
        label: `${team.name} issues`,
        hint: team.key,
        run: go("/$workspace/team/$team", { team: team.key }),
      })),
      ...projects.map((project) => ({
        id: `project-${project.id}`,
        group: "Projects",
        label: project.name,
        icon: <ProjectIcon />,
        run: go("/$workspace/project/$project", { project: project.id }),
      })),
      ...(
        [
          ...(guest
            ? []
            : ([
                ["members", "Members", UsersRound, "/$workspace/settings/members"],
                ["teams", "Teams", Layers, "/$workspace/settings/teams"],
                ["labels", "Labels", Tag, "/$workspace/settings/labels"],
                ["agents", "Agents", Bot, "/$workspace/settings/agents"],
                ["github", "GitHub", GitPullRequest, "/$workspace/settings/github"],
                ["forgejo", "Forgejo", GitMerge, "/$workspace/settings/forgejo"],
              ] as const)),
          ["account", "Account", UserRound, "/$workspace/settings/account"],
        ] as const
      ).map(([id, label, icon, to]) => ({
        id,
        group: "Settings",
        label,
        icon: <NavIcon icon={icon} />,
        run: go(to),
      })),
      ...entries
        .filter((e) => e.key !== workspace.url_key)
        .map((e) => ({
          id: `workspace-${e.key}`,
          group: "Workspaces",
          label: `Switch to ${e.workspace.name}`,
          run: () => void navigate({ to: "/$workspace", params: { workspace: e.key } }),
        })),
    );
    // A workspace on this Mac has no one to sign out.
    if (place.kind !== "mac") {
      list.push({
        id: "sign-out",
        group: "Account",
        label: place.kind === "server" ? `Sign out of ${placeName(place)}` : "Sign out",
        run: () => void signOut(place).then(() => navigate({ to: "/" })),
      });
    }
    return list;
  }, [
    current,
    currentStates,
    guest,
    teams,
    teamsById,
    projects,
    me,
    workspace,
    entries,
    place,
    sync,
    navigate,
    newIssue,
  ]);

  const services = useRecords("Service");
  const specs = useRecords("Spec");
  const decisions = useRecords("Decision");
  const initiatives = useRecords("Initiative");

  // What a query is matched against, lowercased once per change of the
  // records rather than on every key: spec bodies are long.
  const knowledge = useMemo(() => {
    const ws = workspace.url_key;
    const index = <T,>(
      group: string,
      records: T[],
      name: (r: T) => string,
      text: (r: T) => string,
      id: (r: T) => string,
      open: (r: T) => void,
    ): Searchable[] =>
      records.map((r) => ({
        group,
        id: id(r),
        label: name(r),
        name: name(r).toLowerCase(),
        text: text(r).toLowerCase(),
        open: () => open(r),
      }));
    return [
      index(
        "Services",
        services,
        (s) => s.name,
        (s) => `${s.description} ${s.current_state}`,
        (s) => s.id,
        (s) =>
          void navigate({
            to: "/$workspace/service/$service",
            params: { workspace: ws, service: s.id },
          }),
      ),
      index(
        "Specs",
        specs,
        (s) => s.title,
        (s) => s.body,
        (s) => s.id,
        (s) =>
          void navigate({ to: "/$workspace/spec/$spec", params: { workspace: ws, spec: s.id } }),
      ),
      index(
        "Decisions",
        decisions,
        (d) => d.title,
        (d) => `${d.context} ${d.decision} ${d.consequences}`,
        (d) => d.id,
        (d) =>
          void navigate({
            to: "/$workspace/decision/$decision",
            params: { workspace: ws, decision: d.id },
          }),
      ),
      index(
        "Initiatives",
        initiatives,
        (i) => i.name,
        (i) => i.description,
        (i) => i.id,
        (i) =>
          void navigate({
            to: "/$workspace/initiative/$initiative",
            params: { workspace: ws, initiative: i.id },
          }),
      ),
    ];
  }, [services, specs, decisions, initiatives, navigate, workspace.url_key]);

  // Made offline, an issue opens once the server has given it a number.
  const issueIndex = useMemo(
    () =>
      issues
        .filter((issue) => issue.number)
        .map((issue) => {
          const key = identifier(issue, teamsById);
          return { issue, key, id: key.toLowerCase(), title: issue.title.toLowerCase() };
        }),
    [issues, teamsById],
  );

  return useMemo(() => {
    const commandsFound = matching(commands, q);
    if (!q) return commandsFound;
    const found: Command[] = [];
    for (const { issue, key, id, title } of issueIndex) {
      if (!id.includes(q) && !title.includes(q)) continue;
      found.push({
        id: `issue-${issue.id}`,
        group: "Issues",
        label: issue.title,
        hint: key,
        run: () =>
          void navigate({
            to: "/$workspace/issue/$identifier",
            params: { workspace: workspace.url_key, identifier: key },
          }),
      });
      if (found.length === 8) break;
    }
    return [...found, ...knowledge.flatMap((group) => findIn(group, q)), ...commandsFound];
  }, [commands, issueIndex, q, navigate, workspace.url_key, knowledge]);
}

/** A record the command menu can find: its name and text, lowercased. */
interface Searchable {
  group: string;
  id: string;
  label: string;
  name: string;
  text: string;
  open: () => void;
}

/** Up to five of a group: names that match first, then texts. */
function findIn(records: Searchable[], q: string): Command[] {
  const byName: Searchable[] = [];
  const byText: Searchable[] = [];
  for (const r of records) {
    if (r.name.includes(q)) {
      byName.push(r);
      if (byName.length === 5) break;
    } else if (byText.length < 5 && r.text.includes(q)) byText.push(r);
  }
  return [...byName, ...byText]
    .slice(0, 5)
    .map((r) => ({ id: `${r.group}-${r.id}`, group: r.group, label: r.label, run: r.open }));
}
