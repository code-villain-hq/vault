import { Menu } from "@base-ui/react/menu";
import {
  AppVersion,
  Redirect,
  SearchButton,
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
import {
  ArrowLeft,
  Check,
  ChevronsUpDown,
  FolderGit2,
  Plug,
  UserRound,
  UsersRound,
} from "lucide-react";
import type { ReactNode } from "react";
import { LayerMenus } from "./commands";
import { Initial, NavIcon } from "./icons";
import { isDesktop } from "./platform";
import { bundled, type Place, placeName, setCurrentPlace } from "./server";
import {
  defaultWorkspace,
  rememberWorkspace,
  signOut,
  useSession,
  type WorkspaceEntry,
  workspaceEntries,
} from "./session";
import { AuthScreen, Button, Splash } from "./ui";
import { useSyncStatus, useWorkspace, WorkspaceProvider } from "./workspace";

/**
 * `/`: opens the workspace used last, or asks to sign in.
 */
export function Home() {
  const session = useSession();
  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") return <Redirect to="/login" />;
  const workspace = defaultWorkspace(workspaceEntries(session.places));
  if (!workspace) {
    const signOutEverywhere = () =>
      void Promise.all(session.places.map(({ place }) => signOut(place)));
    return (
      <AuthScreen title="You're not in any workspace">
        <p className="mb-6 text-center text-muted">Ask a workspace admin for an invite link.</p>
        <Button variant="secondary" className="w-full" onClick={signOutEverywhere}>
          Sign out
        </Button>
      </AuthScreen>
    );
  }
  return <Redirect to="/$workspace" params={{ workspace: workspace.key }} />;
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
      <Shell />
    </WorkspaceProvider>
  );
}

/** `/$workspace`: its repositories, which is what a team shares most. */
export function WorkspaceHome() {
  const { workspace } = useWorkspace();
  return <Redirect to="/$workspace/repositories" params={{ workspace: workspace.url_key }} />;
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
 * The bar at the top of each page: where it sits (the workspace, unless
 * said), its title, anything shown by it (`meta`), then the page's controls.
 */
export function PageHeader({
  title,
  parent,
  meta,
  children,
}: {
  title: string;
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

/** The page's body, scrolling under the header. */
export function PageBody({ children }: { children: ReactNode }) {
  return (
    <div className="page-content min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex max-w-5xl flex-col gap-8 px-6 py-6">{children}</div>
    </div>
  );
}

/** Where a place is, in a sentence: "layer.codevillain.eu", or "this Mac". */
export function placeWords(place: Place) {
  return place.kind === "mac" ? "Layer on this Mac" : placeName(place);
}

/** The workspace frame: sidebar and page. */
function Shell() {
  return (
    <div className="app-shell flex h-full bg-bg">
      <Sidebar />
      <main className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-bg">
        <Outlet />
      </main>
      <LayerMenus />
    </div>
  );
}

// Items read in `fg`, as Layer's task list does; their icons stay `muted`.
const navLinkBase =
  "nav-link flex h-7 items-center gap-2.5 rounded-md px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent [&.active]:bg-selected [&.active]:font-bold [&.active]:text-fg [&_svg]:text-muted";
const navLink = `${navLinkBase} text-fg`;

/** The last page outside settings, where "Back to app" returns. */
let lastPagePath: string | null = null;

function Sidebar() {
  const { pathname } = useLocation();
  const inSettings = pathname.includes("/settings/");
  if (!inSettings) lastPagePath = pathname;
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
              <span className="font-display text-wordmark font-bold">Layer</span>
            </div>
          )}
          <SearchButton />
        </div>
        <div className="workspace-switcher">
          <WorkspaceMenu />
        </div>
        {inSettings ? <SettingsNav /> : <WorkspaceNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator status={useSyncStatus()} refusals={useWorkspace().refusals} />
        <ShortcutsButton />
      </div>
    </aside>
  );
}

/** What the workspace shares; each title names its "G" key. */
function WorkspaceNav() {
  const { workspace } = useWorkspace();
  const params = { workspace: workspace.url_key };
  return (
    <nav aria-label="Workspace" className="flex flex-col gap-0.5">
      <Link
        to="/$workspace/repositories"
        params={params}
        className={navLink}
        title="Repositories (G then R)"
      >
        <NavIcon icon={FolderGit2} />
        Repositories
      </Link>
      <Link
        to="/$workspace/connections"
        params={params}
        className={navLink}
        title="Connections (G then C)"
      >
        <NavIcon icon={Plug} />
        Connections
      </Link>
    </nav>
  );
}

/** Settings take the sidebar's place, with a way back. */
function SettingsNav() {
  const { workspace } = useWorkspace();
  const router = useRouter();
  const params = { workspace: workspace.url_key };
  const back = lastPagePath ?? `/${workspace.url_key}`;
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
      <AppVersion name="Layer" />
    </>
  );
}

/** Workspaces grouped by where they live, in the order of the places. */
function byPlace(entries: WorkspaceEntry[]) {
  const groups: { first: WorkspaceEntry; entries: WorkspaceEntry[] }[] = [];
  for (const entry of entries) {
    const group = groups.find((g) => g.first.place.id === entry.place.id);
    if (group) group.entries.push(entry);
    else groups.push({ first: entry, entries: [entry] });
  }
  return groups;
}

function WorkspaceMenu() {
  const { place, workspace } = useWorkspace();
  const session = useSession();
  const navigate = useNavigate();
  const groups = byPlace(session.status === "signed-in" ? workspaceEntries(session.places) : []);
  const item =
    "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none";
  const settings = (page: "members" | "account") =>
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
            {groups.map(({ first, entries }, index) => (
              <Menu.Group key={first.place.id}>
                <Menu.GroupLabel
                  className={`truncate px-2 pb-1.5 text-xs text-faint ${index ? "pt-2" : "pt-1.5"}`}
                >
                  {/* In the browser there's one place, and the account names it. */}
                  {first.place.kind === "here" ? first.me.user.email : placeName(first.place)}
                </Menu.GroupLabel>
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
              </Menu.Group>
            ))}
            <Menu.Separator className="my-1 h-px bg-line" />
            <Menu.Item
              className={item}
              onClick={() => settings(place.kind === "mac" ? "account" : "members")}
            >
              Settings
            </Menu.Item>
            {/* Only you are in a workspace on this Mac. */}
            {place.kind !== "mac" && (
              <Menu.Item className={item} onClick={() => settings("members")}>
                Invite people
              </Menu.Item>
            )}
            {bundled && (
              <Menu.Item className={item} onClick={() => void navigate({ to: "/login" })}>
                Sign in to a server…
              </Menu.Item>
            )}
            {place.kind !== "mac" && (
              <Menu.Item
                className={item}
                onClick={async () => {
                  await signOut(place);
                  await navigate({ to: "/" });
                }}
              >
                {place.kind === "server" ? `Sign out of ${placeName(place)}` : "Sign out"}
              </Menu.Item>
            )}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
