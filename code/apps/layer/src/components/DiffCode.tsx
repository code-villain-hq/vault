/** A diff line's code, coloured as the editor colours it, with its changed words marked (DIFF-9). */
import { Fragment, useEffect, useMemo, useState } from "react";
import { wordMarks, type DiffLine } from "../lib/diff";
import { colourDiff, type Token } from "../lib/highlight";

/**
 * The patch's colours and word marks. Colours arrive once the grammar has
 * loaded and the patch has been read; until then, and for a file Layer has
 * no grammar for, lines are drawn plain.
 */
export function useDiffColours(lines: DiffLine[], path: string | null) {
  const [coloured, setColoured] = useState<{ lines: DiffLine[]; runs: (Token[] | undefined)[] | null } | null>(null);
  useEffect(() => {
    if (!path) return;
    let live = true;
    colourDiff(lines, path)
      .then((runs) => { if (live) setColoured({ lines, runs }); })
      // A grammar that fails to load leaves the patch plain, as an unknown language does.
      .catch(() => { if (live) setColoured({ lines, runs: null }); });
    return () => { live = false; };
  }, [lines, path]);
  const marks = useMemo(() => wordMarks(lines), [lines]);
  const runs = coloured && coloured.lines === lines ? coloured.runs : null;
  return { runs, marks };
}

// Shiki's font style bits: italic 1, bold 2, underline 4, strikethrough 8.
function look({ color, fontStyle = 0 }: Token) {
  const lines = [fontStyle & 4 && "underline", fontStyle & 8 && "line-through"].filter(Boolean).join(" ");
  return {
    color,
    fontStyle: fontStyle & 1 ? "italic" : undefined,
    fontWeight: fontStyle & 2 ? 700 : undefined,
    textDecoration: lines || undefined,
  };
}

/** One line's code, without its +/- sign: its runs split where the mark starts and ends. */
export function DiffCode({ text, runs, mark }: { text: string; runs?: Token[]; mark?: [number, number] }) {
  const parts = runs ?? [{ content: text }];
  let at = 0;
  return (
    <>
      {parts.map((run, i) => {
        const start = at;
        const end = (at += run.content.length);
        const style = run.color ? look(run) : undefined;
        if (!mark || end <= mark[0] || start >= mark[1]) return <span key={i} style={style}>{run.content}</span>;
        const from = Math.max(start, mark[0]) - start;
        const to = Math.min(end, mark[1]) - start;
        return (
          <Fragment key={i}>
            {from > 0 && <span style={style}>{run.content.slice(0, from)}</span>}
            <mark style={style}>{run.content.slice(from, to)}</mark>
            {to < run.content.length && <span style={style}>{run.content.slice(to)}</span>}
          </Fragment>
        );
      })}
    </>
  );
}
