// Sign-in screens: shared in `@code-villain/web`, named for Almanac.
import { createAuthPages } from "@code-villain/web";
import { DEFAULT_SERVER } from "./server";
import { signedIn, useSession, workspaceKey } from "./session";

export const { useProviders, LoginPage, SignedInPage, StartPage } = createAuthPages(
  { useSession, signedIn, workspaceKey },
  {
    defaultServer: DEFAULT_SERVER,
    start: {
      workspacePlaceholder: "My topics",
      offlineNote: "Keeps your topics on this Mac. Only you see them.",
    },
  },
);
