import { expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { PrComments } from "../src/components/PrComments";
import type { PrDetailRead } from "../src/lib/pr-detail-read";
import type { FeedbackNote, PrComments as Snapshot } from "../src/lib/types";

const empty: Snapshot = {
  comments: [],
  reviews: [],
  threads: [],
  comments_state: "complete",
  reviews_state: "complete",
  threads_state: "complete",
};
const source = "https://git.example.com/acme/api/pull/42";

function note(author: string, url: string, body: string): FeedbackNote {
  return { author, url, body, state: null, at: null, bot: false };
}

function markup(read: PrDetailRead<Snapshot>) {
  return renderToStaticMarkup(createElement(PrComments, { read, onOpen: () => {} }));
}

test("empty, loading and request failure do not read alike", () => {
  const waiting = markup({ status: "loading" });
  expect(waiting).toContain("Reading comments");
  expect(waiting).not.toContain("No conversation comments");
  const failed = markup({ status: "failed", message: "Could not read this pull request." });
  expect(failed).toContain('role="alert"');
  expect(failed).toContain("Could not read this pull request.");
  expect(failed).not.toContain("No conversation comments");
  const complete = markup({ status: "ready", data: empty });
  expect(complete).toContain("No conversation comments");
  expect(complete).toContain("No submitted review text");
  expect(complete).toContain("No inline review threads");
});

test("an incomplete section keeps its entries while another section can be unavailable", () => {
  const html = markup({
    status: "ready",
    data: {
      ...empty,
      comments: [note("lee", `${source}#comment`, "Available comment")],
      comments_state: "incomplete",
      reviews_state: "unavailable",
    },
  });
  expect(html).toContain("conversation comment list is incomplete");
  expect(html).toContain("Available comment");
  expect(html).toContain("read submitted review text");
  expect(html).not.toContain("No submitted review text");
  expect(html).toContain("No inline review threads");
});

test("each note and reply keeps its source and the thread keeps its context and labels", () => {
  const html = markup({
    status: "ready",
    data: {
      ...empty,
      comments: [note("lee", `${source}#comment`, "**Conversation**")],
      reviews: [{ ...note("sam", `${source}#review`, "Review body"), state: "CHANGES_REQUESTED" }],
      threads: [
        {
          path: "src/file.ts",
          line: 12,
          start_line: 10,
          code: [{ n: 11, op: "+", text: "let attempt = 1;" }],
          resolved: false,
          outdated: false,
          url: `${source}#first`,
          comments: [
            note("jo", `${source}#first`, "Thread body"),
            note("pat", `${source}#reply`, "Reply body"),
          ],
        },
        {
          path: "src/old.ts",
          line: null,
          start_line: null,
          code: [],
          resolved: true,
          outdated: true,
          url: `${source}#settled`,
          comments: [note("jo", `${source}#settled`, "Settled thread")],
        },
      ],
    },
  });
  expect(html).toContain("<strong>Conversation</strong>");
  expect(html).toContain("requested changes");
  expect(html).toContain("src/file.ts:10-12");
  expect(html).toContain("let attempt = 1;");
  for (const fragment of ["comment", "review", "first", "reply"]) {
    expect(html).toContain(`href="${source}#${fragment}"`);
  }
  expect(html).toContain("pat");
  expect(html).toMatch(/<details(?![^>]*\bopen\b)[^>]*><summary[^>]*>.*src\/old.ts/);
  expect(html).toContain("Resolved");
  expect(html).toContain("Outdated");
});

test("GitHub content cannot add HTML or unsafe source and Markdown links", () => {
  const html = markup({
    status: "ready",
    data: {
      ...empty,
      comments: [
        note("lee", "javascript:bad()", '<img src="bad">[visible](javascript:bad)'),
        note("sam", "https://user:pass@git.example.com/acme/api/1", "Another comment"),
        note("pat", "/acme/api/1", "A relative source"),
      ],
    },
  });
  expect(html).toContain("visible");
  expect(html).toContain("Another comment");
  expect(html).not.toContain("<img");
  expect(html).not.toContain("href=");
});
