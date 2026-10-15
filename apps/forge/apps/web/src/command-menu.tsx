import { Dialog } from "@base-ui/react/dialog";
import { useNavigate, useParams } from "@tanstack/react-router";
import {
  Bot,
  Box,
  CircleUserRound,
  Folders,
  Funnel,
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
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { signOut } from "./session";
import { useRecords, useWorkspace } from "./workspace";

interface Command {
  id: string;
  group: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
  run: () => void;
}

/** ⌘K: jump anywhere, find issues, or act on the open issue. */
/** Opens the command menu, as ⌘K does. */
export function openCommandMenu() {
  window.dispatchEvent(new Event("forge:command-menu"));
}

export function CommandMenu() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setOpen((o) => !o);
      }
    };
    const onOpen = () => setOpen(true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("forge:command-menu", onOpen);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("forge:command-menu", onOpen);
    };
  }, []);

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-40 bg-black/40 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <Dialog.Popup className="fixed top-[14vh] left-1/2 z-40 w-[min(36rem,calc(100vw-2rem))] -translate-x-1/2 overflow-hidden rounded-xl border border-line bg-surface shadow-2xl shadow-black/30 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
          <Dialog.Title className="sr-only">Command menu</Dialog.Title>
          {open && <Commands onDone={() => setOpen(false)} />}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Commands({ onDone }: { onDone: () => void }) {
  const { sync, me, workspace } = useWorkspace();
  const navigate = useNavigate();
  const newIssue = useNewIssue();
  const teams = useTeams();
  const teamsById = useMemo(() => byId(teams), [teams]);
  const issues = useRecords("Issue");
  const projects = useRecords("Project");
  const params = useParams({ strict: false }) as { identifier?: string };
  const current = useIssueByIdentifier(params.identifier ?? "");
  const currentStates = useTeamStates(current?.team_id);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

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
          ["account", "Account", UserRound, "/$workspace/settings/account"],
        ] as const
      ).map(([id, label, icon, to]) => ({
        id,
        group: "Settings",
        label,
        icon: <NavIcon icon={icon} />,
        run: go(to),
      })),
      ...me.workspaces
        .filter((w) => w.id !== workspace.id)
        .map((w) => ({
          id: `workspace-${w.id}`,
          group: "Workspaces",
          label: `Switch to ${w.name}`,
          run: () => void navigate({ to: "/$workspace", params: { workspace: w.url_key } }),
        })),
      { id: "sign-out", group: "Account", label: "Sign out", run: () => void signOut() },
    );
    return list;
  }, [current, currentStates, teams, teamsById, projects, me, workspace, sync, navigate, newIssue]);

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

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    const matching = q ? commands.filter((c) => c.label.toLowerCase().includes(q)) : commands;
    if (!q) return matching;
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
    return [...found, ...findKnowledge(q), ...matching];
  }, [commands, issues, query, teamsById, navigate, workspace.url_key, findKnowledge]);

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const run = (command: Command | undefined) => {
    if (!command) return;
    onDone();
    command.run();
  };

  let lastGroup = "";
  return (
    <div>
      <input
        // biome-ignore lint/a11y/noAutofocus: the menu opens to type into it
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((i) => Math.min(i + 1, results.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => Math.max(i - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            run(results[active]);
          }
        }}
        placeholder="Type a command, or search issues, specs and more…"
        aria-label="Command"
        role="combobox"
        aria-expanded="true"
        aria-controls="command-list"
        aria-activedescendant={results[active] ? `command-${results[active].id}` : undefined}
        className="h-12 w-full border-b border-line bg-transparent px-4 text-[15px] outline-none placeholder:text-faint"
      />
      <div
        ref={listRef}
        id="command-list"
        role="listbox"
        className="max-h-96 overflow-y-auto p-1.5"
      >
        {results.length === 0 && <p className="px-3 py-6 text-center text-muted">No results</p>}
        {results.map((command, index) => {
          const header = command.group !== lastGroup ? command.group : null;
          lastGroup = command.group;
          return (
            <div key={command.id}>
              {header && <div className="px-2.5 pt-2 pb-1 text-xs text-faint">{header}</div>}
              <div
                id={`command-${command.id}`}
                role="option"
                aria-selected={index === active}
                tabIndex={-1}
                data-index={index}
                onMouseMove={() => setActive(index)}
                onClick={() => run(command)}
                onKeyDown={() => {}}
                className={`flex h-9 cursor-default items-center gap-2.5 rounded-md px-2.5 ${index === active ? "bg-hover" : ""}`}
              >
                {command.icon}
                <span className="truncate">{command.label}</span>
                {command.hint && (
                  <span className="ml-auto font-mono text-xs text-faint">{command.hint}</span>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
