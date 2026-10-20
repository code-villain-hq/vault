import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { JoinPage, LoginPage, SignedInPage, SignupPage } from "./auth";
import { InboxPage } from "./inbox";
import { IssuePage } from "./issue-page";
import { MyIssuesPage, TeamIssuesPage, TriagePage, WorkspaceHome } from "./issues";
import {
  DecisionPage,
  DecisionsPage,
  DomainPage,
  DomainsPage,
  ServicePage,
  ServicesPage,
  SpecPage,
  SpecsRedirect,
} from "./knowledge";
import { Home, NotFound, WorkspaceLayout } from "./layout";
import { InitiativePage, InitiativesPage, ProjectPage, ProjectsPage } from "./planning";
import { AccountPage, MembersPage } from "./settings";
import { AgentsSettingsPage } from "./settings-agents";
import { ForgejoSettingsPage } from "./settings-forgejo";
import { GitHubSettingsPage } from "./settings-github";
import { LabelsSettingsPage, TeamsSettingsPage } from "./settings-tracker";

const root = createRootRoute({ component: Outlet, notFoundComponent: NotFound });

const index = createRoute({ getParentRoute: () => root, path: "/", component: Home });

const login = createRoute({
  getParentRoute: () => root,
  path: "/login",
  validateSearch: (search: Record<string, unknown>): { redirect?: string; error?: string } => ({
    redirect: typeof search.redirect === "string" ? search.redirect : undefined,
    // Why signing in with GitHub didn't work, from the server.
    error: typeof search.error === "string" ? search.error : undefined,
  }),
  component: LoginPage,
});

const signup = createRoute({ getParentRoute: () => root, path: "/signup", component: SignupPage });

const join = createRoute({ getParentRoute: () => root, path: "/join/$token", component: JoinPage });

/** Where a desktop sign-in ends in the browser, to hand back to the app. */
const signedInRoute = createRoute({
  getParentRoute: () => root,
  path: "/signed-in",
  validateSearch: (
    search: Record<string, unknown>,
  ): { code?: string; next?: string; error?: string } => ({
    code: typeof search.code === "string" ? search.code : undefined,
    next: typeof search.next === "string" ? search.next : undefined,
    error: typeof search.error === "string" ? search.error : undefined,
  }),
  component: SignedInPage,
});

/** Everything inside a workspace: `/<url key>/...`. */
const workspace = createRoute({
  getParentRoute: () => root,
  path: "/$workspace",
  component: WorkspaceLayout,
});

const home = createRoute({ getParentRoute: () => workspace, path: "/", component: WorkspaceHome });
const inbox = createRoute({
  getParentRoute: () => workspace,
  path: "/inbox",
  component: InboxPage,
});
const services = createRoute({
  getParentRoute: () => workspace,
  path: "/services",
  component: ServicesPage,
});
const service = createRoute({
  getParentRoute: () => workspace,
  path: "/service/$service",
  component: ServicePage,
});
const domains = createRoute({
  getParentRoute: () => workspace,
  path: "/domains",
  component: DomainsPage,
});
const domain = createRoute({
  getParentRoute: () => workspace,
  path: "/domains/$domain",
  component: DomainPage,
});
// The Specs page became Domains; old links still arrive.
const specs = createRoute({
  getParentRoute: () => workspace,
  path: "/specs",
  component: SpecsRedirect,
});
const spec = createRoute({
  getParentRoute: () => workspace,
  path: "/spec/$spec",
  component: SpecPage,
});
const decisions = createRoute({
  getParentRoute: () => workspace,
  path: "/decisions",
  component: DecisionsPage,
});
const decision = createRoute({
  getParentRoute: () => workspace,
  path: "/decision/$decision",
  component: DecisionPage,
});
const agents = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/agents",
  component: AgentsSettingsPage,
});
const forgejo = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/forgejo",
  component: ForgejoSettingsPage,
});
const github = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/github",
  component: GitHubSettingsPage,
});
const myIssues = createRoute({
  getParentRoute: () => workspace,
  path: "/my-issues",
  component: MyIssuesPage,
});
const triage = createRoute({
  getParentRoute: () => workspace,
  path: "/triage",
  component: TriagePage,
});
const teamIssues = createRoute({
  getParentRoute: () => workspace,
  path: "/team/$team",
  component: TeamIssuesPage,
});
const issue = createRoute({
  getParentRoute: () => workspace,
  path: "/issue/$identifier",
  component: IssuePage,
});
const projects = createRoute({
  getParentRoute: () => workspace,
  path: "/projects",
  component: ProjectsPage,
});
const project = createRoute({
  getParentRoute: () => workspace,
  path: "/project/$project",
  component: ProjectPage,
});
const initiatives = createRoute({
  getParentRoute: () => workspace,
  path: "/initiatives",
  component: InitiativesPage,
});
const initiative = createRoute({
  getParentRoute: () => workspace,
  path: "/initiative/$initiative",
  component: InitiativePage,
});
const members = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/members",
  component: MembersPage,
});
const teamsSettings = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/teams",
  component: TeamsSettingsPage,
});
const labels = createRoute({
  getParentRoute: () => workspace,
  path: "/settings/labels",
  component: LabelsSettingsPage,
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
  signedInRoute,
  join,
  workspace.addChildren([
    home,
    inbox,
    services,
    service,
    domains,
    domain,
    specs,
    spec,
    decisions,
    decision,
    myIssues,
    triage,
    teamIssues,
    issue,
    projects,
    project,
    initiatives,
    initiative,
    members,
    agents,
    github,
    forgejo,
    teamsSettings,
    labels,
    account,
  ]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
