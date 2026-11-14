import { describe, expect, test } from "bun:test";
import { taskMatches } from "../src/lib/task-search";

const task = {
  name: "ACME-4826 [FE] web: login fails on Safari",
  branch: "michael/ACME-4826",
  issue_key: "ACME-4826",
};
const epic = { key: "ACME-19433", summary: "Accounts - Authentication" };
const ticket = {
  summary: "[FE] web: login fails on Safari",
  description: "The cookie is dropped when webView=true.",
  epic_key: null,
  epic_summary: null,
};

describe("finding a task in the sidebar (TASK-19)", () => {
  test("by its key, whole or in part, in any case", () => {
    expect(taskMatches(task, "ACME-4826")).toBe(true);
    expect(taskMatches(task, "acme-48")).toBe(true);
    expect(taskMatches(task, "4826")).toBe(true);
    expect(taskMatches(task, "ACME-4827")).toBe(false);
  });

  test("by every word of its title, in any order", () => {
    expect(taskMatches(task, "safari login")).toBe(true);
    expect(taskMatches(task, "safari logout")).toBe(false);
  });

  test("by its epic's key or title", () => {
    expect(taskMatches(task, "ACME-19433", epic)).toBe(true);
    expect(taskMatches(task, "authentication", epic)).toBe(true);
    expect(taskMatches(task, "authentication")).toBe(false);
  });

  test("by its ticket's description, when the ticket is on your list", () => {
    expect(taskMatches(task, "webview cookie", null, ticket)).toBe(true);
    expect(taskMatches(task, "webview")).toBe(false);
  });

  test("an empty search keeps every task", () => {
    expect(taskMatches(task, "  ")).toBe(true);
  });
});
