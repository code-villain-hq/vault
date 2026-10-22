import { expect, test } from "bun:test";
import { normalizeServer } from "./server";

test("a server address becomes its origin", () => {
  expect(normalizeServer(" forge.example.com/ ")).toBe("https://forge.example.com");
  expect(normalizeServer("https://forge.example.com/app")).toBe("https://forge.example.com");
  expect(normalizeServer("http://forge.example.com")).toBe("http://forge.example.com");
});

test("a server on this Mac is http unless it says otherwise", () => {
  expect(normalizeServer("localhost:7390")).toBe("http://localhost:7390");
  expect(normalizeServer("127.0.0.1:7380")).toBe("http://127.0.0.1:7380");
  expect(normalizeServer("[::1]:7380")).toBe("http://[::1]:7380");
  expect(normalizeServer("forge.localhost:7380")).toBe("http://forge.localhost:7380");
  expect(normalizeServer("https://localhost:8443")).toBe("https://localhost:8443");
});

test("addresses that aren't web addresses are refused", () => {
  expect(() => normalizeServer("ftp://files.example.com")).toThrow();
  expect(() => normalizeServer("mailto:ada@example.com")).toThrow();
  expect(() => normalizeServer("not an address")).toThrow();
});
