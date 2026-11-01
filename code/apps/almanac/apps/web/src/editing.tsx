// Changing a collection by hand: a click on a row's value changes it in
// place, and "+ New" under a collection adds a row, its fields in one line
// to tab through. Both write the row's cells through the sync engine, as a
// talk does, so they work offline. Guests, and pages only to look at, get
// neither.

import type { Block, Cell, Field, Point, Reminder } from "@almanac/types";
import { Plus } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useMemo, useRef, useState } from "react";
import { cellOf } from "./collection";
import { doneFor, isDue, markDone, markNotDone } from "./reminders";
import { Button } from "./ui";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";
import { useAdding } from "./writing";

/** The kinds a person picks from the field's own options. */
const PICKED = new Set(["status", "choice"]);

const DAY = /^\d{4}-\d{2}-\d{2}$/;

/**
 * What a person typed, as the field keeps it and as the server reads a
 * talk's: a number from "9,95" or "3,4 kg" in a field of kilograms,
 * nothing for "", and null for what isn't a number in a number's field.
 */
function cellFrom(field: Field, typed: string | boolean): Cell | undefined | null {
  if (typeof typed === "boolean") return typed || undefined;
  const words = typed.trim();
  if (!words) return undefined;
  if (field.kind === "number") {
    const lower = words.toLowerCase();
    const unit = field.unit.toLowerCase();
    const bare = unit && lower.endsWith(unit) ? lower.slice(0, -unit.length) : lower;
    const plain = bare.replace(/\s/g, "").replaceAll(",", ".");
    const n = plain ? Number(plain) : Number.NaN;
    return Number.isFinite(n) ? n : null;
  }
  return words;
}

/** Writes a row's cell, or takes it out. */
export function useSetCell() {
  const { sync } = useWorkspace();
  return (row: Point, field: Field, cell: Cell | undefined) => {
    const cells = { ...row.cells };
    if (cell === undefined) delete cells[field.name];
    else cells[field.name] = cell;
    sync.update("Point", row.id, { cells });
  };
}

/** The reminders that ring for checklist items, by item. */
export function useItemReminders(): Map<string, Reminder> {
  const reminders = useRecords("Reminder");
  return useMemo(
    () =>
      new Map(
        reminders
          .filter((r) => r.point_id && (r.status === "on" || r.status === "done"))
          .map((r) => [r.point_id as string, r]),
      ),
    [reminders],
  );
}

/**
 * Whether a row to tick is done: its `check`, or, with a reminder, done
 * for the last day it was due, until it comes due again.
 */
export function itemDone(row: Point, check: Field, reminder: Reminder | undefined): boolean {
  if (!reminder) return cellOf(row, check) === true;
  return doneFor(reminder) !== undefined && !isDue(reminder);
}

/**
 * Ticks a row by its `check`, or unticks it when it's done. A row a
 * reminder rings for ticks the reminder instead, as everywhere a row ticks.
 */
export function useTick() {
  const { sync } = useWorkspace();
  const rings = useItemReminders();
  return (row: Point, check: Field) => {
    const reminder = rings.get(row.id);
    const done = itemDone(row, check, reminder);
    if (reminder) return done ? markNotDone(sync, reminder) : markDone(sync, reminder);
    sync.update("Point", row.id, { cells: { ...row.cells, [check.name]: !done } });
  };
}

/**
 * A row's value that a click changes: a check ticks, a status or a choice
 * offers its options, the rest opens a field to type in, Enter or leaving
 * it keeps what's typed and Escape doesn't.
 */
export function EditCell({
  row,
  field,
  children,
  className = "",
}: {
  row: Point;
  field: Field;
  children: ReactNode;
  className?: string;
}) {
  const guest = useIsGuest();
  const set = useSetCell();
  const tick = useTick();
  const [editing, setEditing] = useState(false);
  if (guest || field.kind === "photo") return <>{children}</>;
  const cell = cellOf(row, field);
  const look = `-mx-1 min-h-6 w-[calc(100%+0.5rem)] cursor-text rounded px-1 hover:bg-hover ${className}`;
  if (field.kind === "check") {
    return (
      <button
        type="button"
        title={`${cell === true ? "Untick" : "Tick"} ${field.name}`}
        onClick={() => tick(row, field)}
        className={`${look} cursor-pointer`}
        style={{ textAlign: "inherit" }}
      >
        {children}
      </button>
    );
  }
  if (!editing) {
    return (
      <button
        type="button"
        title={`Change ${field.name}`}
        onClick={() => setEditing(true)}
        className={look}
        style={{ textAlign: "inherit" }}
      >
        {children}
      </button>
    );
  }
  const done = (typed?: string) => {
    setEditing(false);
    if (typed === undefined) return;
    const next = cellFrom(field, typed);
    if (next !== null && next !== cell) set(row, field, next);
  };
  if (PICKED.has(field.kind)) {
    return <Options field={field} value={cell} onDone={done} />;
  }
  return <TypeIn field={field} value={cell} onDone={done} />;
}

/** A status's or a choice's options, a chip each, and – to leave it empty. */
function Options({
  field,
  value,
  onDone,
}: {
  field: Field;
  value: Cell | undefined;
  onDone: (typed?: string) => void;
}) {
  const chip = "rounded px-1.5 py-0.5 text-xs hover:bg-hover focus-visible:bg-hover";
  return (
    <fieldset
      aria-label={field.name}
      className="m-0 flex min-w-0 flex-wrap gap-1 border-0 p-0"
      onKeyDown={(e) => e.key === "Escape" && onDone()}
      // WebKit doesn't focus a clicked button: pressing one would blur the
      // options away before its click. Focus stays put while one is pressed.
      onMouseDown={(e) => e.preventDefault()}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onDone();
      }}
    >
      {field.options.map((option, i) => (
        <button
          key={option}
          type="button"
          // biome-ignore lint/a11y/noAutofocus: the options open where the value was clicked
          autoFocus={option === value || (i === 0 && !field.options.includes(String(value)))}
          onClick={() => onDone(option)}
          className={`${chip} ${option === value ? "bg-chip font-semibold" : "text-muted"}`}
        >
          {option}
        </button>
      ))}
      <button
        type="button"
        title="Leave it empty"
        onClick={() => onDone("")}
        className={`${chip} text-faint`}
      >
        –
      </button>
    </fieldset>
  );
}

/** A field to type a value in: a date picker for a day, a number's keyboard for a number. */
function TypeIn({
  field,
  value,
  onDone,
}: {
  field: Field;
  value: Cell | undefined;
  onDone: (typed?: string) => void;
}) {
  const shown = value === undefined ? "" : String(value);
  const date = field.kind === "day" && (shown === "" || DAY.test(shown));
  const [typed, setTyped] = useState(shown);
  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      onDone(typed);
    }
    if (e.key === "Escape") onDone();
  };
  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: the field opens where the value was clicked
      autoFocus
      aria-label={field.name}
      type={date ? "date" : "text"}
      inputMode={field.kind === "number" ? "decimal" : undefined}
      value={typed}
      onChange={(e) => setTyped(e.target.value)}
      onFocus={(e) => e.currentTarget.select()}
      onKeyDown={keys}
      onBlur={() => onDone(typed)}
      className="ui-field -mx-1 h-7 w-[calc(100%+0.5rem)] min-w-24 rounded px-1.5"
      style={{ textAlign: "inherit" }}
    />
  );
}

/**
 * "+ New" under a collection: a row's name, then its fields in one line,
 * Tab between them, Enter adds it and starts the next. A collection just
 * added opens on it, for its next row.
 */
export function NewRow({ block, rows }: { block: Block; rows: Point[] }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const adding = useAdding();
  const [open, setOpen] = useState(adding?.fresh === block.id);
  const [name, setName] = useState("");
  const [typed, setTyped] = useState<Record<string, string | boolean>>({});
  const first = useRef<HTMLInputElement>(null);
  if (guest) return null;
  const fields = block.fields.filter((f) => f.kind !== "photo");
  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex items-center gap-1.5 self-start rounded-md px-2.5 py-1 text-faint text-sm hover:bg-hover hover:text-fg"
      >
        <Plus size={14} strokeWidth={1.75} aria-hidden="true" />
        New
      </button>
    );
  }
  const close = () => {
    setOpen(false);
    setName("");
    setTyped({});
    if (adding?.fresh === block.id) adding.settle();
  };
  const add = () => {
    if (!name.trim()) return;
    const cells: Record<string, Cell> = {};
    for (const field of fields) {
      const value = typed[field.name];
      const cell = value === undefined ? undefined : cellFrom(field, value);
      if (cell !== undefined && cell !== null) cells[field.name] = cell;
    }
    sync.create("Point", {
      topic_id: block.topic_id,
      block_id: block.id,
      theme: rows[0]?.theme || undefined,
      text: name.trim(),
      cells,
      seen: true,
      position: Math.max(-1, ...rows.map((r) => r.position)) + 1,
    });
    setName("");
    setTyped({});
    first.current?.focus();
  };
  const put = (field: Field, value: string | boolean) =>
    setTyped({ ...typed, [field.name]: value });
  const box = "ui-field h-8 rounded-md px-2 text-sm";
  return (
    <form
      aria-label={`New in ${block.title || "the collection"}`}
      onSubmit={(e) => {
        e.preventDefault();
        add();
      }}
      onKeyDown={(e) => e.key === "Escape" && close()}
      className="flex flex-wrap items-center gap-2 rounded-lg bg-surface p-2"
    >
      <input
        ref={first}
        // biome-ignore lint/a11y/noAutofocus: New opens to type the row's name
        autoFocus
        aria-label="Name"
        placeholder="Name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        className={`${box} min-w-40 flex-[2]`}
      />
      {fields.map((field) => {
        const value = typed[field.name];
        if (field.kind === "check") {
          return (
            <label key={field.name} className="flex items-center gap-1.5 px-1 text-sm text-muted">
              <input
                type="checkbox"
                checked={value === true}
                onChange={(e) => put(field, e.target.checked)}
              />
              {field.name}
            </label>
          );
        }
        if (PICKED.has(field.kind)) {
          return (
            <select
              key={field.name}
              aria-label={field.name}
              value={typeof value === "string" ? value : ""}
              onChange={(e) => put(field, e.target.value)}
              className={`${box} ${value ? "" : "text-faint"}`}
            >
              <option value="">{field.name}</option>
              {field.options.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          );
        }
        return (
          <input
            key={field.name}
            aria-label={field.name}
            placeholder={field.unit ? `${field.name} (${field.unit})` : field.name}
            type={field.kind === "day" ? "date" : "text"}
            inputMode={field.kind === "number" ? "decimal" : undefined}
            value={typeof value === "string" ? value : ""}
            onChange={(e) => put(field, e.target.value)}
            className={`${box} ${field.kind === "number" || field.kind === "day" ? "w-32" : "min-w-32 flex-1"}`}
          />
        );
      })}
      <span className="ml-auto flex gap-1.5">
        <Button variant="secondary" onClick={close}>
          Done
        </Button>
        <Button type="submit" disabled={!name.trim()}>
          Add
        </Button>
      </span>
    </form>
  );
}
