import { describe, expect, test } from "bun:test";
import { outsideZoom } from "../src/lib/outsideZoom";

describe("a terminal inside the interface scale (PANE-15)", () => {
  test("undoes the zoom, so a click lands on the row under the pointer", () => {
    expect(outsideZoom(13, 1.15).zoom * 1.15).toBeCloseTo(1);
  });

  test("scales its text instead, so it looks as it did", () => {
    expect(outsideZoom(13, 1.15).fontSize).toBeCloseTo(14.95);
  });

  test("is untouched at 100%", () => {
    expect(outsideZoom(13, 1)).toEqual({ zoom: 1, fontSize: 13 });
  });
});
