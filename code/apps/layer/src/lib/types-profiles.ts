/**
 * Profiles (§25), as `commands/profiles.rs` sends them. A file of its own:
 * `types.ts` is at its size ceiling.
 */
import type { Schema, WorkspaceInfo } from "@layer/types";

export interface ProfileView {
  id: string;
  name: string;
  /** Open in a copy of the app, this window's or another (PROF-3). */
  open: boolean;
  /** This window's profile. */
  current: boolean;
  /** Where its task folders go: what deleting it leaves on disk (PROF-9). */
  worktree_root: string;
  /** Its colour, `--profile-<n>`, as on its Dock icon (PROF-6). */
  colour: number;
  /** The server it lives on, by host; null on this Mac (PROF-15). */
  server: string | null;
}

/** How a server signs people in (PROF-15): with its company's account, or not at all. */
export interface ServerWays {
  oidc: boolean;
  /** The company the server runs for. */
  company: string | null;
}

/** Signed in to a server: the person's workspaces there (PROF-15). */
export interface ServerSignedIn {
  host: string;
  /** The company the server runs for, or its host when it doesn't say. */
  company: string;
  email: string;
  workspaces: WorkspaceInfo[];
}

/** Where the window's sync client finds this workspace (PROF-11). */
export interface WorkspacePlace {
  origin: string;
  token: string;
  user_id: string;
  workspace_id: string;
}

/** How `apply_setup` went (PROF-14). */
export interface SetupApplied {
  /** False while changes made here are on their way: send them, then again. */
  applied: boolean;
  /** The repositories or settings changed, so read them again. */
  setup: boolean;
}

/** The workspace's records, as the window's sync client holds them. */
export interface SetupSnapshot {
  repositories: Schema["Repository"]["record"][];
  connections: Schema["Connection"]["record"][];
  log_sources: Schema["LogSource"]["record"][];
  agent_models: Schema["AgentModel"]["record"][];
  notes: Schema["Note"]["record"][];
  users: Schema["User"]["record"][];
}

/** A repository of the workspace's with no clone on this Mac (REPO-9). */
export interface Elsewhere {
  id: string;
  remote_key: string;
  name: string;
  default_branch: string;
  group: string | null;
  added_by: string | null;
}
