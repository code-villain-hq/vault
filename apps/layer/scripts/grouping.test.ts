import { describe, expect, test } from "bun:test";
import { byEpic, epicsOf, groupBy, sortPrs, ticketKeyIn } from "../src/lib/grouping";
import type { JiraIssue } from "../src/lib/types";

const issue = (key: string, epic_key: string | null, epic_summary: string | null, issue_type = "Story") =>
  ({ key, summary: `${key} title`, epic_key, epic_summary, issue_type }) as JiraIssue;

describe("finding a pull request's ticket", () => {
  test("reads the key a branch or title starts with, as pr_task.rs does", () => {
    expect(ticketKeyIn("ACME-123-fix-login")).toBe("ACME-123");
    expect(ticketKeyIn("feature/ACME-9")).toBe("ACME-9");
    expect(ticketKeyIn("ACME-12 Document the retry settings")).toBe("ACME-12");
  });

  test("takes nothing that only looks like one", () => {
    expect(ticketKeyIn("patch-1")).toBeNull();
    expect(ticketKeyIn("fix utf-8 decoding")).toBeNull();
    expect(ticketKeyIn("")).toBeNull();
  });
});

describe("grouping a list", () => {
  test("keeps the order it was sorted in, and puts the ungrouped last", () => {
    const groups = groupBy(["a1", "x", "b1", "a2"], (s) => (s === "x" ? null : { id: s[0], label: s[0] }), "None");
    expect(groups.map((g) => [g.id, g.items])).toEqual([["a", ["a1", "a2"]], ["b", ["b1"]], ["", ["x"]]]);
  });

  test("labels an epic by its title, and by its key when Jira sent no title", () => {
    const groups = byEpic(["t1", "t2"], (t) => (t === "t1" ? { key: "E-1", summary: "Search" } : { key: "E-2", summary: "" }));
    expect(groups.map((g) => [g.label, g.key])).toEqual([["Search", "E-1"], ["E-2", undefined]]);
  });
});

describe("which epic a ticket is in", () => {
  test("an epic is its own, and a ticket with none is known to have none", () => {
    const epics = epicsOf(
      [issue("A-1", "E-1", "E-1 title"), issue("E-1", null, null, "Epic"), issue("A-2", null, null)],
      (t) => t === "Epic",
    );
    expect(epics["A-1"]).toEqual({ key: "E-1", summary: "E-1 title" });
    expect(epics["E-1"]).toEqual({ key: "E-1", summary: "E-1 title" });
    expect(epics["A-2"]).toBeNull();
  });

  test("an epic link with no title takes it from the epic when that was sent", () => {
    const epics = epicsOf([issue("A-1", "E-1", null), issue("E-1", null, null, "Epic")], (t) => t === "Epic");
    expect(epics["A-1"]).toEqual({ key: "E-1", summary: "E-1 title" });
  });
});

describe("sorting pull requests", () => {
  const pr = (n: number, updated_at: string, additions = 0) => ({ n, updated_at, additions, deletions: 0 });
  const prs = [pr(1, "2026-10-01T00:00:00Z", 500), pr(2, "2026-10-03T00:00:00Z", 10), pr(3, "2026-10-02T00:00:00Z", 10)];

  test("newest, oldest and smallest first", () => {
    expect(sortPrs(prs, "newest").map((p) => p.n)).toEqual([2, 3, 1]);
    expect(sortPrs(prs, "oldest").map((p) => p.n)).toEqual([1, 3, 2]);
    // Equal sizes fall back to the newest first.
    expect(sortPrs(prs, "smallest").map((p) => p.n)).toEqual([2, 3, 1]);
  });
});
