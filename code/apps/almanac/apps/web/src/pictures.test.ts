import { describe, expect, test } from "bun:test";
import type { Point } from "@almanac/types";
import { captionPatch } from "./pictures";

const picture = (cells: Point["cells"]): Point => ({
  id: "p1",
  workspace_id: "w",
  topic_id: "t",
  block_id: "b1",
  theme: "",
  text: "",
  happened: "",
  facet: "",
  look: "",
  amount: null,
  note: "",
  cells,
  done: false,
  position: 0,
  source: null,
  source_url: null,
  talk_id: null,
  seen: true,
  created_at: "",
  updated_at: "",
});

describe("captionPatch", () => {
  test("sends the words and only the photo's cell, so another device's cells stay", () => {
    const patch = captionPatch(picture({ photo: "abc", Ort: "Küche" }), "Am Herd");
    expect(patch.text).toBe("Am Herd");
    expect(patch.changed_cells).toEqual({ photo: "abc" });
  });
});
