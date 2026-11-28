import type { CodeLine } from "../lib/types";

const OP_CLASS = { "+": "add", "-": "del", " ": "" } as const;

/** The lines a thread is on, above its comments, as GitHub shows them. */
export function PrCodeExcerpt({ lines }: { lines: CodeLine[] }) {
  return (
    <div className="fb-code">
      <div className="diff-lines">
        {lines.map((l, i) => (
          // oxlint-disable-next-line react/no-array-index-key -- diff lines in order, including lines without numbers
          <div key={i} className={`diff-line ${OP_CLASS[l.op] ?? ""}`}>
            <span className="ln">{l.n ?? ""}</span>
            <span className="tx">
              {l.op}
              {l.text}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
