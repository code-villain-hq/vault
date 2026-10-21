/**
 * Profiles (§25), as `commands/profiles.rs` sends them. A file of its own:
 * `types.ts` is at its size ceiling.
 */
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
}
