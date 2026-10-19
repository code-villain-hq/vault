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
  ArrowLeftRight,
  BookA,
  ChevronsUpDown,
  Cpu,
  Gauge,
  Globe,
  Languages,
  ListChecks,
  UserRound,
  UsersRound,
} from "lucide-react";
import { type ReactNode, useEffect } from "react";
import { Initial, NavIcon } from "./icons";
import { isDesktop } from "./platform";
import { Redirect } from "./redirect";
import { defaultWorkspace, rememberWorkspace, signOut, useSession } from "./session";
import { SyncIndicator } from "./sync-indicator";
import { AuthScreen, Button, Splash } from "./ui";
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
      <Link to="/" className="ui-link block text-center">
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
  parent,
  meta,
  children,
}: {
  title: string;
  /** The page this one sits in, shown before the title. */
  parent?: ReactNode;
  meta?: ReactNode;
  children?: ReactNode;
}) {
  const { workspace } = useWorkspace();
  return (
    <>
      {/* In the desktop app the native title bar is hidden, so this bar and the
          sidebar's top row are what the window is moved by. */}
      <header
        className="app-topbar @container flex min-h-12 shrink-0 items-center gap-3 px-4"
        data-tauri-drag-region="deep"
      >
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
      <h1 className="sr-only">{title}</h1>
    </>
  );
}

/** The pages ⌘1 to ⌘5 open, in the sidebar's order. */
const SHORTCUT_PAGES = [
  "/$workspace",
  "/$workspace/translations",
  "/$workspace/review",
  "/$workspace/glossary",
  "/$workspace/import",
] as const;

/** The workspace frame: sidebar and page. */
function Shell() {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      const page = SHORTCUT_PAGES[Number(event.key) - 1];
      if (!page) return;
      event.preventDefault();
      void navigate({ to: page, params: { workspace: workspace.url_key } });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, workspace.url_key]);
  return (
    <div className="app-shell flex h-full bg-bg">
      <Sidebar />
      <main className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-bg">
        <Outlet />
      </main>
    </div>
  );
}

// Items read in `fg`, as Layer's task list does; their icons stay `muted`.
const navLinkBase =
  "nav-link flex h-7 items-center gap-2.5 rounded-md px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent [&.active]:bg-selected [&.active]:font-bold [&.active]:text-fg [&_svg]:text-muted";
const navLink = `${navLinkBase} text-fg`;

/** The last page outside settings, where "Back to app" returns. */
let lastAppPath: string | null = null;

function Sidebar() {
  const { pathname } = useLocation();
  const inSettings = pathname.includes("/settings/");
  if (!inSettings) lastAppPath = pathname;
  return (
    <aside
      className="app-sidebar flex w-56 shrink-0 flex-col gap-4 overflow-y-auto px-2 pb-2"
      aria-label="Sidebar"
    >
      <div className="sidebar-navigation flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto">
        {/* The top row lines up with the page's toolbar. In the desktop app
            macOS draws the traffic lights here, in the wordmark's place. */}
        <div
          className="sidebar-heading sticky top-0 z-10 flex h-12 shrink-0 items-center gap-1 bg-bg"
          data-tauri-drag-region="deep"
        >
          {isDesktop ? (
            <div className="min-w-0 flex-1" />
          ) : (
            <div className="app-brand flex h-9 min-w-0 flex-1 items-center gap-2 px-2">
              <span className="font-display text-wordmark font-bold">Glyph</span>
            </div>
          )}
        </div>
        <div className="workspace-switcher">
          <WorkspaceMenu />
        </div>
        {inSettings ? <SettingsNav /> : <AppNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator />
      </div>
    </aside>
  );
}

/** The views, in the order ⌘1 to ⌘5 open them. */
function AppNav() {
  const { workspace } = useWorkspace();
  const params = { workspace: workspace.url_key };
  const waiting = useRecords("Suggestion").length;
  return (
    <nav aria-label="Views" className="flex flex-col gap-0.5">
      <Link
        to="/$workspace"
        params={params}
        activeOptions={{ exact: true }}
        className={navLink}
        title="Overview (⌘1)"
      >
        <NavIcon icon={Gauge} />
        Overview
      </Link>
      <Link
        to="/$workspace/translations"
        params={params}
        className={navLink}
        title="Translations (⌘2)"
      >
        <NavIcon icon={Languages} />
        Translations
      </Link>
      <Link to="/$workspace/review" params={params} className={navLink} title="Review (⌘3)">
        <NavIcon icon={ListChecks} />
        Review
        {waiting > 0 && (
          <span className="nav-count ml-auto text-xs font-normal text-faint">{waiting}</span>
        )}
      </Link>
      <Link to="/$workspace/glossary" params={params} className={navLink} title="Glossary (⌘4)">
        <NavIcon icon={BookA} />
        Glossary
      </Link>
      <Link
        to="/$workspace/import"
        params={params}
        className={navLink}
        title="Import & export (⌘5)"
      >
        <NavIcon icon={ArrowLeftRight} />
        Import & export
      </Link>
    </nav>
  );
}

/** Settings take the sidebar's place, with a way back. */
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
        className={`${navLinkBase} text-muted`}
      >
        <NavIcon icon={ArrowLeft} />
        Back to app
      </a>
      <nav aria-label="Workspace settings" className="flex flex-col gap-0.5">
        <span className="px-2 pb-1 text-xs text-faint">Workspace</span>
        <Link to="/$workspace/settings/languages" params={params} className={navLink}>
          <NavIcon icon={Globe} />
          Languages
        </Link>
        <Link to="/$workspace/settings/models" params={params} className={navLink}>
          <NavIcon icon={Cpu} />
          Models
        </Link>
        <Link to="/$workspace/settings/members" params={params} className={navLink}>
          <NavIcon icon={UsersRound} />
          Members
        </Link>
      </nav>
      <nav aria-label="Personal settings" className="flex flex-col gap-0.5">
        <span className="px-2 pb-1 text-xs text-faint">You</span>
        <Link to="/$workspace/settings/account" params={params} className={navLink}>
          <NavIcon icon={UserRound} />
          Account
        </Link>
      </nav>
    </>
  );
}

function WorkspaceMenu() {
  const { me, workspace } = useWorkspace();
  const navigate = useNavigate();
  const others = me.workspaces.filter((w) => w.id !== workspace.id);
  const item =
    "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none";
  const settings = (page: "languages" | "members") =>
    void navigate({
      to: `/$workspace/settings/${page}`,
      params: { workspace: workspace.url_key },
    });

  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label={`Workspace: ${workspace.name}`}
        className="flex h-7 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left text-xs font-medium text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover"
      >
        <Initial name={workspace.name} square />
        <span className="truncate">{workspace.name}</span>
        <ChevronsUpDown
          size={15}
          strokeWidth={1.75}
          aria-hidden="true"
          className="ml-auto shrink-0 text-faint"
        />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start">
          <Menu.Popup className="ui-menu min-w-52 origin-(--transform-origin) p-1 outline-none transition-[scale,opacity] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
            <div className="truncate px-2 py-1.5 text-xs text-faint">{me.user.email}</div>
            {others.map((other) => (
              <Menu.Item
                key={other.id}
                className={item}
                onClick={() =>
                  void navigate({ to: "/$workspace", params: { workspace: other.url_key } })
                }
              >
                <Initial name={other.name} square />
                {other.name}
              </Menu.Item>
            ))}
            <Menu.Separator className="my-1 h-px bg-line" />
            <Menu.Item className={item} onClick={() => settings("languages")}>
              Settings
            </Menu.Item>
            <Menu.Item className={item} onClick={() => settings("members")}>
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
