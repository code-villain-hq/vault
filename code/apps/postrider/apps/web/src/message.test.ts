import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fromFrame, paper, REPORTER, REPORTER_HASH } from "./message";

test("only what the frame's own script says is heard", () => {
  const token = crypto.randomUUID();
  expect(fromFrame({ key: "e", token }, token)).toEqual({ key: "e", token });
  // A page the frame was sent to doesn't know the token.
  expect(fromFrame({ key: "Backspace" }, token)).toBeUndefined();
  expect(fromFrame({ link: "https://evil.test", token: "guess" }, token)).toBeUndefined();
  expect(fromFrame({ height: 10, token: undefined }, token)).toBeUndefined();
  expect(fromFrame(null, token)).toBeUndefined();
  expect(fromFrame(token, token)).toBeUndefined();
});

// A mail's frame has the window's policy as well as its own: a script the
// window's doesn't allow by hash never runs, and the mail is never measured.
test("the frame's script is the one both policies allow", () => {
  expect(`sha256-${createHash("sha256").update(REPORTER).digest("base64")}`).toBe(REPORTER_HASH);
  const window = JSON.parse(
    readFileSync(`${import.meta.dir}/../../desktop/src-tauri/tauri.conf.json`, "utf8"),
  );
  expect(window.app.security.csp["script-src"].split(" ")).toContain(`'${REPORTER_HASH}'`);
  // No DOM here: a mail with no refresh in it, as the stand-in finds.
  globalThis.DOMParser ??= class {
    parseFromString() {
      return { querySelectorAll: () => [] };
    }
  } as unknown as typeof DOMParser;
  const { srcDoc, token } = paper("<p>Hi</p>");
  expect(srcDoc).toContain(`script-src '${REPORTER_HASH}';`);
  expect(srcDoc).toContain(`<script data-token="${token}">${REPORTER}</script>`);
});

// Held back, a mail loads nothing from the web: its sender can't see it was
// read. What it carries itself still shows.
test("a mail's pictures from the web wait to be asked for", () => {
  globalThis.DOMParser ??= class {
    parseFromString() {
      return { querySelectorAll: () => [] };
    }
  } as unknown as typeof DOMParser;
  const held = paper("<p>Hi</p>", false).srcDoc;
  expect(held).toContain("img-src data:;");
  const policy = held.match(/Content-Security-Policy" content="([^"]*)"/)?.[1];
  expect(policy).not.toMatch(/https?:/);
  expect(paper("<p>Hi</p>", true).srcDoc).toContain("img-src https: http: data:;");
});
