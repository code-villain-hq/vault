import { describe, expect, test } from "bun:test";
import { nextStop } from "../src/lib/modalFocus";

describe("Tab in a dialog goes round its controls, never out (WIN-6)", () => {
  test("Tab moves on, and from the last control comes back to the first", () => {
    expect(nextStop(3, 0, false)).toBe(1);
    expect(nextStop(3, 2, false)).toBe(0);
  });

  test("⇧Tab moves back, and from the first control goes round to the last", () => {
    expect(nextStop(3, 2, true)).toBe(1);
    expect(nextStop(3, 0, true)).toBe(2);
  });

  test("with the focus on none of them, Tab starts at the first and ⇧Tab at the last", () => {
    expect(nextStop(3, -1, false)).toBe(0);
    expect(nextStop(3, -1, true)).toBe(2);
  });

  test("a dialog with one control keeps the focus on it", () => {
    expect(nextStop(1, 0, false)).toBe(0);
    expect(nextStop(1, 0, true)).toBe(0);
  });
});
