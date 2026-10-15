import { create } from "zustand";
import { api } from "./api";
import type { AppliedAnswers, SpecKind, SpecPart, SpecPartView, SpecRepoFailure, TaskSpec } from "./types";

/**
 * Each task's spec as the Spec tab holds it (§19). Kept here rather than in
 * the tab, so an edit or a draft under way survives switching tabs or
 * tasks; and what an editor holds is kept as a draft in the task folder
 * too, so quitting loses nothing (SPEC-6).
 *
 * A spec is one per task: its requirements and design are the task's
 * (`checkoutId` null), and each repository has its own tasks.
 */
export interface SpecState {
  loaded: boolean;
  spec: TaskSpec | null;
  /** What each editor holds where it is not what is approved, by `slotKey`. */
  edits: Record<string, string>;
  /** A draft being streamed in: which file, or a merge of specs from before. */
  drafting: { requestId: string; part: SpecPart | "merge" } | null;
  /** Repositories whose check is running (SPEC-15). */
  checking: string[];
  /** What Apply answers last did, and what the editor held before it, for Undo (SPEC-20). */
  applied: { results: AppliedAnswers; before: string | undefined } | null;
}

/** One file of the spec, as the tab names it. */
export interface SpecFile {
  part: SpecPart;
  checkoutId: string | null;
}

interface Specs {
  byTask: Record<string, SpecState>;
  load: (taskId: string) => Promise<void>;
  edit: (taskId: string, file: SpecFile, text: string) => void;
  /** Draft one file: requirements or design for the task, or every repository's tasks. */
  draft: (taskId: string, part: SpecPart, kind: SpecKind | null) => Promise<void>;
  /** Merge the specs each repository had of its own into drafts of one (SPEC-25). */
  merge: (taskId: string) => Promise<void>;
  /** Approve files as their editors hold them; the repositories that failed are returned. */
  approve: (taskId: string, files: SpecFile[], kind?: SpecKind | null, texts?: Record<string, string>) => Promise<SpecRepoFailure[]>;
  putBack: (taskId: string, part: SpecPart, checkoutId: string) => Promise<void>;
  turnDown: (taskId: string, file: SpecFile) => Promise<void>;
  check: (taskId: string, checkoutId: string) => Promise<void>;
  /** Build the answered open questions into the requirements (SPEC-20). */
  applyAnswers: (taskId: string) => Promise<void>;
  /** Put back what the requirements editor held before Apply answers. */
  undoAnswers: (taskId: string) => void;
  /** A piece of a draft under way (`spec:draft`): the file's whole draft so far. */
  chunk: (requestId: string, part: SpecPart, checkoutId: string | null, text: string) => void;
  /** An agent ticked a step or proposed a change, or one stopped (`spec:changed`, SPEC-22). */
  changed: (taskId: string) => void;
  /** The agent Start work picked, for the Spec tab to start once the spec is agreed (SPEC-18). */
  pending: { taskId: string; agentId: string | null } | null;
  setPending: (p: { taskId: string; agentId: string | null } | null) => void;
}

export const slotKey = (part: SpecPart, checkoutId: string | null) =>
  part === "tasks" ? `tasks:${checkoutId ?? ""}` : part;

/** One file's view in a spec. */
export function partView(spec: TaskSpec | null | undefined, part: SpecPart, checkoutId: string | null): SpecPartView | undefined {
  return spec?.parts.find((p) => p.part === part && (part !== "tasks" || p.checkout_id === checkoutId));
}

/** What is approved of one file, or "". */
export function approvedText(spec: TaskSpec | null | undefined, part: SpecPart, checkoutId: string | null): string {
  return partView(spec, part, checkoutId)?.approved ?? "";
}

/** What an editor shows: the edit, else what is approved. */
export function editorText(state: SpecState | undefined, part: SpecPart, checkoutId: string | null): string {
  return state?.edits[slotKey(part, checkoutId)] ?? approvedText(state?.spec, part, checkoutId);
}

/** The tab's badge: steps done of all, or else how many requirements. */
export function specProgress(spec: TaskSpec | null): string | null {
  const steps = (spec?.repos ?? []).flatMap((r) => r.steps);
  if (steps.length > 0) return `${steps.filter((s) => s.done).length}/${steps.length}`;
  const reqs = spec?.requirements.length ?? 0;
  return reqs > 0 ? String(reqs) : null;
}

const EMPTY: SpecState = { loaded: false, spec: null, edits: {}, drafting: null, checking: [], applied: null };

/** Drafts are written a moment after typing stops, not at every key. */
const saving = new Map<string, { timer: ReturnType<typeof setTimeout>; write: () => Promise<void> }>();

function keep(taskId: string, file: SpecFile, text: string | null) {
  const key = `${taskId}:${slotKey(file.part, file.checkoutId)}`;
  clearTimeout(saving.get(key)?.timer);
  const write = () => {
    saving.delete(key);
    return api.saveSpecDraft(taskId, file.part, file.checkoutId, text).catch(() => {});
  };
  saving.set(key, { timer: setTimeout(() => void write(), 600), write });
}

/**
 * Write now what is waiting to be written. A run that reads the drafts
 * (a redraft, Apply answers) would otherwise miss an answer picked a
 * moment before.
 */
async function flush(taskId: string) {
  const waiting = [...saving.entries()].filter(([key]) => key.startsWith(`${taskId}:`));
  await Promise.all(waiting.map(([, w]) => {
    clearTimeout(w.timer);
    return w.write();
  }));
}

/** The edits a freshly read spec brings: its drafts. */
function draftsOf(spec: TaskSpec): Record<string, string> {
  const edits: Record<string, string> = {};
  for (const p of spec.parts) if (p.draft !== null) edits[slotKey(p.part, p.checkout_id)] = p.draft;
  return edits;
}

export const useSpecs = create<Specs>((set, get) => {
  const put = (taskId: string, part: Partial<SpecState>) =>
    set((s) => ({ byTask: { ...s.byTask, [taskId]: { ...(s.byTask[taskId] ?? EMPTY), ...part } } }));
  const state = (taskId: string) => get().byTask[taskId] ?? EMPTY;

  /** A run streamed into the editors; a failed one puts back what they held (SPEC-5). */
  async function streamed(taskId: string, part: SpecPart | "merge", run: (requestId: string) => Promise<void>) {
    await flush(taskId);
    const before = state(taskId).edits;
    const requestId = `spec-${crypto.randomUUID()}`;
    put(taskId, { drafting: { requestId, part }, applied: null });
    try {
      await run(requestId);
      put(taskId, { drafting: null });
      await get().load(taskId);
    } catch (e) {
      put(taskId, { drafting: null, edits: before });
      throw e;
    }
  }

  return {
    byTask: {},
    load: async (taskId) => {
      const spec = await api.readSpec(taskId);
      // An edit under way here wins over the draft kept on disk, which is
      // at most a moment behind it.
      put(taskId, { loaded: true, spec, edits: { ...draftsOf(spec), ...state(taskId).edits } });
    },
    edit: (taskId, file, text) => {
      const key = slotKey(file.part, file.checkoutId);
      const edits = { ...state(taskId).edits };
      const same = text.trim() === approvedText(state(taskId).spec, file.part, file.checkoutId).trim();
      if (same) delete edits[key];
      else edits[key] = text;
      // Undo after a change of the user's own would throw that away too.
      put(taskId, file.part === "requirements" ? { edits, applied: null } : { edits });
      keep(taskId, file, same ? null : text);
    },
    draft: (taskId, part, kind) => streamed(taskId, part, (requestId) => api.draftSpec(taskId, part, kind, requestId)),
    merge: (taskId) => streamed(taskId, "merge", (requestId) => api.mergeSpecs(taskId, requestId)),
    approve: async (taskId, files, kind = null, texts) => {
      const s = state(taskId);
      const body = files.map((f) => {
        const key = slotKey(f.part, f.checkoutId);
        clearTimeout(saving.get(`${taskId}:${key}`)?.timer);
        return { part: f.part, checkout_id: f.checkoutId, text: texts?.[key] ?? editorText(s, f.part, f.checkoutId) };
      });
      const { spec, failed } = await api.approveSpec(taskId, body, kind);
      const edits = { ...state(taskId).edits };
      for (const f of files) delete edits[slotKey(f.part, f.checkoutId)];
      put(taskId, files.some((f) => f.part === "requirements") ? { spec, edits, applied: null } : { spec, edits });
      return failed;
    },
    putBack: async (taskId, part, checkoutId) => {
      put(taskId, { spec: await api.putBackSpec(taskId, part, checkoutId) });
    },
    turnDown: async (taskId, file) => {
      const spec = await api.turnDownSpecProposal(taskId, file.part, file.checkoutId);
      const edits = { ...state(taskId).edits };
      delete edits[slotKey(file.part, file.checkoutId)];
      put(taskId, { spec, edits });
    },
    check: async (taskId, checkoutId) => {
      put(taskId, { checking: [...state(taskId).checking, checkoutId] });
      try {
        await api.checkSpec(taskId, checkoutId);
        await get().load(taskId);
      } finally {
        put(taskId, { checking: state(taskId).checking.filter((c) => c !== checkoutId) });
      }
    },
    applyAnswers: async (taskId) => {
      const before = state(taskId).edits.requirements;
      const got: { results?: AppliedAnswers } = {};
      await streamed(taskId, "requirements", async (requestId) => {
        got.results = await api.applySpecAnswers(taskId, requestId);
      });
      if (got.results) put(taskId, { applied: { results: got.results, before } });
    },
    undoAnswers: (taskId) => {
      const s = state(taskId);
      if (!s.applied) return;
      get().edit(taskId, { part: "requirements", checkoutId: null }, s.applied.before ?? approvedText(s.spec, "requirements", null));
      put(taskId, { applied: null });
    },
    chunk: (requestId, part, checkoutId, text) => {
      const entry = Object.entries(get().byTask).find(([, s]) => s.drafting?.requestId === requestId);
      if (!entry) return;
      const [taskId, s] = entry;
      put(taskId, { edits: { ...s.edits, [slotKey(part, checkoutId)]: text } });
    },
    changed: (taskId) => {
      if (state(taskId).loaded) void get().load(taskId).catch(() => {});
    },
    pending: null,
    setPending: (pending) => set({ pending }),
  };
});
