// Who is signed in, in every place: the kit's session, with Raven's `Me`.
import {
  createSession,
  type Session as KitSession,
  type SignedIn as KitSignedIn,
  type WorkspaceEntry as KitWorkspaceEntry,
} from "@code-villain/web";
import type { Me } from "@raven/types";
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
