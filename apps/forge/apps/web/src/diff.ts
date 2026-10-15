/** One part of a diff, a line or a word: kept, removed or added. */
export type DiffPart = { kind: "same" | "removed" | "added"; text: string };

/**
 * One line of a diff. A changed line paired with the line it replaced also
 * says which of its words changed.
 */
export type DiffLine = DiffPart & { words?: DiffPart[] };

/**
 * What changes, line by line, from `before` to `after`: the fewest lines
 * removed and added (a longest common subsequence), removals first.
 */
export function lineDiff(before: string, after: string): DiffLine[] {
  return diff(before === "" ? [] : before.split("\n"), after === "" ? [] : after.split("\n"));
}

/** The fewest parts removed and added to turn `a` into `b`, removals first. */
function diff(a: string[], b: string[]): DiffPart[] {
  // common(i, j): lines in common between a[i..] and b[j..].
  const width = b.length + 1;
  const table = new Int32Array((a.length + 1) * width);
  const common = (i: number, j: number) => table[i * width + j] ?? 0;
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i * width + j] =
        a[i] === b[j] ? common(i + 1, j + 1) + 1 : Math.max(common(i + 1, j), common(i, j + 1));
    }
  }
  const lines: DiffPart[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      lines.push({ kind: "same", text: a[i++] ?? "" });
      j++;
    } else if (i < a.length && (j === b.length || common(i + 1, j) >= common(i, j + 1))) {
      lines.push({ kind: "removed", text: a[i++] ?? "" });
    } else {
      lines.push({ kind: "added", text: b[j++] ?? "" });
    }
  }
  return lines;
}

/**
 * The diff with long runs of unchanged lines folded away, keeping `context`
 * lines next to each change. A folded run becomes `{ skipped: n }`.
 */
export function foldDiff(lines: DiffLine[], context = 2): (DiffLine | { skipped: number })[] {
  const near = lines.map((_, index) =>
    lines
      .slice(Math.max(0, index - context), index + context + 1)
      .some((line) => line.kind !== "same"),
  );
  const folded: (DiffLine | { skipped: number })[] = [];
  lines.forEach((line, index) => {
    if (near[index]) {
      folded.push(line);
      return;
    }
    const last = folded[folded.length - 1];
    if (last && "skipped" in last) last.skipped++;
    else folded.push({ skipped: 1 });
  });
  return folded;
}

/**
 * The diff with each run of removed lines followed by as many added ones
 * paired up, line by line, so each pair says which words changed. Lines
 * with no word in common stay whole.
 */
export function wordPairs(lines: DiffLine[]): DiffLine[] {
  const paired = lines.map((line) => ({ ...line }));
  let index = 0;
  while (index < paired.length) {
    const removed = run(paired, index, "removed");
    const added = run(paired, index + removed, "added");
    if (removed > 0 && removed === added) {
      for (let k = 0; k < removed; k++) {
        const old = paired[index + k];
        const now = paired[index + removed + k];
        if (!old || !now) continue;
        const words = diff(tokens(old.text), tokens(now.text));
        if (!words.some((w) => w.kind === "same" && w.text.trim() !== "")) continue;
        old.words = words.filter((w) => w.kind !== "added");
        now.words = words.filter((w) => w.kind !== "removed");
      }
    }
    index += Math.max(1, removed + added);
  }
  return paired;
}

function run(lines: DiffLine[], from: number, kind: DiffLine["kind"]): number {
  let n = 0;
  while (lines[from + n]?.kind === kind) n++;
  return n;
}

/** Words and the spaces between them, so they join back into the line. */
function tokens(line: string): string[] {
  return line.split(/(\s+)/).filter((t) => t !== "");
}
