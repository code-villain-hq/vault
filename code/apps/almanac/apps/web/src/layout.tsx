import type { Topic } from "@almanac/types";
import { Menu } from "@base-ui/react/menu";
import {
  AppVersion,
  ErrorMessage,
  focusBackAfterKeys,
  NotAddedYet,
  Redirect,
  SearchButton,
  ShortcutsButton,
  SidebarEdge,
  SyncIndicator,
  useSidebarWidth,
  useSignOutError,
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
  CalendarDays,
  Check,
  ChevronRight,
  ChevronsUpDown,
  LayoutGrid,
  Plus,
  Sparkles,
  UserRound,
  UsersRound,
} from "lucide-react";
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { TalksProvider } from "./asking";
import { AlmanacMenus } from "./commands";
import { groupsOf, hueOf } from "./hues";
import { Initial, NavIcon, topicIcon } from "./icons";
import { NewTopicDialog, useNewTopicKey } from "./new-topic";
import { isDesktop } from "./platform";
import { bundled, placeName, setCurrentPlace } from "./server";
import {
  defaultWorkspace,
  rememberWorkspace,
  signOut,
  useSession,
  type WorkspaceEntry,
  workspaceEntries,
} from "./session";
import { TOPIC_ACCESS } from "./settings";
import { AuthScreen, Button, Splash } from "./ui";
import {
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

/** The workspace's topics, in their order, from its local copy. */
export function useTopics(): Topic[] {
  const topics = useRecords("Topic");
  return useMemo(() => [...topics].sort((a, b) => a.position - b.position), [topics]);
}

/** The topic `topic` sits under, while it's there. */
export function parentOf(topic: Topic, topics: Topic[]): Topic | undefined {
  return topic.parent_id ? topics.find((t) => t.id === topic.parent_id) : undefined;
}

/** The topics under `topic`, in their order. */
export function childrenOf(topic: Topic, topics: Topic[]): Topic[] {
  return topics.filter((t) => t.parent_id === topic.id);
}

/** Whether a topic is the workspace's own, Almanac, which knows the others. */
export function isOwn(topic: Topic) {
  return topic.kind === "almanac";
}

/**
 * The topics with points not read yet, by id: on their page, not what they
 * learn about you, which About shows.
 */
export function useUnread(): Set<string> {
  const points = useRecords("Point");
  const blocks = useRecords("Block");
  return useMemo(() => {
    const you = new Set(blocks.filter((b) => b.kind === "you").map((b) => b.id));
    return new Set(points.filter((p) => !p.seen && !you.has(p.block_id)).map((p) => p.topic_id));
  }, [points, blocks]);
}

/**
 * `/$workspace`: the workspace's own topic, Almanac; the first topic on a
 * server from before it had one; with none, the way to start one.
 */
export function WorkspaceHome() {
  const { workspace } = useWorkspace();
  const topics = useTopics();
  const { ready } = useSyncStatus();
  if (!ready) return null;
  const first = topics.find(isOwn) ?? topics[0];
  if (!first) return <NoTopics />;
  return (
    <Redirect
      to="/$workspace/topics/$topic"
      params={{ workspace: workspace.url_key, topic: first.id }}
    />
  );
}

/** A workspace with nothing in it yet: the one thing to do is start a topic. */
function NoTopics() {
  const guest = useIsGuest();
  const people = useRecords("User");
  const newTopic = useNewTopic();
  return (
    <>
      <PageHeader title="Topics" />
      <PageBody>
        {guest ? (
          <NotAddedYet access={TOPIC_ACCESS} people={people} />
        ) : (
          <div className="mx-auto flex max-w-sm flex-col items-center gap-3 py-16 text-center">
            <h2 className="font-medium">Nothing kept yet</h2>
            <p className="text-muted">
              A topic keeps what you learn about one thing: your plants, a pitch, a book. Name one,
              and ask it something.
            </p>
            <Button variant="primary" onClick={() => newTopic()}>
              New topic
            </Button>
          </div>
        )}
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
    <div className="page-content relative min-h-0 flex-1 overflow-y-auto">
      <div className="flex flex-col gap-8 px-6 py-6 sm:px-10">{children}</div>
    </div>
  );
}

/** Opens Settings at its first page you have: Claude, or a guest's Account. */
export function useOpenSettings() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const navigate = useNavigate();
  return useCallback(
    () =>
      void navigate({
        to: guest ? "/$workspace/settings/account" : "/$workspace/settings/claude",
        params: { workspace: workspace.url_key },
      }),
    [navigate, guest, workspace.url_key],
  );
}

/**
 * Signing out of the open workspace's server, as the menu and ⌘K offer it:
 * then home, or to Account, which says why it couldn't. None on this Mac,
 * which has no one to sign out.
 */
export function useSignOut() {
  const { place, workspace } = useWorkspace();
  const navigate = useNavigate();
  return useMemo(() => {
    if (place.kind === "mac") return null;
    return {
      label: place.kind === "server" ? `Sign out of ${placeName(place)}` : "Sign out",
      run: async () => {
        if (await signOut(place)) await navigate({ to: "/" });
        else
          await navigate({
            to: "/$workspace/settings/account",
            params: { workspace: workspace.url_key },
          });
      },
    };
  }, [place, workspace.url_key, navigate]);
}

/** The workspace frame: sidebar and page, and the dialog that starts a topic. */
function Shell() {
  // The name it starts with, while the dialog is open.
  const [starting, setStarting] = useState<string | null>(null);
  const open = useCallback((name?: string) => setStarting(name ?? ""), []);
  useNewTopicKey(open);
  const sidebar = useSidebarWidth();
  return (
    <NewTopicContext.Provider value={open}>
      <TalksProvider>
        <div className="app-shell flex h-full bg-bg" style={sidebar.style}>
          <Sidebar />
          <SidebarEdge width={sidebar.width} onChange={sidebar.setWidth} />
          <main className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-bg">
            <Outlet />
          </main>
        </div>
      </TalksProvider>
      <AlmanacMenus />
      <NewTopicDialog name={starting} onClose={() => setStarting(null)} />
    </NewTopicContext.Provider>
  );
}

const NewTopicContext = createContext<(name?: string) => void>(() => {});

/** Opens the dialog that starts a topic, with a name if one's known. */
export function useNewTopic() {
  return useContext(NewTopicContext);
}

// Items read in `fg`, as Layer's task list does; their icons stay `muted`.
const navLinkBase =
  "nav-link flex h-7 items-center gap-2.5 rounded-md px-2 outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent [&.active]:bg-selected [&.active]:font-bold [&.active]:text-fg [&_svg]:text-muted";
const navLink = `${navLinkBase} text-fg`;

/** Where this browser keeps the topics whose sub-topics are folded away. */
const FOLDED_KEY = "almanac.folded";

/** The topics whose sub-topics are folded away, kept in this browser only. */
function useFolded(): [Set<string>, (id: string) => void] {
  const [folded, setFolded] = useState<Set<string>>(() => {
    try {
      return new Set(JSON.parse(localStorage.getItem(FOLDED_KEY) ?? "[]") as string[]);
    } catch {
      return new Set();
    }
  });
  const toggle = (id: string) => {
    const next = new Set(folded);
    if (!next.delete(id)) next.add(id);
    setFolded(next);
    try {
      localStorage.setItem(FOLDED_KEY, JSON.stringify([...next]));
    } catch {
      // Folding still works until the page reloads.
    }
  };
  return [folded, toggle];
}

/** The last page outside settings, where "Back to …" returns. */
let lastPath: string | null = null;

function Sidebar() {
  const { pathname } = useLocation();
  const inSettings = pathname.includes("/settings/");
  if (!inSettings) lastPath = pathname;
  return (
    <aside
      className="app-sidebar flex w-(--sidebar-width) shrink-0 flex-col gap-4 overflow-y-auto px-2 pb-2"
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
              <span className="font-display text-wordmark font-bold">Almanac</span>
            </div>
          )}
          <SearchButton />
        </div>
        <div className="workspace-switcher">
          <WorkspaceMenu />
        </div>
        {inSettings ? <SettingsNav /> : <TopicNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator status={useSyncStatus()} refusals={useWorkspace().refusals} />
        <ShortcutsButton />
      </div>
    </aside>
  );
}

/**
 * Almanac and Calendar, then the topics by group, each with its sub-topics
 * under it, then the way to start one.
 */
function TopicNav() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const topics = useTopics();
  const unread = useUnread();
  const newTopic = useNewTopic();
  const [folded, toggle] = useFolded();
  const params = { workspace: workspace.url_key };
  const own = topics.find(isOwn);
  const groups = useMemo(() => {
    const byName = new Map<string, Topic[]>();
    for (const topic of topics.filter((t) => !isOwn(t) && !parentOf(t, topics))) {
      const list = byName.get(topic.group_name) ?? [];
      list.push(topic);
      byName.set(topic.group_name, list);
    }
    // Ungrouped topics first, then the groups by name, as their hues go.
    return groupsOf(topics).map((name) => [name, byName.get(name) ?? []] as const);
  }, [topics]);
  return (
    <>
      <nav aria-label="Workspace" className="flex flex-col gap-0.5">
        {own && (
          <Link
            to="/$workspace/topics/$topic"
            params={{ ...params, topic: own.id }}
            className={navLink}
          >
            <NavIcon icon={topicIcon(own.icon)} />
            <span className="truncate">{own.name}</span>
          </Link>
        )}
        <Link
          to="/$workspace/calendar"
          params={params}
          className={navLink}
          title="Calendar (G then C)"
        >
          <NavIcon icon={CalendarDays} />
          Calendar
        </Link>
      </nav>
      {groups.map(([group, list]) => (
        <nav key={group || "-"} aria-label={group || "Topics"} className="flex flex-col gap-0.5">
          <span className="sidebar-group-label truncate px-2 pb-1 text-xs text-faint">
            {group || "Topics"}
          </span>
          {list.flatMap((topic) => {
            const children = childrenOf(topic, topics);
            const open = !folded.has(topic.id);
            const link = (shown: Topic, newUnder = false) => (
              <Link
                key={shown.id}
                to="/$workspace/topics/$topic"
                params={{ ...params, topic: shown.id }}
                className={`${navLink} ${shown.parent_id ? "pl-7" : ""} ${children.length && !shown.parent_id ? "pr-8" : ""}`}
              >
                <TopicNavIcon topic={shown} topics={topics} />
                <span className="truncate">{shown.name}</span>
                {(unread.has(shown.id) || newUnder) && (
                  <span
                    role="img"
                    aria-label="New points"
                    title="New points"
                    className="ml-auto size-1.5 shrink-0 rounded-full bg-accent"
                  />
                )}
              </Link>
            );
            if (!children.length) return [link(topic)];
            return [
              <div key={topic.id} className="group/fold relative">
                {link(topic, !open && children.some((child) => unread.has(child.id)))}
                <button
                  type="button"
                  aria-expanded={open}
                  aria-label={`${open ? "Fold" : "Unfold"} ${topic.name}'s sub-topics`}
                  title={open ? "Fold sub-topics" : "Unfold sub-topics"}
                  onClick={() => toggle(topic.id)}
                  className={`absolute top-1 right-1 flex size-5 items-center justify-center rounded text-muted hover:bg-hover hover:text-fg focus-visible:opacity-100 ${open ? "opacity-0 group-hover/fold:opacity-100" : ""}`}
                >
                  <ChevronRight
                    size={14}
                    strokeWidth={1.75}
                    aria-hidden="true"
                    className={`transition-transform ${open ? "rotate-90" : ""}`}
                  />
                </button>
              </div>,
              ...(open ? children.map((child) => link(child)) : []),
            ];
          })}
        </nav>
      ))}
      {!guest && (
        <button
          type="button"
          onClick={() => newTopic()}
          className={`${navLinkBase} text-muted`}
          title="New topic (C)"
        >
          <NavIcon icon={Plus} />
          New topic
        </button>
      )}
    </>
  );
}

/** A topic's icon in the sidebar, in its hue, where the other icons are grey. */
function TopicNavIcon({ topic, topics }: { topic: Topic; topics: Topic[] }) {
  const Icon = topicIcon(topic.icon);
  return (
    <Icon
      size={15}
      strokeWidth={1.75}
      aria-hidden="true"
      className="shrink-0"
      style={{ color: `var(--data-${hueOf(topic, topics)})` }}
    />
  );
}

/** Settings take the sidebar's place, with a way back. */
function SettingsNav() {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const topics = useTopics();
  const router = useRouter();
  const params = { workspace: workspace.url_key };
  const back = lastPath ?? `/${workspace.url_key}`;
  const backTo = back.endsWith("/calendar")
    ? "Calendar"
    : topics.find((t) => back.split("/")[3] === t.id)?.name;
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
          <Link to="/$workspace/settings/claude" params={params} className={navLink}>
            <NavIcon icon={Sparkles} />
            Claude
          </Link>
          <Link to="/$workspace/settings/members" params={params} className={navLink}>
            <NavIcon icon={UsersRound} />
            Members
          </Link>
          <Link to="/$workspace/settings/blocks" params={params} className={navLink}>
            <NavIcon icon={LayoutGrid} />
            Blocks
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
      <AppVersion name="Almanac" />
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
  const openSettings = useOpenSettings();
  const out = useSignOut();

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
          <Menu.Popup
            finalFocus={focusBackAfterKeys}
            className="ui-menu min-w-52 origin-(--transform-origin) p-1 outline-none transition-[scale,opacity] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0"
          >
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
            <Menu.Item className={item} onClick={openSettings}>
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
            {out && (
              <Menu.Item className={item} onClick={out.run}>
                {out.label}
              </Menu.Item>
            )}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
