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
import { useCallback, useMemo } from "react";
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
import { useRecords, useWorkspace } from "./workspace";

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
          ["projects", "Projects", Box, "G P", "/$workspace/projects"],
          ["initiatives", "Initiatives", Mountain, "G N", "/$workspace/initiatives"],
          ["services", "Services", Server, "G S", "/$workspace/services"],
          ["domains", "Domains", Folders, "G E", "/$workspace/domains"],
          ["decisions", "Decisions", Scale, "G D", "/$workspace/decisions"],
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
          ["members", "Members", UsersRound, "/$workspace/settings/members"],
          ["teams", "Teams", Layers, "/$workspace/settings/teams"],
          ["labels", "Labels", Tag, "/$workspace/settings/labels"],
          ["agents", "Agents", Bot, "/$workspace/settings/agents"],
          ["github", "GitHub", GitPullRequest, "/$workspace/settings/github"],
          ["forgejo", "Forgejo", GitMerge, "/$workspace/settings/forgejo"],
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
  /** Services, specs, decisions and initiatives: names first, then their text. */
  const findKnowledge = useCallback(
    (q: string): Command[] => {
      const ws = workspace.url_key;
      const search = <T,>(
        group: string,
        records: T[],
        name: (r: T) => string,
        text: (r: T) => string,
        open: (r: T) => void,
        id: (r: T) => string,
      ): Command[] =>
        records
          .map((r) => ({
            r,
            rank: name(r).toLowerCase().includes(q) ? 0 : text(r).toLowerCase().includes(q) ? 1 : 2,
          }))
          .filter((m) => m.rank < 2)
          .sort((a, b) => a.rank - b.rank)
          .slice(0, 5)
          .map(({ r }) => ({ id: `${group}-${id(r)}`, group, label: name(r), run: () => open(r) }));
      return [
        ...search(
          "Services",
          services,
          (s) => s.name,
          (s) => `${s.description} ${s.current_state}`,
          (s) =>
            void navigate({
              to: "/$workspace/service/$service",
              params: { workspace: ws, service: s.id },
            }),
          (s) => s.id,
        ),
        ...search(
          "Specs",
          specs,
          (s) => s.title,
          (s) => s.body,
          (s) =>
            void navigate({ to: "/$workspace/spec/$spec", params: { workspace: ws, spec: s.id } }),
          (s) => s.id,
        ),
        ...search(
          "Decisions",
          decisions,
          (d) => d.title,
          (d) => `${d.context} ${d.decision} ${d.consequences}`,
          (d) =>
            void navigate({
              to: "/$workspace/decision/$decision",
              params: { workspace: ws, decision: d.id },
            }),
          (d) => d.id,
        ),
        ...search(
          "Initiatives",
          initiatives,
          (i) => i.name,
          (i) => i.description,
          (i) =>
            void navigate({
              to: "/$workspace/initiative/$initiative",
              params: { workspace: ws, initiative: i.id },
            }),
          (i) => i.id,
        ),
      ];
    },
    [services, specs, decisions, initiatives, navigate, workspace.url_key],
  );

  return useMemo(() => {
    const commandsFound = matching(commands, q);
    if (!q) return commandsFound;
    const found = issues
      .filter((issue) => {
        const id = identifier(issue, teamsById).toLowerCase();
        return id.includes(q) || issue.title.toLowerCase().includes(q);
      })
      // Made offline, an issue opens once the server has given it a number.
      .filter((issue) => issue.number)
      .slice(0, 8)
      .map(
        (issue): Command => ({
          id: `issue-${issue.id}`,
          group: "Issues",
          label: issue.title,
          hint: identifier(issue, teamsById),
          run: () =>
            void navigate({
              to: "/$workspace/issue/$identifier",
              params: { workspace: workspace.url_key, identifier: identifier(issue, teamsById) },
            }),
        }),
      );
    return [...found, ...findKnowledge(q), ...commandsFound];
  }, [commands, issues, q, teamsById, navigate, workspace.url_key, findKnowledge]);
}
