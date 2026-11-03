import { describe, expect, test } from "bun:test";
import { unframe } from "../src/lib/paneFeed";

describe("a terminal's live feed", () => {
  test("a batch says where it ends, then its bytes", () => {
    // As the backend frames it: 258, then "hi".
    const frame = new Uint8Array([0, 0, 0, 0, 0, 0, 1, 2, 104, 105]).buffer;
    const [bytes, end] = unframe(frame);
    expect(end).toBe(258);
    expect(new TextDecoder().decode(bytes)).toBe("hi");
  });

  test("past 4 GB of output the position still holds", () => {
    const frame = new ArrayBuffer(8);
    new DataView(frame).setBigUint64(0, 5_000_000_000n);
    const [bytes, end] = unframe(frame);
    expect(end).toBe(5_000_000_000);
    expect(bytes.length).toBe(0);
  });
});
