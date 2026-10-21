import { expect, test } from "bun:test";
import { remoteKey } from "./remote";

test("every spelling of a remote is one key, as the server's", () => {
  for (const url of [
    "git@github.com:acme/billing.git",
    "ssh://git@github.com/acme/billing",
    "https://GitHub.com/acme/billing/",
    "github.com/acme/billing",
    "ssh://git@github.com:22/acme/billing.git",
  ]) {
    expect(remoteKey(url)).toBe("github.com/acme/billing");
  }
});

test("a remote on disk is its path", () => {
  expect(remoteKey("/Users/ada/code/api")).toBe("/Users/ada/code/api");
});
