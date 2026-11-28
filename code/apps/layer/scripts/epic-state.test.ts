import { describe, expect, test } from "bun:test";
import { epicChatOffered, openWaits, subTaskState } from "../src/lib/epicState";
import type { CheckoutPr, SubTaskView } from "../src/lib/types";

const idle = { waiting: false, live: 0 };

function sub(title: string, over: Partial<SubTaskView> = {}): SubTaskView {
  return {
    title,
    key: null,
    existing: false,
    repos: ["api"],
    serves: [],
    steps: 1,
    done: 0,
    waits_for: [],
    task_id: null,
    ...over,
  };
}

function pr(merged: boolean): CheckoutPr {
  return {
    checkout_id: "c",
    repo: "acme/api",
    checks: [],
    checks_state: merged ? "unavailable" : "complete",
    reviews: [],
    past: [],
    verdict: "none",
    base: "main",
    changed: 0,
    to_reply: 0,
    error: null,
    pr: {
      number: 1,
      title: "",
      state: merged ? "closed" : "open",
      draft: false,
      author: "",
      head: "",
      head_sha: "",
      base: "main",
      url: "",
      mergeable_state: null,
      merged,
      comments: 0,
      review_comments: 0,
    },
  };
}

describe("a sub-task's state (EPIC-10)", () => {
  test("its pull requests speak before its agents", () => {
    expect(subTaskState(true, "changes_requested", { waiting: true, live: 1 }, false)).toBe(
      "changes",
    );
    expect(subTaskState(true, "open", { waiting: false, live: 1 }, false)).toBe("in review");
  });

  test("an agent that needs you comes before one at work, and both before waiting", () => {
    expect(subTaskState(true, "none", { waiting: true, live: 2 }, true)).toBe("asking");
    expect(subTaskState(true, "none", { waiting: false, live: 1 }, true)).toBe("working");
    expect(subTaskState(true, "none", idle, true)).toBe("waits");
    expect(subTaskState(true, "none", idle, false)).toBe("ready");
  });

  test("one not filed is only that", () => {
    expect(subTaskState(false, "none", idle, true)).toBe("to file");
  });
});

describe("what a sub-task waits for", () => {
  test("is what has not merged, and one not filed has not", () => {
    const all = [
      sub("Log both sessions", { task_id: "t1" }),
      sub("Retry", { task_id: "t2" }),
      sub("Copy"),
    ];
    const waiting = sub("Say it", { waits_for: ["log both sessions", "Retry", "Copy"] });
    expect(openWaits(waiting, all, { t1: [pr(true)], t2: [pr(false)] })).toEqual(["Retry", "Copy"]);
  });
});

describe("Start the epic's chat in an epic's menu (EPIC-11)", () => {
  const epic = { id: "t-1", issue_key: "ACME-300", epic: true };

  test("is offered for a Jira epic with no task here yet", () => {
    expect(epicChatOffered("ACME-300", [], [])).toBe(true);
  });

  test("is offered for an epic here until its chat has started", () => {
    expect(epicChatOffered("ACME-300", [epic], [{ task_id: "t-2", kind: "agent" }])).toBe(true);
    expect(epicChatOffered("ACME-300", [epic], [{ task_id: "t-1", kind: "shell" }])).toBe(true);
    expect(epicChatOffered("ACME-300", [epic], [{ task_id: "t-1", kind: "agent" }])).toBe(false);
  });

  test("is not offered when the ticket's task is no epic", () => {
    expect(epicChatOffered("ACME-300", [{ ...epic, epic: false }], [])).toBe(false);
  });
});
