import { Menu } from "@base-ui/react/menu";
import {
  AppVersion,
  type Command,
  CommandMenu,
  matching,
  Redirect,
  SearchButton,
  Shortcuts,
  ShortcutsButton,
  SyncIndicator,
} from "@code-villain/web";
import {
  Link,
  Outlet,
  useLocation,
  useNavigate,
  useParams,
  useRouter,
} from "@tanstack/react-router";
import { ArrowLeft, Check, ChevronsUpDown, Inbox, Mail } from "lucide-react";
import { type ReactNode, useMemo } from "react";
import { InboxProvider, useUnread } from "./mail";
import { isDesktop } from "./platform";
import { setCurrentPlace } from "./server";
import {
  defaultWorkspace,
  loadSession,
  rememberWorkspace,
  useSession,
  workspaceEntries,
} from "./session";
import { AuthScreen, Button, Initial, NavIcon, Splash } from "./ui";
import { useSyncStatus, useWorkspace, WorkspaceProvider } from "./workspace";

/**
 * `/`: opens the workspace used last. Postrider runs on this Mac, where
 * nobody signs in; with no answer from it, it says so.
 */
export function Home() {
  const session = useSession();
  if (session.status === "loading") return <Splash />;
  const workspace =
    session.status === "signed-in" ? defaultWorkspace(workspaceEntries(session.places)) : null;
  if (!workspace) return <NotStarted />;
  return <Redirect to="/$workspace" params={{ workspace: workspace.key }} />;
}

/** Postrider's server on this Mac didn't answer. */
function NotStarted() {
  return (
    <AuthScreen title="Postrider didn't start">
      <p className="mb-6 text-center text-muted">
        {isDesktop
          ? "Its server on this Mac didn't answer. Quit Postrider and open it again."
          : "Can't reach Postrider. Start it with `bun run dev`, then try again."}
      </p>
      <Button variant="secondary" className="w-full" onClick={() => void loadSession()}>
        Try again
      </Button>
    </AuthScreen>
  );
}

/** `/$workspace/...`: the sidebar around the page. */
export function WorkspaceLayout() {
  const session = useSession();
  const { workspace: urlKey } = useParams({ strict: false }) as { workspace: string };

  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") return <Redirect to="/" />;
  const entry = workspaceEntries(session.places).find((e) => e.key === urlKey);
  if (!entry) return <NotFound />;
  rememberWorkspace(entry.key);
  setCurrentPlace(entry.place);

  return (
    <WorkspaceProvider
      place={entry.place}
      me={entry.me}
      workspace={entry.workspace}
      routeKey={entry.key}
    >
      <InboxProvider>
        <Shell />
      </InboxProvider>
    </WorkspaceProvider>
  );
}

export function NotFound() {
  return (
    <AuthScreen title="Nothing here">
      <p className="mb-6 text-center text-muted">This page doesn't exist.</p>
      <Link to="/" className="ui-link block text-center">
        Go to your Inbox
      </Link>
    </AuthScreen>
  );
}

/**
 * The bar at the top of each page: its title, then the page's controls.
 * In the desktop app the native title bar is hidden, so this bar and the
 * sidebar's top row are what the window is moved by.
 */
export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header
      className="app-topbar flex min-h-12 shrink-0 items-center gap-3 px-2"
      data-tauri-drag-region="deep"
    >
      <h1 className="truncate text-xs font-semibold">{title}</h1>
      <div className="ml-auto flex min-w-0 items-center gap-3">{children}</div>
    </header>
  );
}

/** The page's body, scrolling under the header. */
export function PageBody({ children }: { children: ReactNode }) {
  return <div className="page-content min-h-0 flex-1 overflow-y-auto">{children}</div>;
}

/** The workspace frame: sidebar and page. */
function Shell() {
  return (
    <>
      <div className="app-shell flex h-full bg-bg">
        <Sidebar />
        <main className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-bg">
          <Outlet />
        </main>
      </div>
      <Menus />
    </>
  );
}

// Items read in `fg`; their icons stay `muted`, as in every app.
const navLinkBase =
  "nav-link flex h-7 items-center gap-2.5 rounded-md px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent [&.active]:bg-selected [&.active]:font-bold [&.active]:text-fg [&_svg]:text-muted";
const navLink = `${navLinkBase} text-fg`;

function Sidebar() {
  const { pathname } = useLocation();
  const inSettings = pathname.includes("/settings/");
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
              <span className="font-display text-wordmark font-bold">Postrider</span>
            </div>
          )}
          <SearchButton />
        </div>
        <div className="workspace-switcher">
          <WorkspaceMenu />
        </div>
        {inSettings ? <SettingsNav /> : <MailNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator status={useSyncStatus()} refusals={useWorkspace().refusals} />
        <ShortcutsButton />
      </div>
    </aside>
  );
}

/** The Inbox, with what's unread in it. */
function MailNav() {
  const { workspace } = useWorkspace();
  const unread = useUnread();
  return (
    <nav aria-label="Mail" className="flex flex-col gap-0.5">
      <Link
        to="/$workspace"
        params={{ workspace: workspace.url_key }}
        activeOptions={{ exact: true }}
        className={navLink}
        title="Inbox (G then I)"
      >
        <NavIcon icon={Inbox} />
        Inbox
        {unread > 0 && (
          <span className="nav-count ml-auto flex items-center gap-1.5 text-xs font-normal text-muted tabular-nums">
            <span className="ui-live-dot" aria-hidden="true" />
            <span aria-label={`${unread} unread`}>{unread}</span>
          </span>
        )}
      </Link>
    </nav>
  );
}

/** Settings take the sidebar's place, with a way back. */
function SettingsNav() {
  const { workspace } = useWorkspace();
  const router = useRouter();
  const back = `/${workspace.url_key}`;
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
        Back to Inbox
      </a>
      <nav aria-label="Settings" className="flex flex-col gap-0.5">
        <span className="px-2 pb-1 text-xs text-faint">Settings</span>
        <Link
          to="/$workspace/settings/accounts"
          params={{ workspace: workspace.url_key }}
          className={navLink}
        >
          <NavIcon icon={Mail} />
          Accounts
        </Link>
      </nav>
      <AppVersion name="Postrider" />
    </>
  );
}

function WorkspaceMenu() {
  const { place, workspace } = useWorkspace();
  const session = useSession();
  const navigate = useNavigate();
  const entries = session.status === "signed-in" ? workspaceEntries(session.places) : [];
  const item =
    "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none";

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
            {entries.length > 1 && (
              <>
                {entries.map((entry) => (
                  <Menu.Item
                    key={entry.key}
                    className={item}
                    onClick={() =>
                      void navigate({ to: "/$workspace", params: { workspace: entry.key } })
                    }
                  >
                    <Initial name={entry.workspace.name} square />
                    <span className="truncate">{entry.workspace.name}</span>
                    {entry.place === place && entry.workspace.id === workspace.id && (
                      <Check
                        size={15}
                        strokeWidth={1.75}
                        aria-label="Open"
                        className="ml-auto shrink-0 text-muted"
                      />
                    )}
                  </Menu.Item>
                ))}
                <Menu.Separator className="my-1 h-px bg-line" />
              </>
            )}
            <Menu.Item
              className={item}
              onClick={() =>
                void navigate({
                  to: "/$workspace/settings/accounts",
                  params: { workspace: workspace.url_key },
                })
              }
            >
              Settings
            </Menu.Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

/** ⌘K and the shortcuts list: the Inbox, settings, and the other workspaces. */
function Menus() {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const params = { workspace: workspace.url_key };
  return (
    <>
      <CommandMenu useCommands={useCommands} placeholder="Type a command…" />
      <Shortcuts
        goTo={[{ key: "i", name: "Inbox", go: () => void navigate({ to: "/$workspace", params }) }]}
        onSettings={
          isDesktop
            ? () => void navigate({ to: "/$workspace/settings/accounts", params })
            : undefined
        }
      />
    </>
  );
}

function useCommands(query: string): Command[] {
  const { workspace } = useWorkspace();
  const session = useSession();
  const navigate = useNavigate();

  const commands = useMemo(() => {
    const params = { workspace: workspace.url_key };
    const list: Command[] = [
      {
        id: "inbox",
        group: "Go to",
        label: "Inbox",
        hint: "G I",
        icon: <NavIcon icon={Inbox} />,
        run: () => void navigate({ to: "/$workspace", params }),
      },
      {
        id: "settings-accounts",
        group: "Settings",
        label: "Accounts",
        icon: <NavIcon icon={Mail} />,
        run: () => void navigate({ to: "/$workspace/settings/accounts", params }),
      },
    ];
    for (const entry of session.status === "signed-in" ? workspaceEntries(session.places) : []) {
      if (entry.key === workspace.url_key) continue;
      list.push({
        id: `workspace-${entry.key}`,
        group: "Workspaces",
        label: `Switch to ${entry.workspace.name}`,
        run: () => void navigate({ to: "/$workspace", params: { workspace: entry.key } }),
      });
    }
    return list;
  }, [workspace, session, navigate]);

  return matching(commands, query);
}
