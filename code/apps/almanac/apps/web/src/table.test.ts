import { describe, expect, test } from "bun:test";
import { numberIn } from "./table";

describe("numberIn", () => {
  test("reads a number and its unit", () => {
    expect(numberIn("709 kr")).toBe(709);
    expect(numberIn("3,4 kg")).toBe(3.4);
    expect(numberIn("3.4 kg")).toBe(3.4);
    expect(numberIn("€12")).toBe(12);
    expect(numberIn("−2")).toBe(-2);
  });

  test("reads thousands either way round", () => {
    expect(numberIn("1.525 kr")).toBe(1525);
    expect(numberIn("68 000 kr")).toBe(68000);
    expect(numberIn("1,234.56")).toBe(1234.56);
    expect(numberIn("1.234,56 €")).toBe(1234.56);
  });

  test("leaves words alone", () => {
    expect(numberIn("Ja")).toBeNull();
    expect(numberIn("43, 44")).toBeNull();
    expect(numberIn("nur 43")).toBeNull();
    expect(numberIn("")).toBeNull();
  });
});
