import { describe, expect, test } from "bun:test";
import { fold } from "./format";

describe("fold", () => {
  test("finds a word however its umlaut is typed", () => {
    for (const typed of ["Militär", "militar", "MILITAER"]) {
      expect(fold("Militärausgaben 2025").includes(fold(typed))).toBe(true);
    }
  });
  test("reads ß as ss and drops accents", () => {
    expect(fold("Straße")).toBe("strasse");
    expect(fold("Café")).toBe("cafe");
  });
});
