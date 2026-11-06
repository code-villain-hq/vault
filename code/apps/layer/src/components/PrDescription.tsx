/**
 * The description of the pull requests a task opens (PR-4), and the
 * screenshots that go with it. A fast model drafts it from the diff; where a
 * repository's template asks for screenshots, the task's running agent
 * drafts it instead, as it can run the app and take them. Split from
 * PrPanel at its size ceiling.
 */
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { useRunningAgentsOf } from "../lib/panes";
import { trackerOfKey } from "../lib/tracker";
import type { DraftImage, RepoResult, TaskView } from "../lib/types";
import { Field, Spinner } from "./ui";

type Failure = { fail: (e: unknown) => void; clear: () => void };

/** "web's template asks", "The templates of api and web ask". */
function ask(repos: string[]) {
  if (repos.length === 1) return `${repos[0]}'s template asks`;
  return `The templates of ${repos.slice(0, -1).join(", ")} and ${repos[repos.length - 1]} ask`;
}

export function usePrDescription(task: TaskView, dialog: Failure) {
  const agentPanes = useRunningAgentsOf(task.id);
  const [body, setBody] = useState("");
  const [images, setImages] = useState<DraftImage[]>([]);
  /** What of the agent's screenshots could not be shown. */
  const [problems, setProblems] = useState<string[]>([]);
  const [drafting, setDrafting] = useState(false);
  /** The agent asked for the draft, and whether for screenshots too. */
  const [asked, setAsked] = useState<{ pane: string; shots: string[] } | null>(null);
  /** Repositories whose template asks for screenshots no agent was running to take. */
  const [untaken, setUntaken] = useState<string[]>([]);
  const pollRef = useRef<number | null>(null);

  // Stop waiting for a draft if the panel goes away.
  useEffect(
    () => () => {
      if (pollRef.current) window.clearInterval(pollRef.current);
    },
    [],
  );

  // The description arrives a few words at a time. Showing it as it is written
  // is most of what makes this feel quick: the wait is the same, but it starts
  // reading like an answer straight away instead of a spinner.
  //
  // Only while a draft is being written: a chunk still in flight when the
  // finished text landed was appended to it, repeating the end.
  const streaming = useRef(false);
  useEffect(() => {
    const p = listen<{ task_id: string; text: string }>("pr:draft", (e) => {
      if (e.payload.task_id !== task.id || !streaming.current) return;
      setBody((current) => current + e.payload.text);
    });
    return () => {
      void p.then((un) => un());
    };
  }, [task.id]);

  useEffect(() => {
    setBody(task.issue_url ? `${trackerOfKey(task.issue_key ?? "")}: ${task.issue_url}\n` : "");
    setImages([]);
    setProblems([]);
    setUntaken([]);
  }, [task.id, task.issue_url, task.issue_key]);

  /// Draft the description from the diff, with a cheap one-shot model.
  ///
  /// Fast because it asks nothing of the working agent: no shared context to
  /// re-read, no waiting for it to finish its turn, no file to hand back. If
  /// that is not possible, or a template asks for screenshots the model
  /// cannot take, ask the agent itself: slower, but it knows what the diff
  /// cannot say, and it can run the app.
  async function draft() {
    setDrafting(true);
    setAsked(null);
    setImages([]);
    setProblems([]);
    setUntaken([]);
    dialog.clear();
    // Cleared so the streamed text is not appended to whatever was there.
    setBody("");
    const shots = await api.prScreenshotsWanted(task.id).catch(() => [] as string[]);
    const pane = agentPanes[0];
    if (shots.length > 0 && pane) return askAgent(pane, shots);

    streaming.current = true;
    try {
      const drafted = await api.draftPrDescription(task.id);
      streaming.current = false;
      setBody(drafted.trim());
      setUntaken(shots);
      setDrafting(false);
      return;
    } catch (e) {
      // Whatever was streamed before it failed is half an answer, not an answer.
      streaming.current = false;
      setBody("");
      if (!pane) {
        setDrafting(false);
        dialog.fail(e);
        return;
      }
    }
    await askAgent(pane, []);
  }

  async function askAgent(pane: { id: string; title: string }, shots: string[]) {
    setAsked({ pane: pane.title, shots });
    try {
      await api.requestPrDescription(task.id, pane.id);

      const started = Date.now();
      // Running the app for screenshots takes longer than writing.
      const limit = (shots.length > 0 ? 10 : 5) * 60 * 1000;
      if (pollRef.current) window.clearInterval(pollRef.current);
      // guard: allow poll — waits for a file the agent writes; nothing announces it. Gives up after 5 minutes, 10 with screenshots.
      pollRef.current = window.setInterval(async () => {
        const got = await api.takePrDescription(task.id).catch(() => null);
        if (got) {
          setBody(got.text.trim());
          setImages(got.images);
          setProblems(got.problems);
          setDrafting(false);
          if (pollRef.current) window.clearInterval(pollRef.current);
        } else if (Date.now() - started > limit) {
          setDrafting(false);
          if (pollRef.current) window.clearInterval(pollRef.current);
          dialog.fail("Gave up waiting for the draft. The agent may still be working.");
        }
      }, 2000);
    } catch (e) {
      setDrafting(false);
      dialog.fail(e);
    }
  }

  /**
   * Add the screenshots to the pull requests Open PRs just opened, each to
   * its repository's: not to one that failed to open or was left out.
   */
  async function addImages(
    taskId: string,
    opened: RepoResult[],
    leftOut: string[],
  ): Promise<RepoResult[]> {
    // Seen by now: the dialog showed them before Open was pressed.
    setProblems([]);
    const skip = new Set(
      [...opened.filter((r) => !r.ok).map((r) => r.repo), ...leftOut].map((r) => r.toLowerCase()),
    );
    const going = images.filter((i) => !i.repo || !skip.has(i.repo.toLowerCase()));
    if (going.length === 0) return [];
    const added = await api.showPrImages(taskId, going);
    if (added.every((r) => r.ok)) setImages([]);
    return added;
  }

  return {
    body,
    setBody,
    images,
    removeImage: (path: string) => setImages((now) => now.filter((i) => i.path !== path)),
    problems,
    drafting,
    asked,
    untaken,
    draft,
    addImages,
  };
}

export type PrDescription = ReturnType<typeof usePrDescription>;

/** The Description field: its text, the draft button, and the screenshots. */
export function PrDescriptionField({ d }: { d: PrDescription }) {
  const hint = d.drafting
    ? d.asked
      ? d.asked.shots.length > 0
        ? `Asking ${d.asked.pane}: ${ask(d.asked.shots)} for screenshots, so it runs the app to take them. This takes a few minutes.`
        : `Asking ${d.asked.pane} instead. This one takes longer.`
      : "Writing — it appears here as it goes."
    : d.untaken.length > 0
      ? `${ask(d.untaken)} for screenshots, and no agent is running to take them. Add them to the pull request yourself, or start one and draft again.`
      : d.asked
        ? `Written by ${d.asked.pane}, as a handoff for whoever reviews this.`
        : "Written from the diff by a fast model, as a handoff for whoever reviews this.";
  // The repository named only when they go to more than one.
  const spread = new Set(d.images.map((i) => i.repo?.toLowerCase())).size > 1;
  const where = (i: DraftImage) => (spread && i.repo ? `${i.repo}: ${i.caption}` : i.caption);
  return (
    <Field label="Description" hint={hint}>
      <textarea
        className="ui-field"
        rows={8}
        value={d.body}
        onChange={(e) => d.setBody(e.target.value)}
      />
      {d.images.length > 0 && (
        <div className="pr-shots">
          <div className="pr-shots-list">
            {d.images.map((i) => (
              <figure key={i.path} className="pr-shot">
                {i.preview && <img src={i.preview} alt={i.caption} />}
                <figcaption>{where(i)}</figcaption>
                <button
                  className="pr-shot-remove"
                  title="Leave out"
                  aria-label={`Leave out ${i.caption}`}
                  onClick={() => d.removeImage(i.path)}
                >
                  ×
                </button>
              </figure>
            ))}
          </div>
          <div className="muted">
            Open adds these to the pull request, stored in the repository for everyone who can read
            it.
          </div>
        </div>
      )}
      {d.problems.map((p) => (
        <div key={p} className="muted">
          {p}
        </div>
      ))}
      <div className="row" style={{ marginTop: 8 }}>
        <button
          className="ui-button"
          disabled={d.drafting}
          title="Write the description from the diff on this branch"
          onClick={() => void d.draft()}
        >
          {d.drafting ? "Drafting…" : "✨ Draft description"}
        </button>
        {d.drafting && <Spinner />}
      </div>
    </Field>
  );
}
