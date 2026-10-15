import { expect, test } from "bun:test";
import { contrast, readable, shadeFor } from "./contrast";

test("contrast follows WCAG", () => {
  expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
  expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 1);
});

test("colours that are readable stay as they are", () => {
  expect(shadeFor("#5b5bd6", ["#ffffff"], "#000000", 3)).toBe("#5b5bd6");
});

test("pale colours get just dark enough for light mode, and the reverse", () => {
  const yellow = shadeFor("#f2c94c", ["#ffffff", "#ececf0"], "#000000", 3);
  expect(contrast(yellow, "#ececf0")).toBeGreaterThanOrEqual(3);
  expect(contrast(yellow, "#ececf0")).toBeLessThan(3.2);
  expect(readable("#f2c94c")).toStartWith("light-dark(#");
  expect(readable("tomato")).toBe("tomato");
});
