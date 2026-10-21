import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { ActionPage, type ActionSearch } from "./action";
import { LoginPage, SignedInPage } from "./auth";
import { CallPage } from "./call";
import { HealthPage } from "./health";
import { AppLayout, Home, NotFound } from "./layout";
import { TechnicalPage, type WindowSearch } from "./technical";

const text = (value: unknown) => (typeof value === "string" && value ? value : undefined);

const WINDOWS = [900, 3600, 86_400, 604_800];
const window = (value: unknown) => {
  const secs = Number(value);
  return WINDOWS.includes(secs) ? secs : undefined;
};

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

/** Everything about one watched app: `/<app key>/...`. */
const app = createRoute({ getParentRoute: () => root, path: "/$app", component: AppLayout });

const health = createRoute({ getParentRoute: () => app, path: "/", component: HealthPage });
const technical = createRoute({
  getParentRoute: () => app,
  path: "/technical",
  validateSearch: (search: Record<string, unknown>): WindowSearch => ({
    window: window(search.window),
  }),
  component: TechnicalPage,
});
const action = createRoute({
  getParentRoute: () => app,
  path: "/action",
  validateSearch: (search: Record<string, unknown>): ActionSearch => ({
    name: text(search.name),
    window: window(search.window),
  }),
  component: ActionPage,
});
const call = createRoute({ getParentRoute: () => app, path: "/calls/$trace", component: CallPage });

const routeTree = root.addChildren([
  index,
  login,
  signedIn,
  app.addChildren([health, technical, action, call]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
