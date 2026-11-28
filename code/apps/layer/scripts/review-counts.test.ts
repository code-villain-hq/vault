import { describe, expect, test } from "bun:test";
import { reviewCounts } from "../src/lib/derive";
import type { AuthoredPr, ReviewQueue, ReviewRequest, SourceReviews } from "../src/lib/types";

const req = (number: number) => ({ number }) as ReviewRequest;
const mine = (number: number) => ({ number }) as AuthoredPr;

function source(extra: Partial<SourceReviews> = {}): SourceReviews {
  return {
    source: "github.com",
    mine: [req(1)],
    mine_more: false,
    mine_error: null,
    team: null,
    authored: { prs: [1, 2, 3, 4, 5, 6, 7].map(mine), more: false, error: null },
    ...extra,
  };
}

function queue(extra: Partial<SourceReviews> = {}): ReviewQueue {
  return { sources: [source(extra)] };
}

describe("what the Reviews view counts", () => {
  test("counts both sides, not their sum", () => {
    expect(reviewCounts(queue())).toEqual({ toReview: "1", yours: "7" });
  });

  test("leaves out a team whose lookup failed", () => {
    const team = { slug: "acme/fe", name: "fe", prs: [req(2)], more: false, error: "no read:org" };
    expect(reviewCounts(queue({ team })).toReview).toBe("1");
  });

  test("says when a list was cut", () => {
    expect(reviewCounts(queue({ mine_more: true })).toReview).toBe("1+");
  });

  test("has no count for your PRs when they could not be read", () => {
    expect(
      reviewCounts(queue({ authored: { prs: [], more: false, error: "rate limited" } })).yours,
    ).toBeUndefined();
  });

  test("has nothing before GitHub answers", () => {
    expect(reviewCounts(null)).toEqual({});
  });
  test("keeps healthy counts and limits when another source fails", () => {
    const privateSource = source({
      source: "git.example.com",
      mine: [],
      mine_error: "not connected",
      authored: { prs: [mine(7)], more: true, error: null },
    });
    expect(reviewCounts({ sources: [source(), privateSource] })).toEqual({
      toReview: "1",
      yours: "8+",
    });
  });
});
