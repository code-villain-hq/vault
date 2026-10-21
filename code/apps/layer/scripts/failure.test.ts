import { describe, expect, test } from "bun:test";
import { api, failureText } from "../src/lib/api";
import { useStore } from "../src/store";

// The backend, as far as recording a failure asks it.
const backend = api as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;

describe("the sentence a failure is shown as", () => {
  test("starts with a capital and ends with a full stop", () => {
    expect(failureText("git push was rejected")).toBe("Git push was rejected.");
  });

  test("keeps the backend's own ending", () => {
    expect(failureText("Retrying…")).toBe("Retrying…");
    expect(failureText("Not found (404)")).toBe("Not found (404)");
  });

  test("still says something when the error said nothing", () => {
    expect(failureText(new Error(""))).toBe("Something went wrong. Try again.");
  });
});

describe("a failure in the top bar's status", () => {
  test("is kept in the message center, and never as a toast", () => {
    const kept: unknown[][] = [];
    backend.addMessage = async (...args: unknown[]) => { kept.push(args); };
    useStore.getState().fail("couldn't reach Jira");
    expect(useStore.getState().failure?.text).toBe("Couldn't reach Jira.");
    expect(useStore.getState().toasts).toEqual([]);
    expect(kept).toEqual([["error", "error", "Couldn't reach Jira.", undefined]]);
  });

  test("gives way to a newer one", () => {
    backend.addMessage = async () => {};
    useStore.getState().fail("first");
    useStore.getState().fail("second");
    expect(useStore.getState().failure?.text).toBe("Second.");
  });
});
