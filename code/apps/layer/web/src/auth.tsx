// Sign-in screens: shared in `@code-villain/web`, named for Layer.
// Layer has no StartPage: profiles and workspace.rs own "this Mac".
import { createAuthPages } from "@code-villain/web";
import { DEFAULT_SERVER } from "./server";
import { signedIn, useSession, workspaceKey } from "./session";

export const { useProviders, LoginPage, SignedInPage } = createAuthPages(
  { useSession, signedIn, workspaceKey },
  { defaultServer: DEFAULT_SERVER },
);
