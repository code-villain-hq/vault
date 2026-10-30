import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { about, lead, Prose } from "./prose";

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

test("a topic is about the first sentence of its brief", () => {
  expect(
    about(
      "Geschichte global erklärt, Europa als Akteur unter anderen. Immer Deutsch.\nQuellen nennen.",
    ),
  ).toBe("Geschichte global erklärt, Europa als Akteur unter anderen.");
  expect(about("Zimmerpflanzen, ich schicke Fotos\nThe houseplants at home.")).toBe(
    "Zimmerpflanzen, ich schicke Fotos",
  );
  expect(about("  ")).toBe("");
});

test("a section's tile shows its first sentence", () => {
  expect(lead("**Kuba** war spanische Kolonie bis 1898. Danach hielten die USA die Insel.")).toBe(
    "Kuba war spanische Kolonie bis 1898.",
  );
  expect(
    lead("**9/11 in Zahlen**: Am 11. September 2001 starben knapp 3.000 Menschen. Die Kriege"),
  ).toBe("9/11 in Zahlen: Am 11. September 2001 starben knapp 3.000 Menschen.");
  expect(lead("Rohstoffe, z. B. Kupfer. Und mehr.")).toBe("Rohstoffe, z. B. Kupfer.");
  expect(lead("Will in den Hausflur (4. Stock), geht bis zur Treppe. Kommt zurück.")).toBe(
    "Will in den Hausflur (4. Stock), geht bis zur Treppe.",
  );
  expect(lead("**Wer kauft die Ware?** Kapital verkauft an andere. Entlassene nicht.")).toBe(
    "Wer kauft die Ware? Kapital verkauft an andere.",
  );
  expect(lead("Ein Satz ohne Punkt")).toBe("Ein Satz ohne Punkt");
});
