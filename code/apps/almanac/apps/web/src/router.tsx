import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { LoginPage, SignedInPage, StartPage } from "./auth";
import { CalendarPage } from "./calendar";
import { Home, NotFound, WorkspaceHome, WorkspaceLayout } from "./layout";
import { Ringing } from "./ringing";
import { AccountPage, BlocksPage, ClaudePage, MembersPage } from "./settings";
import { TopicPage, type TopicSearch } from "./topic";

const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);

/** Every page, and in the desktop app the reminders that ring from servers. */
function Root() {
  return (
    <>
      <Ringing />
      <Outlet />
    </>
  );
}

const root = createRootRoute({ component: Root, notFoundComponent: NotFound });

const index = createRoute({ getParentRoute: () => root, path: "/", component: Home });

const login = createRoute({
  getParentRoute: () => root,
  path: "/login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string; error?: string } => ({
    redirect: text(search.redirect),
    error: text(search.error),
  }),
  component: LoginPage,
});

/** The desktop app's first screen: this Mac, or a server. */
const start = createRoute({ getParentRoute: () => root, path: "/start", component: StartPage });

/** Where a desktop sign-in ends in the browser, to hand back to the app. */
const signedIn = createRoute({
  getParentRoute: () => root,
  path: "/signed-in",
  validateSearch: (
    search: Record<string, unknown>,
  ): { code?: string; next?: string; error?: string } => ({
    code: text(search.code),
    next: text(search.next),
    error: text(search.error),
  }),
  component: SignedInPage,
});

/** Everything inside a workspace: `/<url key>/...`. */
const workspace = createRoute({
  getParentRoute: () => root,
  path: "/$workspace",
  component: WorkspaceLayout,
});

const workspaceHome = createRoute({
  getParentRoute: () => workspace,
  path: "/",
  component: WorkspaceHome,
});
const claude = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/claude",
  component: ClaudePage,
});
const members = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/members",
  component: MembersPage,
});
const blocks = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/blocks",
  component: BlocksPage,
});
const account = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/account",
  component: AccountPage,
});

const calendar = createRoute({
  getParentRoute: () => workspace,
  path: "/calendar",
  component: CalendarPage,
});

/** A topic's page, with its talk beside it when `?talk=` says which. */
const topic = createRoute({
  getParentRoute: () => workspace,
  path: "/topics/$topic",
  validateSearch: (search: Record<string, unknown>): TopicSearch => ({
    talk: text(search.talk),
    ask: text(search.ask),
    section: text(search.section),
    row: text(search.row),
    about: search.about === true || search.about === "true" ? true : undefined,
  }),
  component: TopicPage,
});

const routeTree = root.addChildren([
  index,
  login,
  start,
  signedIn,
  workspace.addChildren([workspaceHome, calendar, topic, claude, members, blocks, account]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
