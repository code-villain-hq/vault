import { describe, expect, test } from "bun:test";
import { taskMatches } from "../src/lib/task-search";

const task = {
  name: "DT-4826 [FE] customer-portal: login fails on Safari",
  branch: "michael/DT-4826",
  issue_key: "DT-4826",
};
const epic = { key: "DT-19433", summary: "Auth and Security - P1 Authentication" };
const ticket = {
  summary: "[FE] customer-portal: login fails on Safari",
  description: "The cookie is dropped when webView=true.",
  epic_key: null,
  epic_summary: null,
};

describe("finding a task in the sidebar (TASK-19)", () => {
  test("by its key, whole or in part, in any case", () => {
    expect(taskMatches(task, "DT-4826")).toBe(true);
    expect(taskMatches(task, "dt-48")).toBe(true);
    expect(taskMatches(task, "4826")).toBe(true);
    expect(taskMatches(task, "DT-4827")).toBe(false);
  });

  test("by every word of its title, in any order", () => {
    expect(taskMatches(task, "safari login")).toBe(true);
    expect(taskMatches(task, "safari logout")).toBe(false);
  });

  test("by its epic's key or title", () => {
    expect(taskMatches(task, "DT-19433", epic)).toBe(true);
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
