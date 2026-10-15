import { expect, test } from "bun:test";
import type { Issue } from "@forge/types";
import { applyFilters, toggle } from "./issue-filter";

const issue = (id: string, fields: Partial<Issue>): Issue =>
  ({
    id,
    assignee_id: null,
    priority: 0,
    label_ids: [],
    project_id: null,
    service_id: null,
    ...fields,
  }) as Issue;

const issues = [
  issue("a", { assignee_id: "ada", priority: 1, label_ids: ["bug"] }),
  issue("b", { assignee_id: "bob", priority: 2, label_ids: ["bug", "ui"] }),
  issue("c", { priority: 1, project_id: "p" }),
];
const ids = (list: Issue[]) => list.map((i) => i.id);

test("no filters keep everything", () => {
  expect(ids(applyFilters(issues, {}))).toEqual(["a", "b", "c"]);
  expect(ids(applyFilters(issues, { assignee: [] }))).toEqual(["a", "b", "c"]);
});

test("values of one field are alternatives", () => {
  expect(ids(applyFilters(issues, { assignee: ["ada", "bob"] }))).toEqual(["a", "b"]);
  expect(ids(applyFilters(issues, { label: ["ui"] }))).toEqual(["b"]);
});

test("different fields all have to match", () => {
  expect(ids(applyFilters(issues, { priority: ["1"], label: ["bug"] }))).toEqual(["a"]);
});

test("empty stands for none", () => {
  expect(ids(applyFilters(issues, { assignee: [""] }))).toEqual(["c"]);
  expect(ids(applyFilters(issues, { project: [""] }))).toEqual(["a", "b"]);
});

test("toggling adds and removes a value", () => {
  const once = toggle({}, "label", "bug");
  expect(once).toEqual({ label: ["bug"] });
  expect(toggle(once, "label", "bug")).toEqual({ label: [] });
});
