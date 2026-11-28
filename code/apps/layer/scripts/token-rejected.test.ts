import { describe, expect, test } from "bun:test";
import { api } from "../src/lib/api";
import { githubRefusal, TOKEN_NEEDS_SSO, TOKEN_REJECTED } from "../src/lib/derive";
import type { CheckoutPr, Settings } from "../src/lib/types";
import { useStore } from "../src/store";

const row = (error: string | null) => ({ checkout_id: "c", repo: "web", error }) as CheckoutPr;

describe("a token GitHub refused (PR-12)", () => {
  test("is seen in any task's rows, and only for that refusal", () => {
    expect(githubRefusal({ a: [row(null)], b: [row(null), row(TOKEN_REJECTED)] })).toBe("rejected");
    expect(githubRefusal({ a: [row("GitHub 404 Not Found: {}")] })).toBe(null);
    expect(githubRefusal({})).toBe(null);
  });

  test("a token not authorized for single sign-on is told apart from a rejected one", () => {
    expect(githubRefusal({ a: [row(TOKEN_NEEDS_SSO)], b: [row(null)] })).toBe("sso");
    // A new token is the first thing needed; authorizing comes after.
    expect(githubRefusal({ a: [row(TOKEN_NEEDS_SSO)], b: [row(TOKEN_REJECTED)] })).toBe("rejected");
    expect(
      githubRefusal({ a: [row('GitHub 403 Forbidden: {"message":"API rate limit exceeded"}')] }),
    ).toBe(null);
  });

  test("a second sweep on the same connection waits for the first and reads again", async () => {
    const before = useStore.getState();
    const realSweep = api.githubAllPrs;
    useStore.setState({ settings: { github_connected: true } as unknown as Settings, prs: {} });
    let answer = [{ task_id: "t", rows: [row(TOKEN_REJECTED)], ticket: null }];
    let calls = 0;
    const firstReply = Promise.withResolvers<void>();
    api.githubAllPrs = async () => {
      const now = answer;
      if (++calls === 1) await firstReply.promise;
      return structuredClone(now);
    };
    try {
      const first = useStore.getState().refreshPrs();
      answer = [{ task_id: "t", rows: [row(null)], ticket: null }];
      const second = useStore.getState().refreshPrs();
      const whileInFlight = calls;
      firstReply.resolve();
      await Promise.all([first, second]);
      expect(whileInFlight).toBe(1);
      expect(calls).toBe(2);
      expect(githubRefusal(useStore.getState().prs)).toBe(null);
    } finally {
      api.githubAllPrs = realSweep;
      useStore.setState(before, true);
    }
  });
});
