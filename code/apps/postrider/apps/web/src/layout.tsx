import { Menu } from "@base-ui/react/menu";
import {
  AppVersion,
  type Command,
  CommandMenu,
  type ShortcutSection,
  matching,
  Redirect,
  SearchButton,
  Shortcuts,
  ShortcutsButton,
  SidebarEdge,
  SyncIndicator,
  useSidebarWidth,
} from "@code-villain/web";
import type { Folder as MailFolder } from "@postrider/types";
import {
  Link,
  Outlet,
  useLocation,
  useNavigate,
  useParams,
  useRouter,
} from "@tanstack/react-router";
import {
  Archive,
  ArrowLeft,
  Check,
  ChevronsUpDown,
  FilePen,
  Folder,
  Inbox,
  type LucideIcon,
  Mail,
  MailWarning,
  Newspaper,
  RefreshCw,
  Send,
  SquarePen,
  Trash2,
} from "lucide-react";
import { type ReactNode, useMemo } from "react";
import { NewMailProvider, useNewMail } from "./compose";
import { InboxProvider, useFolders, useInbox, useUnread } from "./mail";
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
 * The bar at the top of each page: where you are, then the page's
 * controls. In the desktop app the native title bar is hidden, so this bar
 * and the sidebar's top row are what the window is moved by.
 */
export function PageHeader({
  title,
  crumb,
  parent,
  back,
  children,
}: {
  title: string;
  /** In the title's place, when it's more than words: a folder's name, to rename. */
  crumb?: ReactNode;
  parent?: ReactNode;
  /** A button back to where the page was opened from, in the parent's place. */
  back?: ReactNode;
  children?: ReactNode;
}) {
  const { workspace } = useWorkspace();
  return (
    <>
      <header
        className="app-topbar flex min-h-12 shrink-0 items-center gap-3 px-4"
        data-tauri-drag-region="deep"
      >
        <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-xs text-muted">
          {back ?? (
            <>
              <span className="shrink-0">{parent ?? workspace.name}</span>
              <span aria-hidden="true" className="text-faint">
                ›
              </span>
            </>
          )}
          {crumb ?? (
            <span className="truncate font-medium text-fg" aria-current="page">
              {title}
            </span>
          )}
        </nav>
        <div className="ml-auto flex min-w-0 flex-1 items-center justify-end gap-3">{children}</div>
      </header>
      <h1 className="sr-only">{title}</h1>
    </>
  );
}

/** The page's body, scrolling under the header. */
export function PageBody({ children }: { children: ReactNode }) {
  return <div className="page-content min-h-0 flex-1 overflow-y-auto">{children}</div>;
}

/** The workspace frame: sidebar and page. */
function Shell() {
  const sidebar = useSidebarWidth();
  return (
    <NewMailProvider>
      <div className="app-shell flex h-full bg-bg" style={sidebar.style}>
        <Sidebar />
        <SidebarEdge width={sidebar.width} onChange={sidebar.setWidth} />
        <main className="workspace-content mb-2 mr-2 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-bg">
          <Outlet />
        </main>
      </div>
      <Menus />
    </NewMailProvider>
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
      className="app-sidebar flex w-(--sidebar-width) shrink-0 flex-col gap-4 px-2 pb-2"
      aria-label="Sidebar"
    >
      {/* The top row lines up with the page's toolbar. In the desktop app
          macOS draws the traffic lights here, in the wordmark's place. */}
      <div
        className="sidebar-heading flex h-12 shrink-0 items-center gap-1"
        data-tauri-drag-region="deep"
      >
        {isDesktop ? (
          <div className="min-w-0 flex-1" />
        ) : (
          <div className="app-brand flex h-9 min-w-0 flex-1 items-center gap-2 px-2">
            <span className="font-display text-wordmark font-bold">Postrider</span>
          </div>
        )}
        <div className="sidebar-actions flex items-center gap-1">
          <SearchButton />
          <NewMailButton />
        </div>
      </div>
      <div className="workspace-switcher">
        <WorkspaceMenu />
      </div>
      {/* Only the navigation scrolls. Its box reaches the sidebar's edges, so
          the scrollbar runs in the margin beside the items, not over them. */}
      <div className="sidebar-navigation -m-2 flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto p-2">
        {inSettings ? <SettingsNav /> : <MailNav />}
      </div>
      <div className="sidebar-footer mt-auto flex items-center gap-2 px-2 pb-1">
        <SyncIndicator
          status={useSyncStatus()}
          refusals={useWorkspace().refusals}
          syncing={useInbox().syncing}
        />
        <SyncButton />
        <ShortcutsButton />
      </div>
    </aside>
  );
}

/** Beside search, as in every app: a new mail, as C opens it. */
function NewMailButton() {
  const newMail = useNewMail();
  return (
    <button
      type="button"
      aria-label="New mail"
      title="New mail (C)"
      onClick={newMail}
      className="ui-button ui-icon-button"
    >
      <SquarePen size={15} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

/**
 * Asks every account's server now, beside the word that says how it went:
 * "Syncing…" until they've answered.
 */
function SyncButton() {
  const { sync, syncing } = useInbox();
  return (
    <button
      type="button"
      aria-label="Sync now"
      aria-disabled={syncing}
      title="Sync now"
      onClick={sync}
      className="ui-button ui-icon-button"
    >
      <RefreshCw size={15} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

/**
 * The Inbox, Newsletters, Sent, Archive, Drafts, Junk and Trash, then the
 * accounts' own folders, each once however many accounts have it, with
 * what's unread in them: not in Sent, Drafts or Trash, where it means
 * nothing.
 */
function MailNav() {
  const { workspace } = useWorkspace();
  const unread = useUnread();
  const { newsletters, sent, archive, drafts, junk, trash, others } = useShownFolders();
  const list = useOpenList();
  return (
    <nav aria-label="Mail" className="flex flex-col gap-4">
      <div className="flex flex-col gap-0.5">
        <Link
          to="/$workspace"
          params={{ workspace: workspace.url_key }}
          activeOptions={{ exact: true }}
          className={`${navLink} ${list === "" ? "active" : ""}`}
          title="Inbox (G then I)"
        >
          <NavIcon icon={Inbox} />
          Inbox
          <Unread count={unread} />
        </Link>
        <Link
          to="/$workspace/newsletters"
          params={{ workspace: workspace.url_key }}
          className={`${navLink} ${list === "newsletters" ? "active" : ""}`}
          title="Newsletters (G then N)"
        >
          <NavIcon icon={Newspaper} />
          Newsletters
          <Unread count={newsletters?.unread ?? 0} />
        </Link>
        {sent && <FolderLink folder={sent} icon={Send} open={list} title="Sent (G then S)" quiet />}
        {archive && (
          <FolderLink folder={archive} icon={Archive} open={list} title="Archive (G then A)" />
        )}
        {drafts && <FolderLink folder={drafts} icon={FilePen} open={list} quiet />}
        {junk && <FolderLink folder={junk} icon={MailWarning} open={list} />}
        {trash && <FolderLink folder={trash} icon={Trash2} open={list} quiet />}
      </div>
      {others.length > 0 && (
        <div className="flex flex-col gap-0.5">
          <span className="px-2 pb-1 text-xs text-faint">Folders</span>
          {others.map((folder) => (
            <FolderLink key={folder.key} folder={folder} icon={Folder} open={list} />
          ))}
        </div>
      )}
    </nav>
  );
}

/**
 * The list an opened mail belongs to, by its folder's key in lower case:
 * "" for the Inbox; undefined on any other page, or for a search.
 */
function useOpenList(): string | undefined {
  const { pathname, search } = useLocation();
  if (!pathname.includes("/mail/")) return undefined;
  const from = search as { folder?: string; q?: string };
  return from.q === undefined ? (from.folder?.toLowerCase() ?? "") : undefined;
}

/** The folders the sidebar shows: each with a role, then the rest. */
function useShownFolders() {
  const { folders } = useFolders();
  return useMemo(
    () => ({
      newsletters: folders.find((f) => f.role === "newsletters"),
      sent: folders.find((f) => f.role === "sent"),
      archive: folders.find((f) => f.role === "archive"),
      drafts: folders.find((f) => f.role === "drafts"),
      junk: folders.find((f) => f.role === "junk"),
      trash: folders.find((f) => f.role === "trash"),
      others: folders.filter((f) => f.role === null),
    }),
    [folders],
  );
}

function FolderLink({
  folder,
  icon,
  open,
  title,
  quiet = false,
}: {
  folder: MailFolder;
  icon: LucideIcon;
  /** The list of the mail open now (`useOpenList`). */
  open: string | undefined;
  title?: string;
  /** Without what's unread in it. */
  quiet?: boolean;
}) {
  const { workspace } = useWorkspace();
  return (
    <Link
      to="/$workspace/folders/$folder"
      params={{ workspace: workspace.url_key, folder: folder.key }}
      className={`${navLink} ${open === folder.key.toLowerCase() ? "active" : ""}`}
      title={title}
    >
      <NavIcon icon={icon} />
      <span className="truncate">{folder.name}</span>
      {!quiet && <Unread count={folder.unread} />}
    </Link>
  );
}

/** What's unread, as the sidebar and the Newsletters page show it: none when nothing is. */
export function Unread({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span className="nav-count ml-auto flex items-center gap-1.5 text-xs font-normal text-muted tabular-nums">
      <span className="ui-live-dot" aria-hidden="true" />
      <span aria-label={`${count} unread`}>{count}</span>
    </span>
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

/** Postrider's own shortcuts; the kit's list adds "Anywhere" and "Menus". */
const SECTIONS: ShortcutSection[] = [
  {
    title: "The Inbox, a folder, a search",
    keys: [
      ["J K or ↓ ↑", "Next and previous mail"],
      ["Enter or O", "Open the mail"],
      ["/", "Search mail"],
    ],
  },
  {
    title: "Newsletters",
    keys: [
      ["J K or ↓ ↑", "Next and previous sender"],
      ["Enter or O", "Open what it sent"],
      ["U", "Unsubscribe from it"],
      ["I", "Keep it in the Inbox"],
      ["/", "Find a sender"],
    ],
  },
  {
    title: "A sender's mail",
    keys: [
      ["U", "Unsubscribe from the sender"],
      ["I", "Keep the sender in the Inbox"],
      ["Esc", "Back to Newsletters"],
    ],
  },
  {
    title: "A mail",
    keys: [
      ["J K", "The next and previous mail, opened"],
      ["R", "Reply"],
      ["E", "Archive, and open the next"],
      ["⌫", "Delete, and open the next"],
      ["V", "Move to a folder, or a new one"],
      ["Esc", "Close it"],
    ],
  },
  {
    title: "Writing",
    keys: [
      ["C", "New mail"],
      ["⌘Enter", "Send"],
      ["⌘Z", "Back to yours, after More professional"],
      ["Esc", "Close it, keeping what you wrote"],
    ],
  },
];

/** ⌘K and the shortcuts list: the Inbox, the folders, settings, and the other workspaces. */
function Menus() {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const { sent, archive } = useShownFolders();
  const params = { workspace: workspace.url_key };
  const goTo = [
    { key: "i", name: "Inbox", go: () => void navigate({ to: "/$workspace", params }) },
    {
      key: "n",
      name: "Newsletters",
      go: () => void navigate({ to: "/$workspace/newsletters", params }),
    },
  ];
  for (const [key, folder] of [
    ["s", sent],
    ["a", archive],
  ] as const) {
    if (!folder) continue;
    goTo.push({
      key,
      name: folder.name,
      go: () =>
        void navigate({
          to: "/$workspace/folders/$folder",
          params: { ...params, folder: folder.key },
        }),
    });
  }
  return (
    <>
      <CommandMenu useCommands={useCommands} placeholder="Type a command…" />
      <Shortcuts
        goTo={goTo}
        sections={SECTIONS}
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
  const { sent, archive, others } = useShownFolders();
  const newMail = useNewMail();

  const commands = useMemo(() => {
    const params = { workspace: workspace.url_key };
    const list: Command[] = [
      {
        id: "new-mail",
        group: "Actions",
        label: "New mail",
        hint: "C",
        icon: <NavIcon icon={SquarePen} />,
        run: newMail,
      },
      {
        id: "inbox",
        group: "Go to",
        label: "Inbox",
        hint: "G I",
        icon: <NavIcon icon={Inbox} />,
        run: () => void navigate({ to: "/$workspace", params }),
      },
      {
        id: "newsletters",
        group: "Go to",
        label: "Newsletters",
        hint: "G N",
        icon: <NavIcon icon={Newspaper} />,
        run: () => void navigate({ to: "/$workspace/newsletters", params }),
      },
      ...[
        { folder: sent, icon: Send, hint: "G S" },
        { folder: archive, icon: Archive, hint: "G A" },
        ...others.map((folder) => ({ folder, icon: Folder, hint: undefined })),
      ].flatMap(({ folder, icon, hint }): Command[] =>
        folder
          ? [
              {
                id: `folder-${folder.key}`,
                group: "Go to",
                label: folder.name,
                hint,
                icon: <NavIcon icon={icon} />,
                run: () =>
                  void navigate({
                    to: "/$workspace/folders/$folder",
                    params: { ...params, folder: folder.key },
                  }),
              },
            ]
          : [],
      ),
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
  }, [workspace, session, navigate, sent, archive, others, newMail]);

  return matching(commands, query);
}
