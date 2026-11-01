// Who is signed in, in every place: the kit's session, with Almanac's `Me`.

import type { Me } from "@almanac/types";
import {
  createSession,
  type Session as KitSession,
  type SignedIn as KitSignedIn,
  type WorkspaceEntry as KitWorkspaceEntry,
} from "@code-villain/web";
import "./server";

export type SignedIn = KitSignedIn<Me>;
export type Session = KitSession<Me>;
export type WorkspaceEntry = KitWorkspaceEntry<Me>;

export { workspaceEntries } from "@code-villain/web";

export const {
  getSession,
  useSession,
  loadSession,
  signedIn,
  signOut,
  defaultWorkspace,
  rememberWorkspace,
  workspaceKey,
} = createSession<Me>();
