import { expect, test } from "bun:test";
import { fromFrame } from "./message";

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
