import { createStore } from "zustand";
import { api, failureText } from "./api";
import type { PrDetailRead } from "./pr-detail-read";
import { prPermalinkIdentity } from "./pr-permalink";
import type { ReviewPrDetails } from "./types";

type Target = Pick<ReviewPrDetails, "repo" | "number" | "url">;
type Section = "comments" | "checks";
type Sections = Record<Section, boolean>;
type Entry = Sections & {
  request: symbol;
  read: PrDetailRead<ReviewPrDetails>;
};

async function readDetails({ repo, number, url }: Target): Promise<ReviewPrDetails> {
  const permalink = prPermalinkIdentity(url);
  const answer = await api.githubReviewPrDetails(repo, number, url);
  if (
    !permalink ||
    typeof answer?.repo !== "string" ||
    answer.repo.toLowerCase() !== repo.toLowerCase() ||
    answer.number !== number ||
    prPermalinkIdentity(answer.url) !== permalink
  ) {
    throw new Error("Couldn’t read details for this pull request. Try again.");
  }
  return answer;
}

/** One view owns the open readers, even when cards move between group subtrees. */
export function createReviewPrReaders(connection: string) {
  const store = createStore(() => new Map<string, Entry>());
  const keyOf = ({ repo, number, url }: Target) =>
    JSON.stringify([connection, repo.toLowerCase(), number, prPermalinkIdentity(url) ?? url]);

  function put(key: string, entry?: Entry) {
    const next = new Map(store.getState());
    if (entry) next.set(key, entry);
    else next.delete(key);
    store.setState(next, true);
  }

  function start(target: Target, key: string, { comments, checks }: Sections) {
    const request = Symbol();
    put(key, { comments, checks, request, read: { status: "loading" } });
    const finish = (read: PrDetailRead<ReviewPrDetails>) => {
      const current = store.getState().get(key);
      if (current?.request === request) put(key, { ...current, read });
    };
    void readDetails(target).then(
      (data) => finish({ status: "ready", data }),
      (error) => finish({ status: "failed", message: failureText(error ?? "") }),
    );
  }

  function toggle(target: Target, section: Section) {
    const key = keyOf(target);
    const entry = store.getState().get(key);
    const sections = { comments: entry?.comments ?? false, checks: entry?.checks ?? false };
    sections[section] = !sections[section];
    if (!sections.comments && !sections.checks) put(key);
    else if (entry) put(key, { ...entry, ...sections });
    else start(target, key, sections);
  }

  function refresh(targets: Target[]) {
    const seen = new Set<string>();
    for (const target of targets) {
      const key = keyOf(target);
      const entry = store.getState().get(key);
      if (!entry || seen.has(key)) continue;
      seen.add(key);
      start(target, key, entry);
    }
  }

  function prune(targets: Target[]) {
    const current = store.getState();
    if (!current.size) return;
    const live = new Set(targets.map(keyOf));
    const next = new Map([...current].filter(([key]) => live.has(key)));
    if (current.size !== next.size) store.setState(next, true);
  }

  return { store, keyOf, toggle, refresh, prune };
}

export type ReviewPrReaders = ReturnType<typeof createReviewPrReaders>;
