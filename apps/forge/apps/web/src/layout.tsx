import { Menu } from "@base-ui/react/menu";
import {
  Link,
  Outlet,
  useLocation,
  useNavigate,
  useParams,
  useRouter,
} from "@tanstack/react-router";
import {
  ArrowLeft,
  Bot,
  Box,
  ChevronDown,
  CircleHelp,
  CircleUserRound,
  Folders,
  Funnel,
  GitPullRequest,
  Inbox,
  Layers,
  Mountain,
  Scale,
  Search,
  Server,
  SquarePen,
  Tag,
  UserRound,
  UsersRound,
} from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { CommandMenu, openCommandMenu } from "./command-menu";
import { useStates, useTeams } from "./data";
import { NavIcon, ServiceIcon } from "./icons";
import { useDesktopNotifications, useUnreadCount } from "./inbox";
import { type NewIssueDefaults, NewIssueProvider, useNewIssue } from "./new-issue";
import { Redirect } from "./redirect";
import { defaultWorkspace, rememberWorkspace, signOut, useSession } from "./session";
import { openShortcuts, Shortcuts } from "./shortcuts";
import { SyncIndicator } from "./sync-indicator";
import { AuthScreen, Button, Logo, Splash } from "./ui";
import { useRecords, useWorkspace, WorkspaceProvider } from "./workspace";

/** `/`: opens the workspace used last, or asks to sign in. */
export function Home() {
  const session = useSession();
  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") return <Redirect to="/login" />;
  const workspace = defaultWorkspace(session.me);
  if (!workspace) {
    return (
      <AuthScreen title="You're not in any workspace">
        <p className="mb-6 text-center text-muted">Ask a workspace admin for an invite link.</p>
        <Button variant="secondary" className="w-full" onClick={() => void signOut()}>
          Sign out
        </Button>
      </AuthScreen>
    );
  }
  return <Redirect to="/$workspace" params={{ workspace: workspace.url_key }} />;
}

/** `/$workspace/...`: needs a signed-in member; shows the sidebar around the page. */
export function WorkspaceLayout() {
  const session = useSession();
  const location = useLocation();
  const { workspace: urlKey } = useParams({ strict: false }) as { workspace: string };

  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") {
    return <Redirect to="/login" search={{ redirect: location.href }} />;
  }
  const workspace = session.me.workspaces.find((w) => w.url_key === urlKey);
  if (!workspace) return <NotFound />;
  rememberWorkspace(workspace.url_key);

  return (
    <WorkspaceProvider me={session.me} workspace={workspace}>
      <Shell />
    </WorkspaceProvider>
  );
}

export function NotFound() {
  return (
    <AuthScreen title="Nothing here">
      <p className="mb-6 text-center text-muted">
        This page doesn't exist, or you're not a member of its workspace.
      </p>
      <Link to="/" className="block text-center text-fg underline-offset-4 hover:underline">
        Go to your workspace
      </Link>
    </AuthScreen>
  );
}

/**
 * The bar at the top of each workspace page: the title, anything shown by
 * it (`meta`), then the page's controls.
 */
export function PageHeader({
  title,
  headingTitle,
  parent,
  meta,
  children,
}: {
  title: string;
  /** The page heading when the toolbar uses a shorter identifier. */
  headingTitle?: string;
  /** The page this one sits in, shown before the title, e.g. Specs for a domain. */
  parent?: ReactNode;
  meta?: ReactNode;
  children?: ReactNode;
}) {
  const { workspace } = useWorkspace();
  return (
    <>
      <header className="app-topbar @container flex min-h-11 shrink-0 items-center gap-3 px-4">
        <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-xs text-muted">
          {parent ?? workspace.name}
          <span aria-hidden="true" className="text-faint">
            ›
          </span>
          <span className="truncate font-medium text-fg" aria-current="page">
            {title}
          </span>
        </nav>
        {meta && <div className="flex min-w-0 items-center gap-3 overflow-hidden">{meta}</div>}
        <div className="ml-auto flex min-w-0 items-center gap-3 overflow-x-auto py-1 [scrollbar-width:none]">
          {children}
        </div>
      </header>
      <h1 className="sr-only">{headingTitle ?? title}</h1>
    </>
  );
}

/** The workspace frame: sidebar, page, new issue dialog and command menu. */
function Shell() {
  const params = useParams({ strict: false }) as { team?: string; project?: string };
  const teams = useTeams();
  // "c" creates an issue in the team or project that's open.
  const pageDefaults = useMemo<NewIssueDefaults>(
    () => ({
      teamId: teams.find((t) => t.key === params.team)?.id,
      projectId: params.project,
    }),
    [teams, params.team, params.project],
  );
  useDesktopNotifications();
  return (
    <NewIssueProvider pageDefaults={pageDefaults}>
      <div className="app-shell flex h-full bg-bg">
        <Sidebar />
        <div className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-panel">
          <Outlet />
        </div>
      </div>
      <CommandMenu />
      <Shortcuts />
    </NewIssueProvider>
  );
}

const navLink =
  "nav-link flex h-7 items-center gap-2.5 rounded-md px-2 text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent [&.active]:bg-selected [&.active]:font-bold [&.active]:text-fg [&_svg]:text-current";

const iconButton =
  "flex size-7 shrink-0 items-center justify-center rounded-md text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent";

/** A sidebar icon, in Lucide's style. */
/** The last page outside settings, where "Back to app" returns. */
let lastAppPath: string | null = null;

function Sidebar() {
  const { pathname } = useLocation();
  const inSettings = pathname.includes("/settings/");
  if (!inSettings) lastAppPath = pathname;
  return (
    <aside
      className="app-sidebar flex w-56 shrink-0 flex-col gap-4 overflow-y-auto p-2"
      aria-label="Sidebar"
    >
      <div className="sidebar-navigation flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto">
        <div className="sidebar-heading flex shrink-0 items-center gap-1">
          <div className="app-brand flex h-9 min-w-0 flex-1 items-center gap-2 px-2">
            <Logo />
            <span className="font-display text-[28px] leading-[1.18] font-bold">Forge</span>
          </div>
          <SidebarActions />
        </div>
        <div className="workspace-switcher">
          <WorkspaceMenu />
        </div>
        {inSettings ? <SettingsNav /> : <AppNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator />
        <button
          type="button"
          aria-label="Keyboard shortcuts"
          title="Keyboard shortcuts (?)"
          onClick={openShortcuts}
          className={`${iconButton} ml-auto size-6`}
        >
          <CircleHelp size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </div>
    </aside>
  );
}

/** Global actions stay beside the app identity, above the workspace picker. */
function SidebarActions() {
  const newIssue = useNewIssue();
  return (
    <div className="sidebar-actions flex items-center gap-1">
      <button
        type="button"
        aria-label="Search and commands"
        title="Search and commands (⌘K)"
        onClick={openCommandMenu}
        className={iconButton}
      >
        <Search size={15} strokeWidth={1.75} aria-hidden="true" />
      </button>
      <button
        type="button"
        aria-label="New issue"
        title="New issue (C)"
        onClick={() => newIssue()}
        className={iconButton}
      >
        <SquarePen size={15} strokeWidth={1.75} aria-hidden="true" />
      </button>
    </div>
  );
}

/** Where people work: their inbox and issues, the workspace, its knowledge and teams. */
function AppNav() {
  const { workspace } = useWorkspace();
  const params = { workspace: workspace.url_key };
  const teams = useTeams();
  const triage = useTriageCount();
  const unread = useUnreadCount();
  return (
    <>
      <nav className="flex flex-col gap-0.5">
        <Link to="/$workspace/inbox" params={params} className={navLink}>
          <NavIcon icon={Inbox} />
          Inbox
          {unread > 0 && (
            <span className="nav-count ml-auto flex items-center gap-1.5 text-xs font-normal text-faint">
              <span className="size-1.5 rounded-full bg-accent" aria-hidden="true" />
              {unread}
            </span>
          )}
        </Link>
        <Link to="/$workspace/my-issues" params={params} className={navLink}>
          <NavIcon icon={CircleUserRound} />
          My issues
        </Link>
        <Link to="/$workspace/triage" params={params} className={navLink}>
          <NavIcon icon={Funnel} />
          Triage
          {triage > 0 && (
            <span className="nav-count ml-auto text-xs font-normal text-faint">{triage}</span>
          )}
        </Link>
      </nav>
      <Section id="workspace" title="Workspace">
        <Link to="/$workspace/projects" params={params} className={navLink}>
          <NavIcon icon={Box} />
          Projects
        </Link>
        <Link to="/$workspace/initiatives" params={params} className={navLink}>
          <NavIcon icon={Mountain} />
          Initiatives
        </Link>
      </Section>
      <Section id="knowledge" title="Knowledge">
        <Link to="/$workspace/services" params={params} className={navLink}>
          <NavIcon icon={Server} />
          Services
        </Link>
        <DomainsNav />
        <Link to="/$workspace/decisions" params={params} className={navLink}>
          <NavIcon icon={Scale} />
          Decisions
        </Link>
      </Section>
      <Section id="teams" title="Teams">
        {teams.map((team) => (
          <Link
            key={team.id}
            to="/$workspace/team/$team"
            params={{ ...params, team: team.key }}
            className={navLink}
          >
            <span className="flex size-[15px] items-center justify-center rounded bg-hover text-[9px] font-semibold text-muted">
              {team.key.charAt(0)}
            </span>
            {team.name}
          </Link>
        ))}
      </Section>
    </>
  );
}

/**
 * Domains, which unfold to each domain so you can go straight to one; folded
 * or not is remembered on this device.
 */
function DomainsNav() {
  const { workspace } = useWorkspace();
  const domains = [...useRecords("Domain")].sort((a, b) => a.name.localeCompare(b.name));
  const key = "forge:sidebar:domains-open";
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem(key) === "open";
    } catch {
      return false;
    }
  });
  const toggle = () => {
    setOpen(!open);
    try {
      localStorage.setItem(key, open ? "closed" : "open");
    } catch {
      // Not remembered, then.
    }
  };
  const params = { workspace: workspace.url_key };
  return (
    <>
      <div className="group relative">
        <Link
          to="/$workspace/domains"
          params={params}
          activeOptions={{ exact: true }}
          className={navLink}
        >
          <NavIcon icon={Folders} />
          Domains
        </Link>
        {domains.length > 0 && (
          <button
            type="button"
            aria-label={open ? "Fold domains" : "Show domains"}
            aria-expanded={open}
            onClick={toggle}
            className="absolute top-1/2 right-1 flex size-5 -translate-y-1/2 items-center justify-center rounded text-faint outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent"
          >
            <ChevronDown
              size={13}
              strokeWidth={2}
              aria-hidden="true"
              className={`transition-transform ${open ? "" : "-rotate-90"}`}
            />
          </button>
        )}
      </div>
      {open &&
        domains.map((domain) => (
          <Link
            key={domain.id}
            to="/$workspace/domains/$domain"
            params={{ ...params, domain: domain.id }}
            className={`${navLink} pl-[2.1rem]`}
          >
            <ServiceIcon service={domain} className="size-3.5" />
            <span className="truncate">{domain.name}</span>
          </Link>
        ))}
    </>
  );
}

/** Settings take the sidebar's place, as in Linear, with a way back. */
function SettingsNav() {
  const { workspace } = useWorkspace();
  const router = useRouter();
  const params = { workspace: workspace.url_key };
  const back = lastAppPath ?? `/${workspace.url_key}`;
  return (
    <>
      <a
        href={back}
        onClick={(event) => {
          event.preventDefault();
          router.history.push(back);
        }}
        className={`${navLink} h-9 text-muted`}
      >
        <NavIcon icon={ArrowLeft} />
        Back to app
      </a>
      <nav className="flex flex-col gap-0.5">
        <span className="px-2 pb-1 text-xs text-faint">Workspace</span>
        <Link to="/$workspace/settings/members" params={params} className={navLink}>
          <NavIcon icon={UsersRound} />
          Members
        </Link>
        <Link to="/$workspace/settings/teams" params={params} className={navLink}>
          <NavIcon icon={Layers} />
          Teams
        </Link>
        <Link to="/$workspace/settings/labels" params={params} className={navLink}>
          <NavIcon icon={Tag} />
          Labels
        </Link>
        <Link to="/$workspace/settings/agents" params={params} className={navLink}>
          <NavIcon icon={Bot} />
          Agents
        </Link>
        <Link to="/$workspace/settings/github" params={params} className={navLink}>
          <NavIcon icon={GitPullRequest} />
          GitHub
        </Link>
      </nav>
      <nav className="flex flex-col gap-0.5">
        <span className="px-2 pb-1 text-xs text-faint">You</span>
        <Link to="/$workspace/settings/account" params={params} className={navLink}>
          <NavIcon icon={UserRound} />
          Account
        </Link>
      </nav>
    </>
  );
}

/** A heading that folds its links away; remembered on this device. */
function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  const key = `forge:sidebar:${id}`;
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem(key) !== "closed";
    } catch {
      return true;
    }
  });
  const toggle = () => {
    setOpen(!open);
    try {
      localStorage.setItem(key, open ? "closed" : "open");
    } catch {
      // Not remembered, then.
    }
  };
  return (
    <nav className="sidebar-section flex flex-col gap-0.5">
      <button
        type="button"
        aria-expanded={open}
        onClick={toggle}
        className="sidebar-group-label group flex h-6 items-center gap-1 self-start rounded px-2 text-xs text-faint outline-none hover:text-muted focus-visible:ring-2 focus-visible:ring-accent"
      >
        {title}
        <ChevronDown
          size={12}
          strokeWidth={2}
          aria-hidden="true"
          className={`transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      <div className={`sidebar-section-items flex-col gap-0.5 ${open ? "flex" : "hidden"}`}>
        {children}
      </div>
    </nav>
  );
}

function useTriageCount() {
  const issues = useRecords("Issue");
  const states = useStates();
  const suggestions = useRecords("Suggestion");
  return useMemo(
    () =>
      issues.filter((i) => states.get(i.state_id)?.type === "triage").length +
      // Triage suggestions belong to an issue already counted.
      suggestions.filter((s) => s.status === "open" && s.kind === "drift").length,
    [issues, states, suggestions],
  );
}

function WorkspaceMenu() {
  const { me, workspace } = useWorkspace();
  const navigate = useNavigate();
  const others = me.workspaces.filter((w) => w.id !== workspace.id);
  const item =
    "flex cursor-default items-center gap-2 rounded px-2 py-1.5 outline-none select-none data-highlighted:bg-hover";

  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label={`Workspace: ${workspace.name}`}
        className="flex h-7 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left text-xs font-medium text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover"
      >
        <WorkspaceBadge name={workspace.name} />
        <span className="truncate">{workspace.name}</span>
        <svg
          aria-hidden="true"
          viewBox="0 0 16 16"
          className="ml-auto size-3.5 shrink-0 text-faint"
          fill="currentColor"
        >
          <path d="M11 10H5l3 3.5zm0-4H5l3-3.5z" />
        </svg>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start">
          <Menu.Popup className="app-menu min-w-52 origin-(--transform-origin) rounded-md bg-surface p-1 outline-none transition-[scale,opacity] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
            <div className="truncate px-2 py-1.5 text-xs text-faint">{me.user.email}</div>
            {others.map((other) => (
              <Menu.Item
                key={other.id}
                className={item}
                onClick={() =>
                  void navigate({ to: "/$workspace", params: { workspace: other.url_key } })
                }
              >
                <WorkspaceBadge name={other.name} />
                {other.name}
              </Menu.Item>
            ))}
            <Menu.Separator className="my-1 h-px bg-line" />
            <Menu.Item
              className={item}
              onClick={() =>
                void navigate({
                  to: "/$workspace/settings/members",
                  params: { workspace: workspace.url_key },
                })
              }
            >
              Settings
            </Menu.Item>
            <Menu.Item
              className={item}
              onClick={() =>
                void navigate({
                  to: "/$workspace/settings/members",
                  params: { workspace: workspace.url_key },
                })
              }
            >
              Invite people
            </Menu.Item>
            <Menu.Item className={item} onClick={() => void signOut()}>
              Sign out
            </Menu.Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

function WorkspaceBadge({ name }: { name: string }) {
  return (
    <span className="flex size-5 shrink-0 items-center justify-center rounded bg-primary text-[12px] font-semibold text-on-primary">
      {name.trim().charAt(0).toUpperCase()}
    </span>
  );
}
