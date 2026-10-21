import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { JoinPage, LoginPage, SignedInPage } from "./auth";
import { ConnectionsPage } from "./connections";
import { Home, NotFound, WorkspaceHome, WorkspaceLayout } from "./layout";
import { RepositoriesPage, RepositoryPage } from "./repositories";
import { AccountPage, MembersPage } from "./settings";

const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);

const root = createRootRoute({ component: Outlet, notFoundComponent: NotFound });

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

const join = createRoute({ getParentRoute: () => root, path: "/join/$token", component: JoinPage });

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
const repositories = createRoute({
  getParentRoute: () => workspace,
  path: "/repositories",
  component: RepositoriesPage,
});
/** One repository, with its notes: `/<workspace>/repositories/<id>`. */
const repository = createRoute({
  getParentRoute: () => workspace,
  path: "/repositories/$repository",
  component: RepositoryPage,
});
const connections = createRoute({
  getParentRoute: () => workspace,
  path: "/connections",
  component: ConnectionsPage,
});
const members = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/members",
  component: MembersPage,
});
const account = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/account",
  component: AccountPage,
});

const routeTree = root.addChildren([
  index,
  login,
  signedIn,
  join,
  workspace.addChildren([workspaceHome, repositories, repository, connections, members, account]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
