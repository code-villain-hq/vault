import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { PrCheckList, PrCheckStatus } from "../src/components/PrChecks";
import { checkRunsView } from "../src/lib/pr-checks";
import type { CheckRun, ReadState } from "../src/lib/types";

const failedUrl = "https://github.example.com/acme/api/runs/1";

function finished(conclusion: string | null, name = "build", url: string | null = null): CheckRun {
  return { name, status: "completed", conclusion, url };
}

const queued: CheckRun = { name: "test", status: "queued", conclusion: null, url: null };

describe("check runs on a pull request", () => {
  test.each([false, true])(
    "a same-name success never hides the failure (reversed: %s)",
    (reverse) => {
      const runs = [finished("success"), finished("failure", "build", failedUrl)];
      const view = checkRunsView({
        checks: reverse ? runs.reverse() : runs,
        checks_state: "complete",
      });
      expect(view.summary).toBe("failing");
      expect(view.runs).toHaveLength(1);
      expect(view.runs[0]).toMatchObject({ name: "build", label: "Failed", url: failedUrl });
    },
  );

  test("failure outranks pending, and pending outranks completed runs", () => {
    const checks = [finished("success"), finished("cancelled", "lint"), queued];
    expect(checkRunsView({ checks, checks_state: "complete" }).summary).toBe("pending");
    expect(
      checkRunsView({ checks: [...checks, finished("timed_out")], checks_state: "complete" })
        .summary,
    ).toBe("failing");
  });

  test.each([
    ["empty", []],
    ["skipped", [finished("skipped")]],
    ["skipped", [finished("neutral")]],
    ["cancelled", [finished("cancelled")]],
    ["cancelled", [finished("success"), finished("cancelled")]],
    ["cancelled", [finished("success"), finished("cancelled", "lint")]],
    ["passing", [finished("skipped"), finished("success")]],
    ["unknown", [finished("future_outcome"), finished("success")]],
    ["unknown", [finished(null)]],
    ["unknown", [{ ...queued, conclusion: "success" }]],
    ["unknown", [{ ...queued, status: "future_status", conclusion: "success" }]],
  ] as const)("%s is distinguished on a complete read", (summary, checks) => {
    expect(checkRunsView({ checks: [...checks], checks_state: "complete" }).summary).toBe(summary);
  });

  test.each([
    ["unavailable", [], "unavailable"],
    ["incomplete", [], "incomplete"],
    ["incomplete", [finished("success")], "incomplete"],
    ["incomplete", [queued, finished("success")], "pending"],
    ["incomplete", [queued, finished("failure")], "failing"],
  ] as const satisfies readonly (readonly [ReadState, CheckRun[], string])[])(
    "%s stays visible even when some outcomes are known",
    (state, checks, summary) => {
      const view = checkRunsView({ checks: [...checks], checks_state: state });
      expect(view.summary).toBe(summary);
      expect(view.state).toBe(state);
      expect(view.runs).toHaveLength(checks.length);
      const html = renderToStaticMarkup(createElement(PrCheckList, { view, onOpen: () => {} }));
      expect(html).toContain(state === "unavailable" ? "Couldn’t read check runs" : "incomplete");
      expect(html).not.toContain("No checks reported");
      const status = renderToStaticMarkup(createElement(PrCheckStatus, { view }));
      expect(status).toContain(state);
      expect(status).not.toContain("Checks passing");
    },
  );

  test("readable rows keep safe web links without letting other schemes open", () => {
    const view = checkRunsView({
      checks: [
        finished("failure", "build", failedUrl),
        finished("success", "lint", "javascript:void(0)"),
        finished("success", "format", "file:///tmp/check"),
        finished("success", "typecheck", "//github.example.com/acme/api/runs/4"),
        finished("success", "test", "http://github.example.com/acme/api/runs/5"),
        finished("success", "deploy", "https://username@github.example.com/acme/api/runs/6"),
        finished("success", "smoke", "https://"),
      ],
      checks_state: "complete",
    });
    expect(view.runs.map((r) => r.url)).toEqual([
      failedUrl,
      null,
      null,
      null,
      "http://github.example.com/acme/api/runs/5",
      null,
      null,
    ]);
    const html = renderToStaticMarkup(createElement(PrCheckList, { view, onOpen: () => {} }));
    expect(html).toContain("Open build on GitHub");
    expect(html).not.toContain("Open lint on GitHub");
    expect(html).toContain("Failed");
    expect(html).toContain("Passed");
  });
});
