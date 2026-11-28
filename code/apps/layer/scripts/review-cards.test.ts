import { expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { SCENARIOS } from "../src/__mock__/world";
import { PrCheckList } from "../src/components/PrChecks";
import { AuthoredCard, ReviewCard } from "../src/components/ReviewCards";
import { createReviewPrReaders } from "../src/lib/review-pr-readers";

const queue = SCENARIOS.reviews().reviews.sources[0];
const onContextMenu = () => {};
const readers = createReviewPrReaders("mock");

function expectClosedDetails(html: string) {
  expect(html).toContain("Show check runs");
  expect(html).toContain("Show comments");
  expect(html.match(/aria-expanded="false"/g)).toHaveLength(2);
  expect(html).not.toContain('role="region"');
  expect(html).not.toContain("Reading comments");
  expect(html).not.toContain("Reading check runs");
}

test("a requested fork exposes collapsed readers without replacing GitHub's failing rollup", () => {
  const pr = queue.mine.find((pr) => pr.cross_repo)!;
  const html = renderToStaticMarkup(
    createElement(ReviewCard, { pr, focused: false, readers, onContextMenu }),
  );
  expect(html).toContain("Checks failing");
  expectClosedDetails(html);
});

test("a request says how long ago it was asked of you, and marks a wait of days", () => {
  const fork = queue.mine.find((pr) => pr.cross_repo)!;
  const html = renderToStaticMarkup(
    createElement(ReviewCard, { pr: fork, focused: false, readers, onContextMenu }),
  );
  expect(html).toContain('class="when long"');
  expect(html).toContain(">asked 4d<");
  const fresh = queue.mine.find((pr) => pr.number === 88)!;
  const recent = renderToStaticMarkup(
    createElement(ReviewCard, { pr: fresh, focused: false, readers, onContextMenu }),
  );
  expect(recent).toContain('class="when"');
});

test("an authored PR names who it waits on and since when, and who answered", () => {
  const waiting = queue.authored!.prs.find((pr) => pr.number === 12)!;
  const html = renderToStaticMarkup(
    createElement(AuthoredCard, { pr: waiting, task: null, readers, onContextMenu }),
  );
  expect(html).toContain('<span class="who">ana</span><span class="said warn">3d</span>');
  expect(html).toContain('<span class="who">@fe</span>');
  expect(html).toContain("Conflicts");
  const answered = queue.authored!.prs.find((pr) => pr.number === 42)!;
  const told = renderToStaticMarkup(
    createElement(AuthoredCard, { pr: answered, task: null, readers, onContextMenu }),
  );
  expect(told).toContain("asked for changes");
  expect(told).toContain('<span class="mark ok">✓</span><span class="who">bo</span>');
  expect(told).toContain(">1 unresolved<");
});

test("an open check list folds what passed while something failed", () => {
  const view = {
    summary: "failing" as const,
    state: "complete" as const,
    runs: [
      { name: "build", label: "Failed", outcome: "failing" as const, url: null },
      { name: "lint", label: "Passed", outcome: "passing" as const, url: null },
      { name: "types", label: "Passed", outcome: "passing" as const, url: null },
    ],
  };
  const html = renderToStaticMarkup(
    createElement(PrCheckList, { view, onOpen: () => {}, fold: true }),
  );
  expect(html).toContain("build");
  expect(html).not.toContain("lint");
  expect(html).toContain(">2 passed<");
  const all = renderToStaticMarkup(createElement(PrCheckList, { view, onOpen: () => {} }));
  expect(all).toContain("lint");
});

test("an authored PR without a task offers the same readers and keeps Start task", () => {
  const pr = queue.authored!.prs[1];
  const html = renderToStaticMarkup(
    createElement(AuthoredCard, { pr, task: null, readers, onContextMenu }),
  );
  expect(html).toContain("Start task");
  expectClosedDetails(html);
});

test("one detail opens at a time: opening Check runs closes Comments, and asking again closes it", () => {
  const tabs = createReviewPrReaders("tabs");
  const pr = queue.mine[0];
  const open = () => {
    const entry = tabs.store.getState().get(tabs.keyOf(pr));
    return entry && { comments: entry.comments, checks: entry.checks };
  };
  tabs.toggle(pr, "comments");
  expect(open()).toEqual({ comments: true, checks: false });
  tabs.toggle(pr, "checks");
  expect(open()).toEqual({ comments: false, checks: true });
  tabs.toggle(pr, "checks");
  expect(open()).toBeUndefined();
});
