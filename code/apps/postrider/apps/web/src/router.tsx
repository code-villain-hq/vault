import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import type { From } from "./inbox";
import { Home, NotFound, WorkspaceLayout } from "./layout";
import { MailboxLayout, NoMailOpen } from "./mailbox";
import { MailPage } from "./message";
import { NewslettersPage } from "./newsletters";
import { SenderPane } from "./sender";
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

/**
 * The Inbox, a folder or a search, in the list column, and the mail open
 * from it beside it: the list is the layout's, the mail the child's.
 */
const mailbox = createRoute({
  getParentRoute: () => workspace,
  id: "mailbox",
  component: MailboxLayout,
});

const inbox = createRoute({ getParentRoute: () => mailbox, path: "/", component: NoMailOpen });
const mail = createRoute({
  getParentRoute: () => mailbox,
  path: "/mail/$account/$uid",
  // The list it was opened from: a folder, by its key, a search, a sender
  // or Flagged; and the folder it's in.
  validateSearch: (search: Record<string, unknown>): From => ({
    folder: text(search.folder),
    q: text(search.q),
    sender: text(search.sender),
    flagged: search.flagged === true || search.flagged === "true" ? true : undefined,
  }),
  component: MailPage,
});
const folder = createRoute({
  getParentRoute: () => mailbox,
  path: "/folders/$folder",
  component: NoMailOpen,
});
const search = createRoute({
  getParentRoute: () => mailbox,
  path: "/search",
  validateSearch: (search: Record<string, unknown>): { q?: string } => ({ q: text(search.q) }),
  component: NoMailOpen,
});
// Every account's flagged mail.
const flagged = createRoute({
  getParentRoute: () => mailbox,
  path: "/flagged",
  component: NoMailOpen,
});
// What one sender sent, from the Newsletters page: by its address.
const sender = createRoute({
  getParentRoute: () => mailbox,
  path: "/newsletters/$sender",
  component: SenderPane,
});
const newsletters = createRoute({
  getParentRoute: () => workspace,
  path: "/newsletters",
  component: NewslettersPage,
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
  workspace.addChildren([
    mailbox.addChildren([inbox, mail, folder, search, flagged, sender]),
    newsletters,
    accounts,
  ]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
