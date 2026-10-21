import { Menu } from "@base-ui/react/menu";
import { Link, Outlet, useLocation, useNavigate, useParams } from "@tanstack/react-router";
import { Activity, ChevronsUpDown, HeartPulse } from "lucide-react";
import { type ReactNode, useEffect } from "react";
import { useApi } from "./api";
import { Initial, NavIcon } from "./icons";
import { Redirect } from "./redirect";
import { signOut, useSession } from "./session";
import type { AppSummary, Level } from "./types";
import { AuthScreen, Splash } from "./ui";

/** `/`: the first app that needs a look, or the first app. */
export function Home() {
  const session = useSession();
  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") return <Redirect to="/login" />;
  return <FirstApp />;
}

function FirstApp() {
  const { data: apps } = useApi<AppSummary[]>("/api/apps");
  if (!apps) return <Splash />;
  const first =
    apps.find((a) => a.status.level === "down") ??
    apps.find((a) => a.status.level === "warn") ??
    apps[0];
  if (!first) return <NotFound />;
  return <Redirect to="/$app" params={{ app: first.key }} />;
}

/** `/$app/...`: needs someone signed in; shows the sidebar around the page. */
export function AppLayout() {
  const session = useSession();
  const location = useLocation();
  if (session.status === "loading") return <Splash />;
  if (session.status === "signed-out") {
    return <Redirect to="/login" search={{ redirect: location.href }} />;
  }
  return <Shell name={session.me.user.name} email={session.me.user.email} />;
}

export function NotFound() {
  return (
    <AuthScreen title="Nothing here">
      <p className="mb-6 text-center text-muted">This page doesn't exist.</p>
      <Link to="/" className="ui-link block text-center">
        Go to Raven
      </Link>
    </AuthScreen>
  );
}

/** The app whose pages are open, from the address. */
export function useAppKey() {
  return (useParams({ strict: false }) as { app: string }).app;
}

/**
 * The bar at the top of each page: where it sits, its title, anything shown
 * by it (`meta`), then the page's controls.
 */
export function PageHeader({
  title,
  parent,
  meta,
  children,
}: {
  title: string;
  parent: ReactNode;
  meta?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <>
      <header className="app-topbar @container flex min-h-12 shrink-0 items-center gap-3 px-4">
        <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-xs text-muted">
          {parent}
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

/** "G" then a letter goes to a page, as in Forge. */
const GO_TO: Record<string, string> = {
  h: "/$app",
  t: "/$app/technical",
};

/** True while the key goes to a field, so letters are typed rather than shortcuts. */
function isTyping(target: EventTarget | null) {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

function Shell({ name, email }: { name: string; email: string }) {
  const app = useAppKey();
  const navigate = useNavigate();
  useEffect(() => {
    let pendingG = 0;
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      const key = event.key.toLowerCase();
      if (key === "g") {
        pendingG = event.timeStamp;
        return;
      }
      const page = GO_TO[key];
      if (page && event.timeStamp - pendingG < 1500) {
        event.preventDefault();
        pendingG = 0;
        void navigate({ to: page, params: { app } });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, app]);
  return (
    <div className="app-shell flex h-full bg-bg">
      <Sidebar name={name} email={email} />
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

function Sidebar({ name, email }: { name: string; email: string }) {
  const app = useAppKey();
  const { data: apps } = useApi<AppSummary[]>("/api/apps");
  const params = { app };
  return (
    <aside
      className="app-sidebar flex w-56 shrink-0 flex-col gap-4 overflow-y-auto px-2 pb-2"
      aria-label="Sidebar"
    >
      <div className="sidebar-navigation flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto">
        <div className="sidebar-heading sticky top-0 z-10 flex h-12 shrink-0 items-center gap-1 bg-bg">
          <div className="app-brand flex h-9 min-w-0 flex-1 items-center gap-2 px-2">
            <span className="font-display text-wordmark font-bold">Raven</span>
          </div>
        </div>
        <nav aria-label="Apps" className="flex flex-col gap-0.5">
          <span className="sidebar-group-label px-2 pb-1 text-xs text-faint">Apps</span>
          {apps?.map((watched) => (
            <Link
              key={watched.key}
              to="/$app"
              params={{ app: watched.key }}
              className={`${navLink} ${watched.key === app ? "bg-selected font-bold" : ""}`}
              title={watched.status.reason}
            >
              <Initial name={watched.name} square />
              {watched.name}
              <span className="ml-auto flex items-center">
                <LevelDot level={watched.status.level} />
              </span>
            </Link>
          ))}
        </nav>
        <nav aria-label="Views" className="flex flex-col gap-0.5">
          <span className="sidebar-group-label px-2 pb-1 text-xs text-faint">
            {apps?.find((a) => a.key === app)?.name ?? " "}
          </span>
          <Link
            to="/$app"
            params={params}
            activeOptions={{ exact: true }}
            className={navLink}
            title="Health (G then H)"
          >
            <NavIcon icon={HeartPulse} />
            Health
          </Link>
          <Link
            to="/$app/technical"
            params={params}
            className={navLink}
            title="Technical (G then T)"
          >
            <NavIcon icon={Activity} />
            Technical
          </Link>
        </nav>
      </div>
      <div className="sidebar-footer mt-auto">
        <AccountMenu name={name} email={email} />
      </div>
    </aside>
  );
}

function AccountMenu({ name, email }: { name: string; email: string }) {
  const item =
    "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none";
  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label={`Account: ${name}`}
        className="flex h-7 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left text-xs font-medium text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover"
      >
        <Initial name={name} />
        <span className="truncate">{name}</span>
        <ChevronsUpDown
          size={15}
          strokeWidth={1.75}
          aria-hidden="true"
          className="ml-auto shrink-0 text-faint"
        />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start" side="top">
          <Menu.Popup className="ui-menu min-w-52 origin-(--transform-origin) p-1 outline-none transition-[scale,opacity] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0">
            <div className="truncate px-2 py-1.5 text-xs text-faint">{email}</div>
            <Menu.Item className={item} onClick={() => void signOut()}>
              Sign out
            </Menu.Item>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
