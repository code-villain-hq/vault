// Sign-in screens: shared in `@code-villain/web`, named for Raven.
import { createAuthPages } from "@code-villain/web";
import { DEFAULT_SERVER } from "./server";
import { signedIn, useSession, workspaceKey } from "./session";

export const { useProviders, LoginPage, SignedInPage, StartPage } = createAuthPages(
  { useSession, signedIn, workspaceKey },
  {
    defaultServer: DEFAULT_SERVER,
    start: {
      workspacePlaceholder: "My apps",
      offlineNote: "Watches the apps on this Mac. Only you see it.",
    },
  },
);
