// A table: a row for each thing (a theme), a column for each of the
// block's facets, and in each cell the row's point with that facet, light
// like a sheet. A header sorts by its column, a cell edits in place, and
// New row starts a thing.

import type { Block, Facet, Point } from "@almanac/types";
import { ArrowDown, ArrowUp, Plus } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { facetIcon } from "./icons";
import { useIsGuest, useWorkspace } from "./workspace";

interface Row {
  name: string;
  /** Its first point under each facet. */
  cells: Map<string, Point>;
}

/** A cell, by its row's name and its column's. */
interface At {
  row: string;
  column: string;
}

/** The rows of `points` in page order, a thing each. */
function rowsOf(points: Point[]): Row[] {
  const rows: Row[] = [];
  for (const point of points) {
    let row = rows.find((r) => r.name === point.theme);
    if (!row) {
      row = { name: point.theme, cells: new Map() };
      rows.push(row);
    }
    if (point.facet && !row.cells.has(point.facet)) row.cells.set(point.facet, point);
  }
  return rows;
}

/**
 * The number a cell says, when it says only a number and perhaps its unit:
 * 709 for "709 kr", 1525 for "1.525 kr", 3.4 for "3,4 kg". Null for words
 * ("Ja", "43, 44", "nur 43").
 */
export function numberIn(text: string): number | null {
  const found =
    /^[€$£]?\s*([-−]?)(\d{1,3}(?:[.,   ]\d{3})+(?:[.,]\d+)?|\d+(?:[.,]\d+)?)\s*(?:[\p{L}%€$£]{1,5}\.?)?$/u.exec(
      text.trim(),
    );
  if (!found) return null;
  const digits = (found[2] ?? "").replace(/[   ]/g, "");
  const separators = digits.match(/[.,]/g) ?? [];
  const last = Math.max(digits.lastIndexOf("."), digits.lastIndexOf(","));
  // A single separator before three digits groups thousands (1.525);
  // otherwise the last one starts the decimals (3,4 and 1.234,56).
  const decimal =
    new Set(separators).size === 2 || (separators.length === 1 && digits.length - last - 1 !== 3);
  const plain = decimal
    ? `${digits.slice(0, last).replace(/[.,]/g, "")}.${digits.slice(last + 1)}`
    : digits.replace(/[.,]/g, "");
  const value = Number(plain);
  return Number.isFinite(value) ? (found[1] ? -value : value) : null;
}

/** Orders two cells of a column: numbers by size, words as they're read. */
function compare(a: Point, b: Point, numeric: boolean): number {
  return numeric
    ? (a.amount ?? 0) - (b.amount ?? 0)
    : a.text.localeCompare(b.text, undefined, { numeric: true, sensitivity: "base" });
}

const HINT = "Enter saves · Esc goes back · Tab, the next cell";

export function Table({
  block,
  points,
  rowName,
}: {
  block: Block;
  points: Point[];
  /** A row's name, which opens its page. */
  rowName: (name: string) => ReactNode;
}) {
  const guest = useIsGuest();
  const { sync } = useWorkspace();
  const columns = block.facets;
  const [sort, setSort] = useState<{ by: string; down: boolean } | null>(null);
  const [editing, setEditing] = useState<At | null>(null);
  const [naming, setNaming] = useState(false);
  // A row named in New row, until one of its cells is kept.
  const [draft, setDraft] = useState<string | null>(null);
  const [at, setAt] = useState<At | null>(null);
  const focusing = useRef<At | "new" | null>(null);
  const table = useRef<HTMLTableElement>(null);

  const all = useMemo(() => rowsOf(points), [points]);
  const numeric = useMemo(
    () =>
      new Set(
        columns
          .filter((c) => {
            const cells = all.flatMap((r) => r.cells.get(c.name) ?? []);
            return cells.length > 0 && cells.every((p) => p.amount !== null);
          })
          .map((c) => c.name),
      ),
    [all, columns],
  );
  const rows = useMemo(() => {
    const sorted = [...all];
    if (sort) {
      const turn = sort.down ? -1 : 1;
      sorted.sort((a, b) => {
        if (!sort.by) return turn * a.name.localeCompare(b.name, undefined, { numeric: true });
        const [x, y] = [a.cells.get(sort.by), b.cells.get(sort.by)];
        // What nobody's said yet goes last, either way round.
        if (!x || !y) return x ? -1 : y ? 1 : 0;
        return turn * compare(x, y, numeric.has(sort.by));
      });
    }
    if (draft && !all.some((r) => r.name === draft)) sorted.push({ name: draft, cells: new Map() });
    return sorted;
  }, [all, sort, numeric, draft]);
  // The draft is a row of its own once a cell of it is kept.
  useEffect(() => {
    if (draft && all.some((r) => r.name === draft)) setDraft(null);
  }, [all, draft]);
  // Focus follows a cell that's done with, wherever the sort puts it.
  useEffect(() => {
    const target = focusing.current;
    if (!target || !table.current) return;
    focusing.current = null;
    const selector =
      target === "new"
        ? "[data-new]"
        : `[data-row="${CSS.escape(target.row)}"][data-column="${CSS.escape(target.column)}"]`;
    table.current.querySelector<HTMLElement>(selector)?.focus();
  });

  const save = (row: string, column: Facet, text: string) => {
    const point = all.find((r) => r.name === row)?.cells.get(column.name);
    const said = text.trim();
    const amount = numberIn(said);
    if (point) {
      if (!said) sync.delete("Point", point.id);
      else if (said !== point.text || amount !== point.amount) {
        // Null takes the number away, which the patch's generated type can't say.
        sync.update("Point", point.id, { text: said, amount: amount as number });
      }
    } else if (said) {
      sync.create("Point", {
        topic_id: block.topic_id,
        block_id: block.id,
        theme: row,
        facet: column.name,
        text: said,
        amount: amount ?? undefined,
        source: "You said",
        seen: true,
      });
    }
  };

  const done = (cell: At, next?: 1 | -1) => {
    const n = columns.findIndex((c) => c.name === cell.column) + (next ?? 0);
    const column = next ? columns[n] : undefined;
    if (column) {
      setEditing({ row: cell.row, column: column.name });
      return;
    }
    setEditing(null);
    focusing.current = cell;
  };

  // The arrows move between cells; the name opens its row, a cell edits.
  const cells: At[][] = rows.map((r) => [
    { row: r.name, column: "" },
    ...columns.map((c) => ({ row: r.name, column: c.name })),
  ]);
  const move = (event: KeyboardEvent, from: At) => {
    const r = cells.findIndex((row) => row[0]?.row === from.row);
    const c = cells[r]?.findIndex((cell) => cell.column === from.column) ?? -1;
    const [dr, dc] =
      event.key === "ArrowUp"
        ? [-1, 0]
        : event.key === "ArrowDown"
          ? [1, 0]
          : event.key === "ArrowLeft"
            ? [0, -1]
            : event.key === "ArrowRight"
              ? [0, 1]
              : [0, 0];
    if (dr === 0 && dc === 0) return false;
    event.preventDefault();
    if (r + dr === cells.length && !guest && columns.length > 0) {
      focusing.current = "new";
      setAt(null);
      return true;
    }
    const to = cells[r + dr]?.[c + dc];
    if (to) {
      focusing.current = to;
      setAt(to);
    }
    return true;
  };
  const first = cells[0]?.[0];
  const tabbable = (cell: At) =>
    at ? at.row === cell.row && at.column === cell.column : cell === first;

  const header = (by: string, label: ReactNode, facet?: Facet) => {
    const sorted = sort?.by === by;
    const Icon = facet ? facetIcon(facet.icon) : null;
    const right = numeric.has(by);
    return (
      <th
        key={by || "name"}
        scope="col"
        aria-sort={sorted ? (sort?.down ? "descending" : "ascending") : undefined}
        className="h-8 border-line border-r border-b bg-surface p-0 font-semibold text-xs last:border-r-0"
      >
        <button
          type="button"
          title={`Sort by ${facet?.name ?? "name"}`}
          onClick={() => setSort(sorted ? { by, down: !sort?.down } : { by, down: false })}
          className={`flex h-8 w-full items-center gap-1.5 px-3 outline-none hover:text-fg focus-visible:shadow-[inset_0_0_0_2px_var(--accent)] ${right ? "justify-end" : ""} ${sorted ? "text-fg" : "text-muted"}`}
        >
          {Icon && <Icon size={14} strokeWidth={1.75} aria-hidden="true" className="text-faint" />}
          {label}
          {sorted &&
            (sort?.down ? (
              <ArrowDown size={12} strokeWidth={2} aria-hidden="true" />
            ) : (
              <ArrowUp size={12} strokeWidth={2} aria-hidden="true" />
            ))}
        </button>
      </th>
    );
  };

  const cell = "h-[34px] border-line border-r border-b px-3 last:border-r-0";
  const focus = "outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--accent)]";
  return (
    <div className="flex flex-col gap-2">
      <div className="overflow-x-auto rounded-md ring-1 ring-line ring-inset">
        <table
          ref={table}
          aria-label={block.title}
          className="w-full border-collapse whitespace-nowrap tabular-nums [&_tr:last-child>td]:border-b-0"
        >
          <thead>
            <tr>
              {header("", "Name")}
              {columns.map((c) => header(c.name, c.name, c))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const name: At = { row: row.name, column: "" };
              const real = all.includes(row);
              return (
                <tr key={row.name} className="group/row [&>td]:hover:bg-hover">
                  <td
                    data-row={row.name}
                    data-column=""
                    tabIndex={tabbable(name) ? 0 : -1}
                    onFocus={() => setAt(name)}
                    onKeyDown={(event) => {
                      if (move(event, name)) return;
                      if (event.key === "Enter" && event.target === event.currentTarget) {
                        event.preventDefault();
                        event.currentTarget.querySelector("a")?.click();
                      }
                    }}
                    className={`${cell} ${focus} max-w-72 truncate font-semibold [&_svg]:invisible group-hover/row:[&_svg]:visible focus-visible:[&_svg]:visible`}
                  >
                    {real ? <Untabbable>{rowName(row.name)}</Untabbable> : row.name}
                  </td>
                  {columns.map((column) => {
                    const here: At = { row: row.name, column: column.name };
                    const point = row.cells.get(column.name);
                    const right = numeric.has(column.name);
                    if (editing?.row === row.name && editing.column === column.name) {
                      return (
                        <td
                          key={column.name}
                          className={`${cell} bg-field p-0 shadow-[inset_0_0_0_2px_var(--accent)]`}
                        >
                          <CellInput
                            label={`${column.name} of ${row.name}`}
                            value={point?.text ?? ""}
                            right={right}
                            onDone={(text, how) => {
                              if (how === "cancel") {
                                if (!real && draft === row.name) {
                                  setDraft(null);
                                  setEditing(null);
                                  focusing.current = "new";
                                  return;
                                }
                                done(here);
                                return;
                              }
                              save(row.name, column, text);
                              done(here, how === "next" ? 1 : how === "back" ? -1 : undefined);
                            }}
                          />
                        </td>
                      );
                    }
                    const look =
                      point?.look === "warn"
                        ? "font-semibold text-warn-ink"
                        : point?.look === "info"
                          ? "text-info-ink"
                          : "";
                    return (
                      <td
                        key={column.name}
                        data-row={row.name}
                        data-column={column.name}
                        tabIndex={tabbable(here) ? 0 : -1}
                        title={point && point.text.length > 32 ? point.text : undefined}
                        onFocus={() => setAt(here)}
                        onClick={() => !guest && setEditing(here)}
                        onKeyDown={(event) => {
                          if (move(event, here)) return;
                          if (!guest && (event.key === "Enter" || event.key === "F2")) {
                            event.preventDefault();
                            setEditing(here);
                          }
                        }}
                        className={`${cell} ${focus} max-w-64 truncate ${right ? "text-right" : ""} ${look} ${guest ? "" : "cursor-text"}`}
                      >
                        {point ? point.text : <span className="text-faint">–</span>}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
            {!guest && columns.length > 0 && (
              <tr>
                <td
                  className={`${cell} p-0 ${naming ? "bg-field shadow-[inset_0_0_0_2px_var(--accent)]" : ""}`}
                >
                  {naming ? (
                    <CellInput
                      label="The new row's name"
                      value=""
                      onDone={(text, how) => {
                        setNaming(false);
                        const named = text.trim();
                        if (how === "cancel" || !named) {
                          focusing.current = "new";
                          return;
                        }
                        const column = columns[0];
                        if (!all.some((r) => r.name === named)) setDraft(named);
                        if (column && how !== "blur")
                          setEditing({ row: named, column: column.name });
                      }}
                    />
                  ) : (
                    <button
                      type="button"
                      data-new
                      onClick={() => setNaming(true)}
                      onKeyDown={(event) => {
                        if (event.key === "ArrowUp" && cells.length > 0) {
                          event.preventDefault();
                          const to = cells[cells.length - 1]?.[0];
                          if (to) {
                            focusing.current = to;
                            setAt(to);
                          }
                        }
                      }}
                      className={`flex h-[34px] w-full items-center gap-1.5 px-3 text-muted hover:text-fg ${focus}`}
                    >
                      <Plus size={14} strokeWidth={1.75} aria-hidden="true" />
                      New row
                    </button>
                  )}
                </td>
                {columns.map((c) => (
                  <td key={c.name} className={cell} />
                ))}
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {(editing || naming) && <p className="text-xs text-faint">{HINT}</p>}
    </div>
  );
}

/**
 * A row's name link, which Tab passes by: its cell takes the focus, and
 * Enter there opens it, so Tab crosses the table once.
 */
function Untabbable({ children }: { children: ReactNode }) {
  const span = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const link = span.current?.querySelector("a");
    if (link) link.tabIndex = -1;
  });
  return (
    <span ref={span} className="contents">
      {children}
    </span>
  );
}

/**
 * A cell being changed: Enter or leaving keeps it, Esc goes back, Tab and
 * Shift-Tab keep it and go on to the next cell or the one before.
 */
function CellInput({
  label,
  value,
  right = false,
  onDone,
}: {
  label: string;
  value: string;
  right?: boolean;
  onDone: (text: string, how: "enter" | "next" | "back" | "cancel" | "blur") => void;
}) {
  const [text, setText] = useState(value);
  const finished = useRef(false);
  const finish = (how: "enter" | "next" | "back" | "cancel" | "blur") => {
    if (finished.current) return;
    finished.current = true;
    onDone(text, how);
  };
  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: a cell is changed where it's clicked
      autoFocus
      aria-label={label}
      value={text}
      onChange={(event) => setText(event.target.value)}
      onFocus={(event) => event.currentTarget.select()}
      onBlur={() => finish("blur")}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          finish("enter");
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          finish("cancel");
        } else if (event.key === "Tab") {
          event.preventDefault();
          finish(event.shiftKey ? "back" : "next");
        }
      }}
      className={`h-[34px] w-full min-w-24 bg-transparent px-3 outline-none ${right ? "text-right" : ""}`}
    />
  );
}
