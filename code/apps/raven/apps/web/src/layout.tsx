import { Menu } from "@base-ui/react/menu";
import {
  AppVersion,
  ErrorMessage,
  NotAddedYet,
  Redirect,
  SearchButton,
  ShortcutsButton,
  SyncIndicator,
  useSignOutError,
} from "@code-villain/web";
import type { App } from "@raven/types";
import {
  Link,
  Outlet,
  useLocation,
  useNavigate,
  useParams,
  useRouter,
} from "@tanstack/react-router";
import {
  Activity,
  ArrowLeft,
  Check,
  ChevronsUpDown,
  HeartPulse,
  LayoutGrid,
  UserRound,
  UsersRound,
} from "lucide-react";
import { createContext, type ReactNode, useContext, useMemo } from "react";
import { useApi } from "./api";
import { RavenMenus } from "./commands";
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
import { APP_ACCESS } from "./settings";
import type { AppSummary, Level } from "./types";
import { AuthScreen, Button, buttonClass, Splash } from "./ui";
import {
  useIsAdmin,
  useIsGuest,
  useRecords,
  useSyncStatus,
  useWorkspace,
  WorkspaceProvider,
} from "./workspace";

/**
 * `/`: opens the workspace used last, or asks to sign in; the desktop app
 * first asks where to start.
 */
export function Home() {
  const signOutError = useSignOutError();
  const session = useSession();
  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") return <Redirect to={isDesktop ? "/start" : "/login"} />;
  const workspace = defaultWorkspace(workspaceEntries(session.places));
  if (!workspace) {
    const signOutEverywhere = () =>
      void Promise.all(session.places.map(({ place }) => signOut(place)));
    return (
      <AuthScreen title="You're not in any workspace">
        <p className="mb-6 text-center text-muted">Ask your company's admin to add you.</p>
        <ErrorMessage>{signOutError}</ErrorMessage>
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
    if (isDesktop) return <Redirect to="/start" />;
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

/** The apps the workspace watches, in their order, from its local copy. */
export function useApps(): App[] {
  const apps = useRecords("App");
  return useMemo(() => [...apps].sort((a, b) => a.position - b.position), [apps]);
}

/**
 * How each app is, by id, as its place last said; empty while it can't be
 * reached. `settled` once the place has answered or failed to.
 */
const Statuses = createContext<{ byId: Map<string, AppSummary>; settled: boolean }>({
  byId: new Map(),
  settled: false,
});

export function useStatuses() {
  return useContext(Statuses);
}

/**
 * `/$workspace`: the first app that's down, else the first that needs a
 * look, else the first app. Offline, the first app.
 */
export function WorkspaceHome() {
  const { workspace } = useWorkspace();
  const apps = useApps();
  const { byId, settled } = useStatuses();
  const { ready } = useSyncStatus();
  if (!ready || (!settled && apps.length > 1)) return null;
  const level = (app: App) => byId.get(app.id)?.status.level;
  const first =
    apps.find((a) => level(a) === "down") ?? apps.find((a) => level(a) === "warn") ?? apps[0];
  if (!first) return <NoApps />;
  return (
    <Redirect to="/$workspace/$app" params={{ workspace: workspace.url_key, app: first.key }} />
  );
}

/** A workspace that watches nothing yet: the one thing to do is add an app. */
function NoApps() {
  const { workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const guest = useIsGuest();
  const people = useRecords("User");
  if (guest) {
    return (
      <>
        <PageHeader title="Apps" />
        <PageBody>
          <NotAddedYet access={APP_ACCESS} people={people} />
        </PageBody>
      </>
    );
  }
  return (
    <>
      <PageHeader title="Apps" />
      <PageBody>
        <div className="mx-auto flex max-w-sm flex-col items-center gap-3 py-16 text-center">
          <h2 className="font-medium">This workspace watches no apps yet</h2>
          <p className="text-muted">
            {isAdmin
              ? "Add an app with its health address, and give it a key to send its calls with."
              : "An admin adds the apps it watches."}
          </p>
          {isAdmin && (
            <Link
              to="/$workspace/settings/apps"
              params={{ workspace: workspace.url_key }}
              className={buttonClass("primary")}
            >
              Add app
            </Link>
          )}
        </div>
      </PageBody>
    </>
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

/** The app whose pages are open, from the address. */
export function useAppKey() {
  return (useParams({ strict: false }) as { app: string }).app;
}

/** The open app, from the local copy; undefined while it loads or if it's gone. */
export function useOpenApp(): App | undefined {
  const key = useAppKey();
  return useApps().find((a) => a.key === key);
}

/** The open app's name, or its key until the local copy has it. */
export function useAppName(): string {
  const key = useAppKey();
  return useOpenApp()?.name ?? key;
}

/** The route params of the open app's pages. */
export function useAppParams() {
  const { workspace } = useWorkspace();
  return { workspace: workspace.url_key, app: useAppKey() };
}

/** Where the open app's pages come from, for its API paths. */
export function useAppPath(page: string) {
  const { workspace } = useWorkspace();
  return `/api/w/${workspace.id}/apps/${encodeURIComponent(useAppKey())}/${page}`;
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

/** Where a place is, in a sentence: "raven.codevillain.eu", or "this Mac". */
export function placeWords(place: Place) {
  return place.kind === "mac" ? "Raven on this Mac" : placeName(place);
}

/**
 * What Raven heard about an app lives in the workspace's place, not in the
 * local copy: while that place can't be reached, the page says so and
 * offers to ask again.
 */
export function NotAvailableOffline({
  what,
  title,
  onRetry,
}: {
  /** "Forge's health" */
  what: string;
  title: string;
  onRetry: () => void;
}) {
  const { place } = useWorkspace();
  return (
    <>
      <PageHeader title={title} parent={useAppName()} />
      <PageBody>
        <div className="mx-auto flex max-w-sm flex-col items-center gap-3 py-16 text-center">
          <h2 className="font-medium">Not available offline</h2>
          <p className="text-muted">
            Raven reads {what} live from {placeWords(place)}, and can't reach it right now.
          </p>
          <Button variant="secondary" onClick={onRetry}>
            Check again
          </Button>
        </div>
      </PageBody>
    </>
  );
}

/** The workspace frame: sidebar and page, with the apps' statuses for both. */
function Shell() {
  const { workspace } = useWorkspace();
  const { data, error } = useApi<AppSummary[]>(`/api/w/${workspace.id}/apps`);
  const statuses = useMemo(
    () => ({
      byId: new Map((data ?? []).map((a) => [a.id, a])),
      settled: data !== undefined || error !== undefined,
    }),
    [data, error],
  );
  return (
    <Statuses.Provider value={statuses}>
      <div className="app-shell flex h-full bg-bg">
        <Sidebar />
        <main className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-bg">
          <Outlet />
        </main>
      </div>
      <RavenMenus />
    </Statuses.Provider>
  );
}

// Items read in `fg`, as Layer's task list does; their icons stay `muted`.
const navLinkBase =
  "nav-link flex h-7 items-center gap-2.5 rounded-md px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent [&.active]:bg-selected [&.active]:font-bold [&.active]:text-fg [&_svg]:text-muted";
const navLink = `${navLinkBase} text-fg`;

const LEVEL_WORDS: Record<Level, string> = {
  up: "Up",
  warn: "Needs a look",
  down: "Down",
  unknown: "Not heard from yet",
};

/**
 * An app's state as a dot. Only a state that needs a look has colour; up
 * and unheard stay quiet.
 */
export function LevelDot({ level }: { level: Level }) {
  const tone = {
    up: "bg-faint",
    unknown: "border border-faint",
    warn: "bg-warn",
    down: "bg-danger",
  }[level];
  return (
    <span
      role="img"
      aria-label={LEVEL_WORDS[level]}
      title={LEVEL_WORDS[level]}
      className={`size-2 shrink-0 rounded-full ${tone}`}
    />
  );
}

export function levelWords(level: Level) {
  return LEVEL_WORDS[level];
}

/** The last page outside settings, where "Back to …" returns. */
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
              <span className="font-display text-wordmark font-bold">Raven</span>
            </div>
          )}
          <SearchButton />
        </div>
        <div className="workspace-switcher">
          <WorkspaceMenu />
        </div>
        {inSettings ? <SettingsNav /> : <AppNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator status={useSyncStatus()} refusals={useWorkspace().refusals} />
        <ShortcutsButton />
      </div>
    </aside>
  );
}

/** The apps the workspace watches, then the open app's views; each names its "G" key. */
function AppNav() {
  const { workspace } = useWorkspace();
  const apps = useApps();
  const { byId } = useStatuses();
  const key = useAppKey();
  const open = apps.find((a) => a.key === key);
  return (
    <>
      <nav aria-label="Apps" className="flex flex-col gap-0.5">
        <span className="sidebar-group-label px-2 pb-1 text-xs text-faint">Apps</span>
        {apps.map((watched) => {
          const status = byId.get(watched.id)?.status;
          return (
            <Link
              key={watched.id}
              to="/$workspace/$app"
              params={{ workspace: workspace.url_key, app: watched.key }}
              // The open app holds the selected view below it: bold, never filled.
              activeProps={{}}
              className={`${navLink} ${watched.key === key ? "font-bold" : ""}`}
              title={status?.reason}
            >
              <Initial name={watched.name} square />
              <span className="truncate">{watched.name}</span>
              {status && (
                <span className="ml-auto flex items-center">
                  <LevelDot level={status.level} />
                </span>
              )}
            </Link>
          );
        })}
      </nav>
      {open && (
        <nav aria-label="Views" className="flex flex-col gap-0.5">
          <span className="sidebar-group-label truncate px-2 pb-1 text-xs text-faint">
            {open.name}
          </span>
          <Link
            to="/$workspace/$app"
            params={{ workspace: workspace.url_key, app: open.key }}
            activeOptions={{ exact: true }}
            className={navLink}
            title="Health (G then H)"
          >
            <NavIcon icon={HeartPulse} />
            Health
          </Link>
          <Link
            to="/$workspace/$app/technical"
            params={{ workspace: workspace.url_key, app: open.key }}
            className={navLink}
            title="Technical (G then T)"
          >
            <NavIcon icon={Activity} />
            Technical
          </Link>
        </nav>
      )}
    </>
  );
}

/** Settings take the sidebar's place, with a way back. */
function SettingsNav() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const apps = useApps();
  const router = useRouter();
  const params = { workspace: workspace.url_key };
  const back = lastAppPath ?? `/${workspace.url_key}`;
  const backTo = apps.find((a) => back.split("/")[2] === a.key)?.name;
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
        {backTo ? `Back to ${backTo}` : "Back"}
      </a>
      {!guest && (
        <nav aria-label="Workspace settings" className="flex flex-col gap-0.5">
          <span className="px-2 pb-1 text-xs text-faint">Workspace</span>
          <Link to="/$workspace/settings/apps" params={params} className={navLink}>
            <NavIcon icon={LayoutGrid} />
            Apps
          </Link>
          <Link to="/$workspace/settings/members" params={params} className={navLink}>
            <NavIcon icon={UsersRound} />
            Members
          </Link>
        </nav>
      )}
      <nav aria-label="Personal settings" className="flex flex-col gap-0.5">
        <span className="px-2 pb-1 text-xs text-faint">You</span>
        <Link to="/$workspace/settings/account" params={params} className={navLink}>
          <NavIcon icon={UserRound} />
          Account
        </Link>
      </nav>
      <AppVersion name="Raven" />
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
  const guest = useIsGuest();
  const session = useSession();
  const navigate = useNavigate();
  const groups = byPlace(session.status === "signed-in" ? workspaceEntries(session.places) : []);
  const item =
    "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none";
  const settings = (page: "apps" | "members" | "account") =>
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
            <Menu.Item className={item} onClick={() => settings(guest ? "account" : "apps")}>
              Settings
            </Menu.Item>
            {isDesktop && (
              <Menu.Item className={item} onClick={() => void navigate({ to: "/start" })}>
                New workspace on this Mac…
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
                  if (await signOut(place)) await navigate({ to: "/" });
                  else
                    await navigate({
                      to: "/$workspace/settings/account",
                      params: { workspace: workspace.url_key },
                    });
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
