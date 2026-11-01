// Fields…, in a collection's ⋯: every field its rows have besides their
// name, changed by hand as a talk's change_block gives them whole. The
// fields pick the collection's picture (`pictureOf`), so this changes it
// too. Renaming a field takes its cells along; removing one takes them off.

import type { Block, Cell, Field, Point } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import { type Confirmation, ConfirmDialog, CreateDialog } from "@code-villain/web";
import { GripVertical, Plus, X } from "lucide-react";
import { useState } from "react";
import { Button } from "./ui";
import { useWorkspace } from "./workspace";

/** The kinds of field, by the names a person picks them by (`FIELD_KINDS`). */
const KINDS: [kind: string, name: string][] = [
  ["text", "Text"],
  ["number", "Number"],
  ["day", "Day"],
  ["choice", "Choice"],
  ["status", "Status"],
  ["check", "Check"],
  ["photo", "Photo"],
  ["link", "Link"],
];

/** Fields whose words a row picks among. */
const PICKED = new Set(["choice", "status"]);

/** At most this many fields besides the name, as the server keeps them (`MOST_FIELDS`). */
const MOST_FIELDS = 8;

/** A field as it's changed here: `was` its name when the dialog opened, none for a new one. */
interface Draft extends Field {
  key: string;
  was: string | null;
}

/** How many of `rows` fill the field named `name`. */
function filledIn(rows: Point[], name: string): number {
  return rows.filter((row) => {
    const cell = row.cells[name];
    return cell !== undefined && cell !== null && cell !== "";
  }).length;
}

/** What's wrong with the fields, in a sentence, or nothing. */
function problemOf(fields: Draft[]): string | null {
  const names = fields.map((f) => f.name.trim());
  if (names.some((n) => !n)) return "Every field has a name.";
  if (names.some((n) => n.length > 40)) return "A field's name is at most 40 characters.";
  const twice = names.find((n, i) => names.indexOf(n) !== i);
  if (twice) return `Two fields are named ${twice}.`;
  return null;
}

/**
 * The dialog: a line a field, its name and its kind, a number's unit and a
 * choice's or a status's words under it; ✕ removes it, the grip drags it,
 * and Add a field adds one, eight at most. Save writes them all at once.
 */
export function FieldsDialog({
  block,
  rows,
  onClose,
}: {
  block: Block;
  rows: Point[];
  onClose: () => void;
}) {
  const { sync } = useWorkspace();
  const [fields, setFields] = useState<Draft[]>(() =>
    block.fields.map((f) => ({ ...f, key: f.name, was: f.name })),
  );
  const [dragged, setDragged] = useState<number | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const problem = problemOf(fields);

  const change = (at: number, patch: Partial<Field>) =>
    setFields(fields.map((f, n) => (n === at ? { ...f, ...patch } : f)));
  const move = (from: number, to: number) => {
    if (to < 0 || to >= fields.length || from === to) return;
    const order = [...fields];
    const [field] = order.splice(from, 1);
    if (!field) return;
    order.splice(to, 0, field);
    setFields(order);
  };
  const remove = (at: number) => {
    const field = fields[at];
    if (!field) return;
    const filled = field.was ? filledIn(rows, field.was) : 0;
    const gone = () => setFields((now) => now.filter((f) => f.key !== field.key));
    if (!filled) return gone();
    const name = field.was ?? field.name;
    setConfirmation({
      title: `Remove ${name}?`,
      body: `${name} is filled in ${filled} ${filled === 1 ? "row" : "rows"}. This can't be undone.`,
      action: "Remove field",
      onConfirm: gone,
    });
  };
  const add = () =>
    setFields([
      ...fields,
      {
        key: crypto.randomUUID(),
        was: null,
        name: "",
        kind: "text",
        unit: "",
        goal: null,
        options: [],
      },
    ]);

  const save = () => {
    if (problem) return;
    const kept: Field[] = fields.map((f) => ({
      name: f.name.trim(),
      kind: f.kind,
      unit: f.kind === "number" ? f.unit.trim() : "",
      goal: f.kind === "number" ? f.goal : null,
      options: PICKED.has(f.kind) ? f.options : [],
    }));
    // Each row's cells by the fields' new names, without the removed ones'.
    const named = new Map(fields.flatMap((f) => (f.was ? [[f.was, f.name.trim()]] : [])));
    const cells = rows.flatMap((row) => {
      const now: Record<string, Cell> = {};
      for (const [name, cell] of Object.entries(row.cells)) {
        const to = named.get(name);
        if (to) now[to] = cell;
        else if (!block.fields.some((f) => f.name === name)) now[name] = cell;
      }
      const same =
        Object.keys(now).length === Object.keys(row.cells).length &&
        Object.entries(now).every(([name, cell]) => row.cells[name] === cell);
      return same
        ? []
        : [
            {
              action: "update" as const,
              model: "Point" as const,
              id: row.id,
              data: { cells: now },
            },
          ];
    });
    sync.transact([
      { action: "update", model: "Block", id: block.id, data: { fields: kept } },
      ...cells,
    ]);
    onClose();
  };

  const box = "ui-field h-8 min-w-0 rounded-md px-2 text-sm";
  const line = "grid grid-cols-[1.25rem_minmax(0,1fr)_7.5rem_1.75rem] items-center gap-2";
  return (
    <CreateDialog open onClose={onClose} width="30rem">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        <div className="flex flex-col gap-0.5 px-4 pt-4">
          <Dialog.Title className="font-semibold">
            Fields of {block.title || "the collection"}
          </Dialog.Title>
          <p className="text-sm text-muted">Every row has its name, and these.</p>
        </div>
        <div className="flex max-h-[60vh] flex-col gap-2 overflow-y-auto px-4 py-3">
          <div className={line}>
            <span />
            <span className={`${box} flex items-center text-muted`}>Name</span>
            <span className={`${box} flex items-center text-faint`}>Each row's name</span>
            <span />
          </div>
          {fields.map((field, at) => (
            // biome-ignore lint/a11y/noStaticElementInteractions: a drop target; the grip moves it with keys too.
            <div
              key={field.key}
              onDragOver={(event) => {
                if (dragged === null) return;
                event.preventDefault();
                if (dragged !== at) {
                  move(dragged, at);
                  setDragged(at);
                }
              }}
              onDrop={(event) => event.preventDefault()}
              className={`flex flex-col gap-1.5 rounded-md ${dragged === at ? "opacity-60" : ""}`}
            >
              <div className={line}>
                <button
                  type="button"
                  draggable
                  aria-label={`Move ${field.name || "this field"}: drag it, or use the arrow keys`}
                  title="Drag to move"
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = "move";
                    const row = event.currentTarget.parentElement;
                    if (row) event.dataTransfer.setDragImage(row, 12, 16);
                    setDragged(at);
                  }}
                  onDragEnd={() => setDragged(null)}
                  onKeyDown={(event) => {
                    const by = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
                    if (!by) return;
                    event.preventDefault();
                    move(at, at + by);
                  }}
                  className="flex h-8 cursor-grab items-center justify-center rounded text-faint hover:text-muted active:cursor-grabbing"
                >
                  <GripVertical size={14} strokeWidth={1.75} aria-hidden="true" />
                </button>
                <input
                  // biome-ignore lint/a11y/noAutofocus: a new field opens to be named
                  autoFocus={field.was === null}
                  aria-label="Field name"
                  placeholder="Field name"
                  value={field.name}
                  maxLength={40}
                  onChange={(event) => change(at, { name: event.target.value })}
                  className={box}
                />
                <select
                  aria-label={`Kind of ${field.name || "field"}`}
                  value={field.kind}
                  onChange={(event) => change(at, { kind: event.target.value })}
                  className={box}
                >
                  {KINDS.map(([kind, name]) => (
                    <option key={kind} value={kind}>
                      {name}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  aria-label={`Remove ${field.name || "this field"}`}
                  title="Remove"
                  onClick={() => remove(at)}
                  className="ui-icon-button size-7 justify-center p-0"
                >
                  <X size={14} strokeWidth={1.75} aria-hidden="true" />
                </button>
              </div>
              {field.kind === "number" && (
                <div className={line}>
                  <span />
                  <input
                    aria-label={`Unit of ${field.name || "the number"}`}
                    placeholder="Unit, like kg or €"
                    value={field.unit}
                    maxLength={12}
                    onChange={(event) => change(at, { unit: event.target.value })}
                    className={`${box} w-40`}
                  />
                </div>
              )}
              {PICKED.has(field.kind) && (
                <div className={line}>
                  <span />
                  <Words field={field} onChange={(options) => change(at, { options })} />
                </div>
              )}
            </div>
          ))}
          {fields.length < MOST_FIELDS ? (
            <button
              type="button"
              onClick={add}
              className="ui-button -ml-2 h-7 gap-1.5 self-start px-2 text-xs text-muted"
            >
              <Plus size={14} strokeWidth={1.75} aria-hidden="true" />
              Add a field
            </button>
          ) : (
            <p className="text-xs text-faint">A collection has at most {MOST_FIELDS} fields.</p>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 px-4 pb-4">
          {problem && <p className="mr-auto text-xs text-warn-ink">{problem}</p>}
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={problem !== null}>
            Save
          </Button>
        </div>
      </form>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </CreateDialog>
  );
}

/**
 * A choice's or a status's words, in order: ✕ takes one off, and a word
 * written after them and Enter adds it, twelve at most.
 */
function Words({ field, onChange }: { field: Field; onChange: (options: string[]) => void }) {
  const [word, setWord] = useState("");
  const keep = () => {
    const w = word.trim();
    if (!w || field.options.includes(w)) return;
    onChange([...field.options, w]);
    setWord("");
  };
  return (
    <div className="col-span-3 flex flex-wrap items-center gap-1.5">
      {field.options.map((option) => (
        <span key={option} className="ui-chip gap-1 pr-1">
          {option}
          <button
            type="button"
            aria-label={`Take ${option} off`}
            onClick={() => onChange(field.options.filter((o) => o !== option))}
            className="flex text-faint hover:text-fg"
          >
            <X size={12} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </span>
      ))}
      {field.options.length < 12 && (
        <input
          aria-label={`A word for ${field.name || "the field"}`}
          placeholder={field.kind === "status" ? "+ Stage" : "+ Word"}
          value={word}
          maxLength={40}
          onChange={(event) => setWord(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            keep();
          }}
          onBlur={keep}
          className="h-7 w-28 min-w-0 rounded-md bg-transparent px-1.5 text-sm outline-none placeholder:text-faint focus:bg-field"
        />
      )}
    </div>
  );
}
