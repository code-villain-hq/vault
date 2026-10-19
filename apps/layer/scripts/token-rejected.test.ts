import { describe, expect, test } from "bun:test";
import { api } from "../src/lib/api";
import { githubRejected, TOKEN_REJECTED } from "../src/lib/derive";
import type { CheckoutPr, Settings } from "../src/lib/types";
import { useStore } from "../src/store";

const backend = api as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
const row = (error: string | null) => ({ checkout_id: "c", repo: "web", error }) as CheckoutPr;

describe("a token GitHub refused (PR-12)", () => {
  test("is seen in any task's rows, and only for that refusal", () => {
    expect(githubRejected({ a: [row(null)], b: [row(null), row(TOKEN_REJECTED)] })).toBe(true);
    expect(githubRejected({ a: [row("GitHub 404 Not Found: {}")] })).toBe(false);
    expect(githubRejected({})).toBe(false);
  });

  test("a sweep asked for during one runs again after it, with the new token's answer", async () => {
    useStore.setState({ settings: { github_connected: true } as unknown as Settings, prs: {} });
    let answer = [{ task_id: "t", rows: [row(TOKEN_REJECTED)], ticket: null }];
    let calls = 0;
    let release = () => {};
    backend.githubAllPrs = async () => {
      calls++;
      const now = answer;
      if (calls === 1) await new Promise<void>((r) => { release = r; });
      return structuredClone(now);
    };
    const first = useStore.getState().refreshPrs();
    // The new token is saved while the first sweep, under the old one, is out.
    answer = [{ task_id: "t", rows: [row(null)], ticket: null }];
    const second = useStore.getState().refreshPrs();
    // A real sweep takes longer than a millisecond; one that began in the same
    // one as the next reads as no older than it, and is rightly not applied.
    await Bun.sleep(2);
    release();
    await Promise.all([first, second]);
    expect(calls).toBe(2);
    expect(githubRejected(useStore.getState().prs)).toBe(false);
  });
});
