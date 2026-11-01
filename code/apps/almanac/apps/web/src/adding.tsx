// Adding a block by hand: Add a block at the end of a page, Add below in a
// block's ⋯ and ⌘K start one of the set (`ADDS` in `writing.tsx`), which
// is written where it will read and kept with what it starts with, in one
// transaction: a block with nothing in it doesn't show. A picture picks
// its photos at once. Hand-made points have no source, as rows added with
// + New have none.

import type { Block, Cell, Field, Topic } from "@almanac/types";
import { Menu } from "@base-ui/react/menu";
import { uuidv7 } from "@code-villain/sync-client";
import { Info, Plus } from "lucide-react";
import { type RefObject, useEffect, useRef, useState } from "react";
import { DropBox, useAddPictures } from "./pictures";
import { Button } from "./ui";
import { useIsGuest, useWorkspace } from "./workspace";
import {
  ADDS,
  type Adding,
  type AddKind,
  type Draft,
  fieldsOf,
  offerAdding,
  useOpening,
  WriteText,
  WritingKeys,
} from "./writing";

/** A block as a draft keeps it. */
interface NewBlock {
  kind: string;
  title?: string;
  fields?: Field[];
  view?: string;
}

/** What a block starts with: a passage, a callout's words or a collection's first row. */
interface First {
  text: string;
  cells?: Record<string, Cell>;
}

/** Adding on a page, with what Add a block and a draft need besides. */
export interface PageAdding extends Adding {
  /** Keeps the draft as a block, with what it starts with; `keepWriting` leaves it open to write more. */
  make: (block: NewBlock, first?: First, keepWriting?: boolean) => void;
  /** Picks the photos a new picture holds. */
  picker: RefObject<HTMLInputElement | null>;
  /** Makes a picture of the photos picked (after the block that picked them) or dropped (at the end). */
  pictures: {
    busy: boolean;
    problem: string | null;
    picked: (files: FileList | null) => void;
    dropped: (files: FileList) => void;
  };
}

/**
 * Adding to a topic's page, or a row's (`row`): `shown` are the page's
 * blocks in order, `all` every block of the topic. A block added below
 * another takes the place after it, and the ones after it move down, as
 * Move down moves them; one added at the end goes after every block.
 */
export function usePageAdding(
  topic: Topic,
  shown: Block[],
  all: Block[],
  row: string | null = null,
): PageAdding {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [fresh, setFresh] = useState<string | null>(null);
  const picker = useRef<HTMLInputElement>(null);
  // Where the photos picked go: after the block whose Add below picked them.
  const pictureAfter = useRef<string | null>(null);

  const place = (block: NewBlock, after: string | null, first?: First): string => {
    const id = uuidv7();
    const at = after ? shown.findIndex((b) => b.id === after) + 1 : 0;
    const moves =
      at > 0
        ? shown.flatMap((b, n) => {
            const position = n < at ? n : n + 1;
            return b.position === position
              ? []
              : [
                  {
                    action: "update" as const,
                    model: "Block" as const,
                    id: b.id,
                    data: { position },
                  },
                ];
          })
        : [];
    sync.transact([
      ...moves,
      {
        action: "create",
        model: "Block",
        id,
        data: {
          topic_id: topic.id,
          kind: block.kind,
          title: block.title ?? "",
          facets: [],
          fields: block.fields ?? [],
          width: "",
          view: block.view ?? "",
          row_id: row ?? undefined,
          position: at > 0 ? at : Math.max(-1, ...all.map((b) => b.position)) + 1,
        },
      },
      ...(first
        ? [
            {
              action: "create" as const,
              model: "Point" as const,
              id: uuidv7(),
              data: {
                topic_id: topic.id,
                block_id: id,
                text: first.text,
                cells: first.cells ?? {},
                seen: true,
                position: 0,
              },
            },
          ]
        : []),
    ]);
    return id;
  };

  const pictures = useAddPictures(topic.id, () => place({ kind: "picture" }, pictureAfter.current));

  const start = (kind: AddKind, after: string | null = null) => {
    setFresh(null);
    if (kind === "picture") {
      setDraft(null);
      pictureAfter.current = after;
      picker.current?.click();
      return;
    }
    setDraft({ kind, after });
  };

  // ⌘K adds at the end of the page that's open.
  const latest = useRef(start);
  latest.current = start;
  useEffect(() => {
    if (guest) return;
    offerAdding((kind) => latest.current(kind));
    return () => offerAdding(null);
  }, [guest]);

  return {
    draft,
    fresh,
    start,
    close: () => setDraft(null),
    settle: () => setFresh(null),
    make: (block, first, keepWriting = false) => {
      if (!draft) return;
      const id = place(block, draft.after, first);
      if (keepWriting) setFresh(id);
      setDraft(null);
    },
    picker,
    pictures: {
      busy: pictures.busy,
      problem: pictures.problem,
      picked: pictures.take,
      dropped: (files) => {
        pictureAfter.current = null;
        pictures.take(files);
      },
    },
  };
}

/** The width a draft takes on the page's grid, its kind's own. */
export function draftWidth(kind: AddKind): string {
  return kind === "heading" || kind === "callout" || kind === "table" ? "whole" : "1/2";
}

/**
 * Add a block, at the end of a page: the set's kinds, in the groups
 * Settings → Blocks shows them. A photo dropped on it makes a picture.
 */
export function AddBlock({ adding }: { adding: PageAdding }) {
  const { busy, problem } = adding.pictures;
  return (
    <DropBox
      onDrop={adding.pictures.dropped}
      className="-mx-2 flex items-center gap-2 self-start rounded-md px-2"
    >
      <Menu.Root>
        <Menu.Trigger
          title="Add a block, or drop a picture here"
          className="ui-button -ml-2 h-7 gap-1.5 px-2 text-xs text-faint hover:text-muted data-popup-open:text-muted"
        >
          <Plus size={14} strokeWidth={1.75} aria-hidden="true" />
          {busy ? "Adding…" : "Add a block"}
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Positioner side="bottom" align="start" sideOffset={4} className="z-50">
            {/* The new block takes focus to be written in. */}
            <Menu.Popup finalFocus={false} className="ui-menu min-w-44 p-1 outline-none">
              {ADDS.map((group, n) => (
                <Menu.Group key={group[0]?.kind}>
                  {n > 0 && <Menu.Separator className="mx-1 my-1 h-px bg-line" />}
                  {group.map(({ kind, name, icon: Icon }) => (
                    <Menu.Item
                      key={kind}
                      onClick={() => adding.start(kind)}
                      className="ui-option flex cursor-default items-center gap-2.5 px-2 py-1.5 outline-none select-none"
                    >
                      <span className="flex shrink-0 text-muted">
                        <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
                      </span>
                      {name}
                    </Menu.Item>
                  ))}
                </Menu.Group>
              ))}
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>
      <input
        ref={adding.picker}
        type="file"
        accept="image/*"
        multiple
        hidden
        onChange={(event) => {
          adding.pictures.picked(event.target.files);
          event.target.value = "";
        }}
      />
      {problem && <p className="text-xs text-warn-ink">{problem}</p>}
    </DropBox>
  );
}

/** The block being added, written where it will read until it's kept. */
export function DraftBlock({ adding, draft }: { adding: PageAdding; draft: Draft }) {
  const { kind } = draft;
  if (kind === "list" || kind === "checklist" || kind === "table") {
    return <DraftCollection adding={adding} kind={kind} />;
  }
  // Kept with its words, or taken off again without.
  const done = (text: string | undefined, enter: boolean) => {
    const words = text?.trim();
    if (!words) adding.close();
    else if (kind === "heading") adding.make({ kind: "heading", title: words });
    else adding.make({ kind }, { text: words }, kind === "paragraph" && enter);
  };
  if (kind === "heading") {
    return (
      <WriteText
        line
        label="Heading"
        placeholder="Heading"
        max={60}
        className="font-semibold"
        onDone={done}
      />
    );
  }
  if (kind === "callout") {
    return (
      <div className="flex flex-col gap-1.5">
        <div className="flex max-w-[68ch] gap-2.5 rounded-md bg-chip px-3.5 py-2.5">
          <Info
            size={16}
            strokeWidth={1.75}
            aria-hidden="true"
            className="mt-0.5 shrink-0 text-muted"
          />
          <div className="min-w-0 flex-1">
            <WriteText label="Callout" placeholder="What to keep in mind" onDone={done} />
          </div>
        </div>
        <WritingKeys />
      </div>
    );
  }
  return (
    <div className="flex max-w-measure flex-col gap-1.5">
      <WriteText label="Paragraph" placeholder="Write something" onDone={done} />
      <WritingKeys next />
    </div>
  );
}

/** Each collection's first row, as + New asks for it. */
const FIRST_ROW: Record<"list" | "checklist" | "table", string> = {
  list: "First item",
  checklist: "First thing to do",
  table: "First row",
};

/**
 * A list, a checklist or a table being added: its title, then its first
 * row. Kept, it stays open on + New for the next.
 */
function DraftCollection({
  adding,
  kind,
}: {
  adding: PageAdding;
  kind: "list" | "checklist" | "table";
}) {
  const [title, setTitle] = useState("");
  const [name, setName] = useState("");
  const [notes, setNotes] = useState("");
  const row = useRef<HTMLInputElement>(null);
  const opening = useOpening();
  const add = () => {
    if (!name.trim()) return;
    const fields = fieldsOf(kind);
    const [notesField] = fields;
    adding.make(
      { kind: "collection", title: title.trim(), fields, view: kind === "table" ? "table" : "" },
      {
        text: name.trim(),
        cells:
          kind === "table" && notesField && notes.trim() ? { [notesField.name]: notes.trim() } : {},
      },
      true,
    );
  };
  const box = "ui-field h-8 rounded-md px-2 text-sm";
  return (
    <form
      aria-label={`New ${kind}`}
      onSubmit={(e) => {
        e.preventDefault();
        add();
      }}
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        e.preventDefault();
        e.stopPropagation();
        adding.close();
      }}
      className="flex flex-col gap-2"
    >
      <input
        // biome-ignore lint/a11y/noAutofocus: it opens to name the collection
        autoFocus
        aria-label="Title"
        placeholder={`Name this ${kind}`}
        value={title}
        maxLength={60}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={opening}
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          e.preventDefault();
          row.current?.focus();
        }}
        className="writing"
      />
      <div className="flex flex-wrap items-center gap-2 rounded-lg bg-surface p-2">
        <input
          ref={row}
          aria-label="Name"
          placeholder={FIRST_ROW[kind]}
          value={name}
          maxLength={2000}
          onChange={(e) => setName(e.target.value)}
          className={`${box} min-w-40 flex-[2]`}
        />
        {kind === "table" && (
          <input
            aria-label="Notes"
            placeholder="Notes"
            value={notes}
            maxLength={2000}
            onChange={(e) => setNotes(e.target.value)}
            className={`${box} min-w-32 flex-[3]`}
          />
        )}
        <span className="ml-auto flex gap-1.5">
          <Button variant="secondary" onClick={adding.close}>
            Cancel
          </Button>
          <Button type="submit" disabled={!name.trim()}>
            Add
          </Button>
        </span>
      </div>
    </form>
  );
}
