import { expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { Confirm } from "../src/components/ui";

const focused = (danger: boolean) => {
  const html = renderToStaticMarkup(createElement(Confirm, {
    title: "Delete note", body: "This can't be undone.", confirmLabel: "Delete note", danger,
    onConfirm: () => {}, onCancel: () => {},
  }));
  return [...html.matchAll(/<button[^>]*autofocus[^>]*>([^<]*)</g)].map((m) => m[1]);
};

test("a confirmation opens on Cancel, so Enter never confirms by accident", () => {
  expect(focused(true)).toEqual(["Cancel"]);
  expect(focused(false)).toEqual(["Cancel"]);
});
