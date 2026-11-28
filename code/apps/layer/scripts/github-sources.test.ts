import { expect, test } from "bun:test";
import { api } from "../src/lib/api";
import { ticketLink } from "../src/lib/tracker";
import type { GithubSource, ReviewQueue, Settings } from "../src/lib/types";
import { useStore } from "../src/store";

test("GitHub settings actions send the clicked source to the desktop", async () => {
  const calls: unknown[] = [];
  const saved = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      __TAURI_INTERNALS__: {
        invoke: async (command: string, args: Record<string, unknown>) => {
          calls.push({ command, source: args.expectedUrl ?? args.source });
          return "ada";
        },
      },
    },
  });
  try {
    await api.githubConnect(
      "https://git.example.com/api/v3",
      "https://git.example.com",
      "",
      null,
      "git.example.com",
    );
    await api.disconnect("github", "github.com");
    await api.githubRetargetPr("checkout", "https://git.example.com/acme/web/pull/42");
    await api.startWorkOnTicket(
      "acme/web#42",
      ["web"],
      null,
      null,
      null,
      false,
      "https://git.example.com/acme/web/issues/42",
    );
    expect(calls).toEqual([
      { command: "github_connect", source: "git.example.com" },
      { command: "disconnect", source: "github.com" },
      { command: "github_retarget_pr", source: "https://git.example.com/acme/web/pull/42" },
      { command: "start_work_on_ticket", source: "https://git.example.com/acme/web/issues/42" },
    ]);
  } finally {
    if (saved) Object.defineProperty(globalThis, "window", saved);
    else Reflect.deleteProperty(globalThis, "window");
  }
});

test("issue links use only the selected source, not the original Code connection", () => {
  const publicSource = {
    key: "github.com",
    api_url: "https://api.github.com",
    web_url: "https://github.com",
  };
  const enterprise = {
    key: "git.example.com",
    api_url: "https://git.example.com/api/v3",
    web_url: "https://git.example.com",
  };
  const settings = {
    tracker_chosen: "github",
    github_issue_source: "git.example.com",
    github: publicSource,
    github_sources: [publicSource, enterprise],
  } as unknown as Settings;
  expect(ticketLink(settings, "acme/web#42")).toBe("https://git.example.com/acme/web/issues/42");
  expect(
    ticketLink({ ...settings, github_sources: settings.github_sources.slice(0, 1) }, "acme/web#42"),
  ).toBeNull();
});

test("a source removed during a review request does not come back in the late response", async () => {
  const pub: GithubSource = {
    key: "github.com",
    api_url: "https://api.github.com",
    web_url: "https://github.com",
    review_team: null,
    shared: true,
    status: "connected",
    revision: "public",
  };
  const ent: GithubSource = {
    ...pub,
    key: "git.example.com",
    api_url: "https://git.example.com/api/v3",
    web_url: "https://git.example.com",
    revision: "enterprise",
  };
  const row = (source: string) => ({
    source,
    mine: [],
    mine_more: false,
    mine_error: null,
    team: null,
    authored: null,
  });
  const before = useStore.getState();
  const settings = {
    github_sources: [pub, ent],
    github_connected: true,
    tracker: null,
  } as unknown as Settings;
  const saved = {
    getSettings: api.getSettings,
    githubReviewQueue: api.githubReviewQueue,
    githubAllPrs: api.githubAllPrs,
  };
  let release: (value: ReviewQueue) => void = () => {};
  let fetches = 0;
  const received: string[][] = [];
  const unsubscribe = useStore.subscribe((state) => {
    if (state.reviewQueue) received.push(state.reviewQueue.sources.map((source) => source.source));
  });
  try {
    useStore.setState({ settings, reviewQueue: null });
    api.getSettings = async () => ({ ...settings, github_sources: [pub] });
    api.githubAllPrs = async () => [];
    api.githubReviewQueue = async () => {
      if (++fetches === 1)
        return new Promise<ReviewQueue>((resolve) => {
          release = resolve;
        });
      return { sources: [row(pub.key)] };
    };
    const old = useStore.getState().refreshReviewQueue({ quiet: true });
    await useStore.getState().refreshSettings();
    const queued = useStore.getState().refreshReviewQueue({ quiet: true });
    release({ sources: [row(pub.key), row(ent.key)] });
    await Promise.all([old, queued]);
    expect(fetches).toBe(2);
    expect(received.flat()).not.toContain(ent.key);
    expect(useStore.getState().reviewQueue?.sources.map((source) => source.source)).toEqual([
      pub.key,
    ]);
  } finally {
    unsubscribe();
    Object.assign(api, saved);
    useStore.setState(before);
  }
});
