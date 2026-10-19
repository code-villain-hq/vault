import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { JoinPage, LoginPage, SignupPage } from "./auth";
import { GlossaryPage, type GlossarySearch } from "./glossary";
import { ImportExportPage } from "./import-export";
import { Home, NotFound, WorkspaceLayout } from "./layout";
import { OverviewPage } from "./overview";
import { ReviewPage, type ReviewSearch } from "./review";
import { AccountPage, MembersPage } from "./settings";
import { LanguagesPage } from "./settings-languages";
import { ModelsPage } from "./settings-models";
import { type Show, TranslationsPage, type TranslationsSearch } from "./translations";

const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);

const root = createRootRoute({ component: Outlet, notFoundComponent: NotFound });

const index = createRoute({ getParentRoute: () => root, path: "/", component: Home });

const login = createRoute({
  getParentRoute: () => root,
  path: "/login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string } => ({
    redirect: text(search.redirect),
  }),
  component: LoginPage,
});

const signup = createRoute({ getParentRoute: () => root, path: "/signup", component: SignupPage });

const join = createRoute({ getParentRoute: () => root, path: "/join/$token", component: JoinPage });

/** Everything inside a workspace: `/<url key>/...`. */
const workspace = createRoute({
  getParentRoute: () => root,
  path: "/$workspace",
  component: WorkspaceLayout,
});

const overview = createRoute({
  getParentRoute: () => workspace,
  path: "/",
  component: OverviewPage,
});
const translations = createRoute({
  getParentRoute: () => workspace,
  path: "/translations",
  validateSearch: (search: Record<string, unknown>): TranslationsSearch => ({
    lang: text(search.lang),
    show: ["missing", "unsure"].includes(search.show as string) ? (search.show as Show) : undefined,
    ns: text(search.ns),
    q: text(search.q),
  }),
  component: TranslationsPage,
});
const review = createRoute({
  getParentRoute: () => workspace,
  path: "/review",
  validateSearch: (search: Record<string, unknown>): ReviewSearch => ({
    target: text(search.target),
  }),
  component: ReviewPage,
});
const glossary = createRoute({
  getParentRoute: () => workspace,
  path: "/glossary",
  validateSearch: (search: Record<string, unknown>): GlossarySearch => ({
    target: text(search.target),
  }),
  component: GlossaryPage,
});
const importExport = createRoute({
  getParentRoute: () => workspace,
  path: "/import",
  component: ImportExportPage,
});
const languages = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/languages",
  component: LanguagesPage,
});
const models = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/models",
  component: ModelsPage,
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
  signup,
  join,
  workspace.addChildren([
    overview,
    translations,
    review,
    glossary,
    importExport,
    languages,
    models,
    members,
    account,
  ]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
