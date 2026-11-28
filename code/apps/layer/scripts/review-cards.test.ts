import { expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { SCENARIOS } from "../src/__mock__/world";
import { AuthoredCard, ReviewCard } from "../src/components/ReviewCards";
import { createReviewPrReaders } from "../src/lib/review-pr-readers";

const queue = SCENARIOS.reviews().reviews.sources[0];
const onContextMenu = () => {};
const readers = createReviewPrReaders("mock");

function expectClosedDetails(html: string) {
  expect(html).toContain("Comments</button>");
  expect(html).toContain("Check runs</button>");
  expect(html.match(/role="tab" [^>]*aria-selected="false"/g)).toHaveLength(2);
  expect(html).not.toContain('role="tabpanel"');
  expect(html).not.toContain("Reading comments");
  expect(html).not.toContain("Reading check runs");
}

test("a requested fork exposes collapsed readers without replacing GitHub's failing rollup", () => {
  const pr = queue.mine.find((pr) => pr.cross_repo)!;
  const html = renderToStaticMarkup(
    createElement(ReviewCard, { pr, focused: false, readers, onContextMenu }),
  );
  expect(html).toContain("checks failing");
  expectClosedDetails(html);
});

test("an authored PR without a task offers the same readers and keeps Start task", () => {
  const pr = queue.authored!.prs[1];
  const html = renderToStaticMarkup(
    createElement(AuthoredCard, { pr, task: null, readers, onContextMenu }),
  );
  expect(html).toContain("Start task");
  expectClosedDetails(html);
});

test("the details are tabs: opening Check runs closes Comments, and choosing it again closes it", () => {
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
