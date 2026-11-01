import { Fragment, type ReactNode } from "react";

/**
 * An answer as written: paragraphs and short lists, with **bold**, *italic*
 * and [links](https://…) kept.
 * Answers are told to stay short and plain, so this is all they use.
 */
export function Prose({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) {
      blocks.push(<p key={blocks.length}>{inline(paragraph.join(" "))}</p>);
      paragraph = [];
    }
    if (list) {
      const items = list.items.map((item, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: an answer's lines don't move
        <li key={i}>{inline(item)}</li>
      ));
      blocks.push(
        list.ordered ? (
          <ol key={blocks.length} className="list-decimal pl-5">
            {items}
          </ol>
        ) : (
          <ul key={blocks.length} className="list-disc pl-5">
            {items}
          </ul>
        ),
      );
      list = null;
    }
  };
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    const bullet = /^[-*•]\s+(.*)$/.exec(line);
    const numbered = /^\d+[.)]\s+(.*)$/.exec(line);
    const item = bullet ?? numbered;
    if (!line) {
      flush();
    } else if (item) {
      if (paragraph.length || (list && list.ordered !== !!numbered)) flush();
      list ??= { ordered: !!numbered, items: [] };
      list.items.push(item[1] ?? "");
    } else {
      if (list) flush();
      paragraph.push(line.replace(/^#+\s*/, ""));
    }
  }
  flush();
  return <div className="flex flex-col gap-2">{blocks}</div>;
}

/**
 * What a topic is about: the first sentence of what it sticks to. The rest of
 * that brief says how its talks answer, which nobody needs to read in a list.
 */
export function about(brief: string): string {
  const line = brief.trim().split("\n")[0] ?? "";
  const [first] = new Intl.Segmenter(undefined, { granularity: "sentence" }).segment(line);
  return first?.segment.trim() ?? "";
}

/** *Italic*: a word or more, not a lone star or a sum's. */
const ITALIC = String.raw`\*[^*\s](?:[^*]*[^*\s])?\*`;
/** **Bold**, which can hold *italic*, at its end too: "**des *Kapital***". */
const BOLD = String.raw`\*\*(?:[^*]|${ITALIC})+?\*\*`;

/** A line as written, with **bold**, *italic* and links kept: an answer's, or a point's. */
export function inline(text: string): ReactNode {
  // The marked parts sit at the odd places the split leaves them, marks and
  // all: **bold**, which can hold italic, and *italic*, either with links.
  return text.split(new RegExp(`(${BOLD}|${ITALIC})`)).map((part, i) => {
    if (i % 2 === 0) return linked(part, i);
    return part.startsWith("**") ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: an answer's words don't move
      <strong key={i} className="font-semibold">
        {inline(part.slice(2, -2))}
      </strong>
    ) : (
      // biome-ignore lint/suspicious/noArrayIndexKey: an answer's words don't move
      <em key={i}>{linked(part.slice(1, -1), i)}</em>
    );
  });
}

/** "[Scorett Outlet](https://…)", and its name and address as the split leaves them. */
const LINK = /\[([^\]]+)\]\s?\((https?:\/\/(?:[^\s()]|\([^\s()]*\))+)\)/;

/** Links as their name, opening the page: "[Scorett Outlet](https://…)". */
function linked(text: string, at: number): ReactNode {
  const parts = text.split(new RegExp(LINK, "g"));
  if (parts.length === 1) return swatched(text, at);
  // The split leaves the words, then each link's name and address.
  return parts.map((part, i) =>
    i % 3 === 0 ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: an answer's words don't move
      <Fragment key={i}>{swatched(part, at)}</Fragment>
    ) : i % 3 === 1 ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: an answer's words don't move
      <a key={i} href={parts[i + 1]} target="_blank" rel="noreferrer" className="ui-link">
        {part}
      </a>
    ) : null,
  );
}

/** Colour codes with their colour before them: "#a78bfa". */
function swatched(text: string, at: number): ReactNode {
  const parts = text.split(/(#[0-9a-f]{6}\b)/i);
  if (parts.length === 1) return text;
  return parts.map((part, i) =>
    i % 2 === 0 ? (
      part
    ) : (
      // biome-ignore lint/suspicious/noArrayIndexKey: a point's words don't move
      <span key={`${at}-${i}`} className="whitespace-nowrap">
        <span
          aria-hidden="true"
          className="mr-1 inline-block size-2.5 rounded-sm ring-1 ring-line"
          style={{ background: part }}
        />
        {part}
      </span>
    ),
  );
}

/** A point's words without the marks `inline` reads: what a search shows. */
export function plain(text: string): string {
  return text
    .replace(new RegExp(LINK, "g"), "$1")
    .replace(new RegExp(BOLD, "g"), (bold) => bold.slice(2, -2))
    .replace(new RegExp(ITALIC, "g"), (italic) => italic.slice(1, -1));
}

/**
 * A passage's first sentence, without its marks: what a section's tile
 * shows. A bold lead that is a sentence of its own ("**Wer kauft die
 * Ware?**") takes the one after it too.
 */
export function lead(text: string): string {
  const [first, next] = sentences(plain(text.trim().split("\n")[0] ?? ""));
  const bold = text.match(/^\*\*([^*]+)\*\*/)?.[1]?.trim();
  return first === bold && next ? `${first} ${next}` : (first ?? "");
}

/**
 * A line's sentences. One ends at a stop before a capital, but not after a
 * number ("Am 11. September", "(4. Stock)") or a short word ("z. B. Kuba"),
 * which German writes with a stop.
 */
function sentences(line: string): string[] {
  const found: string[] = [];
  let start = 0;
  for (const stop of line.matchAll(/[.!?…]["“”»)]*\s+(?=[\p{Lu}„"«(\d])/gu)) {
    const word = line.slice(0, stop.index + 1).match(/\S+$/)?.[0] ?? "";
    if (/^[(„"«]*(?:\d{1,2}|\p{L}{1,3})\.$/u.test(word)) continue;
    const end = stop.index + stop[0].length;
    found.push(line.slice(start, end).trim());
    start = end;
  }
  if (start < line.length) found.push(line.slice(start).trim());
  return found;
}
