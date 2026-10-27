import { describe, expect, test } from "bun:test";
import { groupByStatus } from "../src/lib/ticket-group";
import type { Ticket } from "../src/lib/types";

const t = (key: string, status: string, status_category: string) => ({ key, status, status_category }) as Ticket;

describe("grouping tickets by status (TKT-14)", () => {
  test("puts work under way first, then what waits, then what is done, keeping the sort within each", () => {
    const groups = groupByStatus([
      t("A-1", "Done", "done"),
      t("A-2", "To Do", "new"),
      t("A-3", "In Review", "indeterminate"),
      t("A-4", "In Progress", "indeterminate"),
      t("A-5", "To Do", "new"),
      t("A-6", "In Review", "indeterminate"),
    ]);
    expect(groups.map((g) => g.name)).toEqual(["In Review", "In Progress", "To Do", "Done"]);
    expect(groups.map((g) => g.issues.map((i) => i.key))).toEqual([["A-3", "A-6"], ["A-4"], ["A-2", "A-5"], ["A-1"]]);
  });

  test("joins one status name across projects into one group", () => {
    const groups = groupByStatus([t("A-1", "To Do", "new"), t("B-1", "To Do", "new")]);
    expect(groups).toHaveLength(1);
    expect(groups[0].issues).toHaveLength(2);
  });
});
