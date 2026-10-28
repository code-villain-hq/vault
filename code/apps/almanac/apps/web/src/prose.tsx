import type { ReactNode } from "react";

/**
 * An answer as written: paragraphs and short lists, with **bold** and *italic* kept.
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

/** A line as written, with **bold** and *italic* kept: an answer's, or a point's. */
export function inline(text: string): ReactNode {
  // The marked parts sit at the odd places the split leaves them, marks and
  // all: **bold** and *italic*.
  return text.split(/(\*\*[^*]+\*\*|\*[^*\s](?:[^*]*[^*\s])?\*)/).map((part, i) => {
    if (i % 2 === 0) return part;
    return part.startsWith("**") ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: an answer's words don't move
      <strong key={i} className="font-semibold">
        {part.slice(2, -2)}
      </strong>
    ) : (
      // biome-ignore lint/suspicious/noArrayIndexKey: an answer's words don't move
      <em key={i}>{part.slice(1, -1)}</em>
    );
  });
}
