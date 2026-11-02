import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { InboxPage } from "./inbox";
import { Home, NotFound, WorkspaceLayout } from "./layout";
import { MailPage } from "./message";
import { AccountsPage } from "./settings";

const root = createRootRoute({ component: Outlet, notFoundComponent: NotFound });

const index = createRoute({ getParentRoute: () => root, path: "/", component: Home });

const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);

// Postrider runs on this Mac, where nobody signs in. The kit's sign-in
// pages link to these, with Raven's search; until Postrider runs on a
// server, they open the Inbox.
const login = createRoute({
  getParentRoute: () => root,
  path: "/login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string; error?: string } => ({
    redirect: text(search.redirect),
    error: text(search.error),
  }),
  component: Home,
});
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
  component: Home,
});

/** Everything inside a workspace: `/<url key>/...`. */
const workspace = createRoute({
  getParentRoute: () => root,
  path: "/$workspace",
  component: WorkspaceLayout,
});

const inbox = createRoute({ getParentRoute: () => workspace, path: "/", component: InboxPage });
const mail = createRoute({
  getParentRoute: () => workspace,
  path: "/mail/$account/$uid",
  component: MailPage,
});
const accounts = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/accounts",
  component: AccountsPage,
});

const routeTree = root.addChildren([
  index,
  login,
  signedIn,
  workspace.addChildren([inbox, mail, accounts]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
