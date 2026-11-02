import { expect, test } from "bun:test";
import type { Newsletter } from "@postrider/types";
import { finds, leastReadFirst, neverOpened, opened, rhythm, sorted } from "./rhythm";

function sender(
  name: string,
  sent: number,
  read: number,
  perWeek = 1,
  newest = "2026-11-02T08:00:00Z",
): Newsletter {
  return {
    account_id: "a",
    name,
    names: [name],
    address: `${name.toLowerCase()}@news.test`,
    sent,
    opened: read,
    recent: sent,
    per_week: perWeek,
    newest_uid: 1,
    newest,
  };
}

test("how often a sender writes", () => {
  expect(rhythm(sender("Z", 90, 0, 7.06))).toBe("Daily");
  expect(rhythm(sender("L", 45, 0, 3.4))).toBe("3 a week");
  expect(rhythm(sender("M", 13, 0, 1.05))).toBe("Weekly");
  expect(rhythm(sender("B", 7, 0, 0.55))).toBe("Every 2 weeks");
  expect(rhythm(sender("N", 3, 0, 0.23))).toBe("Monthly");
  expect(rhythm(sender("R", 2, 0, 0.1))).toBe("Now and then");
  expect(rhythm(sender("O", 1, 0, 1))).toBe("Once");
});

test("how much of it you open", () => {
  expect(opened(sender("Z", 90, 0))).toBe("Never opened");
  expect(opened(sender("H", 22, 3))).toBe("Opened 3 of 22");
  expect(neverOpened(sender("Z", 90, 0))).toBe(true);
  expect(neverOpened(sender("O", 1, 0))).toBe(false);
  expect(neverOpened(sender("H", 22, 3))).toBe(false);
  // One that stopped writing isn't worth leaving.
  expect(neverOpened({ ...sender("S", 5, 0), recent: 0 })).toBe(false);
});

test("the least read come first, then who sends most", () => {
  const sorted = [
    sender("Heise", 22, 22),
    sender("Babylon", 4, 2),
    sender("Spotify", 13, 0),
    sender("Zalando", 90, 0),
  ].sort(leastReadFirst);
  expect(sorted.map((n) => n.name)).toEqual(["Zalando", "Spotify", "Babylon", "Heise"]);
});

test("each head sorts its own way, and the other way round", () => {
  const senders = [
    sender("heise", 22, 22, 5.1, "2026-10-30T08:00:00Z"),
    sender("Babylon", 4, 2, 0.3, "2026-11-01T08:00:00Z"),
    sender("Kino", 1, 0, 1, "2026-11-02T09:00:00Z"),
    sender("Zalando", 90, 0, 7, "2026-10-12T08:00:00Z"),
  ];
  const names = (by: "name" | "newest" | "often" | "opened", reversed = false) =>
    sorted(senders, { by, reversed }).map((n) => n.name);
  expect(names("name")).toEqual(["Babylon", "heise", "Kino", "Zalando"]);
  expect(names("name", true)).toEqual(["Zalando", "Kino", "heise", "Babylon"]);
  expect(names("newest")).toEqual(["Kino", "Babylon", "heise", "Zalando"]);
  // Once is the least often, however recent.
  expect(names("often")).toEqual(["Zalando", "heise", "Babylon", "Kino"]);
  expect(names("opened")).toEqual(["Zalando", "Kino", "Babylon", "heise"]);
});

test("a sender is found by any name it wrote as, or its address", () => {
  const linkedin = {
    ...sender("Anna Berg via LinkedIn", 3, 1),
    names: ["Anna Berg via LinkedIn", "Dimitrios Kombas via LinkedIn"],
    address: "messaging-digest-noreply@linkedin.com",
  };
  expect(finds(linkedin, "dimitrios")).toBe(true);
  expect(finds(linkedin, "kombas linkedin")).toBe(true);
  expect(finds(linkedin, "messaging-digest")).toBe(true);
  expect(finds(linkedin, "dimitrios berlin")).toBe(false);
  expect(finds(sender("Zoë Café", 1, 0), "zoe cafe")).toBe(true);
  expect(finds(sender("Zalando", 1, 0), "")).toBe(true);
});
