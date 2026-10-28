import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { Prose } from "./prose";

const html = (text: string) => renderToStaticMarkup(Prose({ text }));

test("keeps bold and italic", () => {
  expect(html("Im ersten Band des *Kapital*, **persönlich frei**.")).toBe(
    '<div class="flex flex-col gap-2"><p>Im ersten Band des <em>Kapital</em>, <strong class="font-semibold">persönlich frei</strong>.</p></div>',
  );
});

test("leaves a lone star and a sum as they are", () => {
  expect(html("2 * 3 * 4 and a star *")).toContain("<p>2 * 3 * 4 and a star *</p>");
});

test("a star at the start of a line is a list", () => {
  expect(html("* one\n* *two*")).toContain("<li>one</li><li><em>two</em></li>");
});
