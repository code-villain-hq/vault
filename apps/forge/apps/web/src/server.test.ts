import { expect, test } from "bun:test";
import { normalizeServer } from "./server";

test("a server address becomes its origin", () => {
  expect(normalizeServer(" forge.example.com/ ")).toBe("http://forge.example.com");
  expect(normalizeServer("https://forge.example.com/app")).toBe("https://forge.example.com");
  expect(normalizeServer("localhost:8080")).toBe("http://localhost:8080");
});

test("addresses that aren't web addresses are refused", () => {
  expect(() => normalizeServer("ftp://files.example.com")).toThrow();
  expect(() => normalizeServer("mailto:ada@example.com")).toThrow();
  expect(() => normalizeServer("not an address")).toThrow();
});
