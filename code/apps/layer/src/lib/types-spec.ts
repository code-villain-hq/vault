/** A task's spec (§19), mirrored from `commands/spec.rs`. */

/** One of a spec's files, in the order they are drafted and approved (SPEC-4). */
export type SpecPart = "requirements" | "design" | "tasks";
export type SpecKind = "feature" | "bugfix";

/** One requirement. `repos`: the folders that meet it; none named, every one (SPEC-1). */
export interface SpecRequirement {
  id: string;
  repos: string[];
  text: string;
}

/**
 * One step of `tasks.md`. `serves`: the requirements it names; `after`:
 * other repositories' steps it waits for, as `api 2`.
 */
export interface SpecStep {
  number: number;
  text: string;
  done: boolean;
  serves: string[];
  after: string[];
}

export interface SpecVerdict {
  id: string;
  verdict: "met" | "not met" | "unclear";
  evidence: string;
}

/** The last check against the spec (SPEC-15). `stale`: the branch moved on since. */
export interface SpecCheck {
  sha: string;
  at: number;
  results: SpecVerdict[];
  stale: boolean;
}

/** A repository's copy that is not what was approved (SPEC-22). `text` is "" when it is gone. */
export interface SpecOutside {
  checkout_id: string;
  folder: string;
  text: string;
}

/** A change an agent proposed with `spec_propose` (SPEC-24); its text is the file's draft. */
export interface SpecProposal {
  why: string;
  by: string;
  pane_id: string | null;
  at: number;
}

/** One file of the spec: the task's requirements or design (`checkout_id` null), or one repository's tasks. */
export interface SpecPartView {
  part: SpecPart;
  checkout_id: string | null;
  file: string;
  approved: string | null;
  draft: string | null;
  /** Approved, but an earlier file was approved since (SPEC-7). */
  stale: boolean;
  outside: SpecOutside[];
  proposal: SpecProposal | null;
}

/** One repository's share of the spec. */
export interface RepoSpec {
  checkout_id: string;
  repo: string;
  folder: string;
  /** Where its copy is kept: `api/specs/ACME-1-x` in the repository, or a path in the app's folder. */
  home: string;
  in_repo: boolean;
  steps: SpecStep[];
  check: SpecCheck | null;
}

/** What Apply answers did to the requirements, by id (SPEC-20). */
export interface AppliedAnswers {
  changed: string[];
  added: string[];
  removed: string[];
}

export interface TaskSpec {
  kind: SpecKind;
  /** Requirements, design, then each repository's tasks. */
  parts: SpecPartView[];
  requirements: SpecRequirement[];
  repos: RepoSpec[];
  /** A `SPEC.md` from before, to start the requirements from (SPEC-17). */
  legacy: string | null;
  /** The repositories have specs of their own from before, to merge (SPEC-25). */
  split: boolean;
}

/** One repository the spec could not be written or committed in (SPEC-6). */
export interface SpecRepoFailure {
  folder: string;
  error: string;
}

export interface SpecApproved {
  spec: TaskSpec;
  failed: SpecRepoFailure[];
}
