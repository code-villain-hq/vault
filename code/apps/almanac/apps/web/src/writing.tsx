// Writing a page by hand: a block of the set added where you want it, and
// what it starts with written in place, as a click on a passage or a
// heading writes in it. It goes through the sync engine, as a talk's
// points do, so it works offline, and a talk reads and tidies it like its
// own. Guests, and pages only to look at, get none of it.

import type { Block, Field, Point } from "@almanac/types";
import type { MoreMenuAction } from "@code-villain/web";
import {
  Heading,
  Image,
  Info,
  List,
  ListChecks,
  type LucideIcon,
  Pilcrow,
  Table2,
} from "lucide-react";
import {
  createContext,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  useContext,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { useIsGuest, useWorkspace } from "./workspace";

/** What a person adds: a kind of the set, a collection as a list, a checklist or a table. */
export type AddKind =
  | "heading"
  | "paragraph"
  | "callout"
  | "list"
  | "checklist"
  | "table"
  | "picture";

/**
 * The blocks a person adds, by the names Settings → Blocks uses, in the
 * groups Add a block shows: what's written, what's kept in rows, a picture.
 */
export const ADDS: { kind: AddKind; name: string; icon: LucideIcon }[][] = [
  [
    { kind: "heading", name: "Heading", icon: Heading },
    { kind: "paragraph", name: "Paragraph", icon: Pilcrow },
    { kind: "callout", name: "Callout", icon: Info },
  ],
  [
    { kind: "list", name: "List", icon: List },
    { kind: "checklist", name: "Checklist", icon: ListChecks },
    { kind: "table", name: "Table", icon: Table2 },
  ],
  [{ kind: "picture", name: "Picture", icon: Image }],
];

/** Each kind of `ADDS` as a ⋯ menu's actions, which `start` adds. */
export function addActions(start: (kind: AddKind) => void): MoreMenuAction[][] {
  return ADDS.map((group) =>
    group.map(({ kind, name, icon: Icon }) => ({
      label: name,
      icon: <Icon size={14} strokeWidth={1.75} aria-hidden="true" />,
      onSelect: () => start(kind),
    })),
  );
}

const field = (name: string, kind: string): Field => ({
  name,
  kind,
  unit: "",
  goal: null,
  options: [],
});

/**
 * The fields a collection starts with, which pick its picture: a list has
 * only its rows' names, a checklist a check to tick, a table a column of
 * notes. Fields… changes them after.
 */
export function fieldsOf(kind: AddKind): Field[] {
  if (kind === "checklist") return [field("Done", "check")];
  if (kind === "table") return [field("Notes", "text")];
  return [];
}

/** A block being written before it's kept: its kind, and the block it goes after (none: the end). */
export interface Draft {
  kind: AddKind;
  after: string | null;
}

/** Adding to the page in front of you, from Add a block, a block's ⋯ or ⌘K. */
export interface Adding {
  draft: Draft | null;
  /**
   * The block just made, still being written: a paragraph's next passage,
   * a collection's next row.
   */
  fresh: string | null;
  /** Starts a block of `kind` after `after`, or at the end; a picture picks its photos. */
  start: (kind: AddKind, after?: string | null) => void;
  /** Leaves the draft. */
  close: () => void;
  /** Ends writing in the block just made. */
  settle: () => void;
}

export const AddingContext = createContext<Adding | null>(null);

/** Adding on this page; none where a page only shows blocks, as About and Settings do. */
export function useAdding(): Adding | null {
  return useContext(AddingContext);
}

// ⌘K adds at the end of the page in front of you, which offers it while open.
let adder: ((kind: AddKind) => void) | null = null;
const heard = new Set<() => void>();

/** Offers ⌘K a page to add to, or none. */
export function offerAdding(add: ((kind: AddKind) => void) | null) {
  adder = add;
  for (const hear of heard) hear();
}

/** What ⌘K adds with, while a page offers it. */
export function useAdder() {
  return useSyncExternalStore(
    (hear) => {
      heard.add(hear);
      return () => heard.delete(hear);
    },
    () => adder,
  );
}

/**
 * A field that opens from a menu or ⌘K takes focus before the popup, as it
 * closes, hands it back to what opened it. For a moment after it opens, the
 * field takes it back instead of hearing it leave: true when it did.
 */
export function useOpening() {
  const at = useRef(performance.now());
  return (e: FocusEvent<HTMLElement>) => {
    if (performance.now() - at.current > 500) return false;
    const field = e.currentTarget;
    setTimeout(() => field.isConnected && field.focus());
    return true;
  };
}

/**
 * Words written where they'll read, in a field that grows with them: Enter
 * keeps them, ⇧Enter breaks the line, Escape leaves them as they were.
 * `onDone` hears once, with the words (none for Escape) and whether Enter
 * kept them; leaving keeps them too.
 */
export function WriteText({
  value = "",
  label,
  placeholder,
  max = 2000,
  line = false,
  className = "",
  onDone,
}: {
  value?: string;
  label: string;
  placeholder?: string;
  max?: number;
  /** One line, for a heading. */
  line?: boolean;
  className?: string;
  onDone: (text: string | undefined, enter: boolean) => void;
}) {
  const [text, setText] = useState(value);
  const box = useRef<HTMLTextAreaElement>(null);
  const over = useRef(false);
  const finish = (words: string | undefined, enter: boolean) => {
    if (over.current) return;
    over.current = true;
    onDone(words, enter);
  };
  // As tall as what's written.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it grows as the text changes
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "0";
    el.style.height = `${el.scrollHeight}px`;
  }, [text]);
  const keys = (e: KeyboardEvent<HTMLTextAreaElement | HTMLInputElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      finish(text, true);
    }
    if (e.key === "Escape") {
      e.preventDefault();
      // Not the talk's Escape too.
      e.stopPropagation();
      finish(undefined, false);
    }
  };
  const opening = useOpening();
  const common = {
    autoFocus: true,
    "aria-label": label,
    placeholder,
    maxLength: max,
    value: text,
    onKeyDown: keys,
    onBlur: (e: FocusEvent<HTMLElement>) => {
      if (!opening(e)) finish(text, false);
    },
    onFocus: (e: { currentTarget: HTMLTextAreaElement | HTMLInputElement }) => {
      const end = e.currentTarget.value.length;
      e.currentTarget.setSelectionRange(end, end);
    },
    className: `writing ${className}`,
  };
  if (line) return <input {...common} onChange={(e) => setText(e.target.value)} />;
  return <textarea {...common} ref={box} rows={1} onChange={(e) => setText(e.target.value)} />;
}

/** Under a field written in: what its keys do. */
export function WritingKeys({ next = false }: { next?: boolean }) {
  return (
    <span className="flex gap-3 text-xs text-faint">
      <span>
        <kbd className="ui-kbd mr-1">↵</kbd>
        {next ? "keep, next passage" : "keep"}
      </span>
      <span>
        <kbd className="ui-kbd mr-1">⇧↵</kbd>new line
      </span>
      <span>
        <kbd className="ui-kbd mr-1">esc</kbd>
        {next ? "stop" : "leave it"}
      </span>
    </span>
  );
}

/**
 * A passage a click writes in, as a click on a row's value does: Enter or
 * leaving keeps it, Escape doesn't, and emptied it leaves the page. A click
 * on a link still opens it, and one that selects words only selects them.
 */
export function Passage({ point, children }: { point: Point; children: ReactNode }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [writing, setWriting] = useState(false);
  if (guest) return <span className="min-w-0 flex-1">{children}</span>;
  if (writing) {
    return (
      <span className="flex min-w-0 flex-1 flex-col gap-1.5">
        <WriteText
          value={point.text}
          label="Passage"
          onDone={(text) => {
            setWriting(false);
            const words = text?.trim();
            if (words === undefined || words === point.text) return;
            if (words) sync.update("Point", point.id, { text: words });
            else sync.delete("Point", point.id);
          }}
        />
        <WritingKeys />
      </span>
    );
  }
  const write = (e: MouseEvent) => {
    if ((e.target as Element).closest("a") || getSelection()?.toString()) return;
    setWriting(true);
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a passage holds links, which a button can't
    <span
      role="button"
      tabIndex={0}
      title="Write in it"
      onClick={write}
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.target === e.currentTarget) {
          e.preventDefault();
          setWriting(true);
        }
      }}
      className="-mx-1 min-w-0 flex-1 cursor-text rounded px-1 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
    >
      {children}
    </span>
  );
}

/** A heading a click writes in. */
export function EditHeading({ block }: { block: Block }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [writing, setWriting] = useState(false);
  if (guest) return <h2 className="min-w-0 truncate font-semibold">{block.title}</h2>;
  if (writing) {
    return (
      <WriteText
        line
        value={block.title}
        label="Heading"
        max={60}
        className="font-semibold"
        onDone={(text) => {
          setWriting(false);
          const words = text?.trim();
          if (words && words !== block.title) sync.update("Block", block.id, { title: words });
        }}
      />
    );
  }
  return (
    <h2 className="min-w-0 truncate font-semibold">
      <button
        type="button"
        title="Write in it"
        onClick={() => setWriting(true)}
        className="-mx-1 max-w-full cursor-text truncate rounded px-1 text-left hover:bg-hover"
      >
        {block.title}
      </button>
    </h2>
  );
}

/**
 * The next passage of a paragraph just added: Enter keeps it and starts
 * another, and Escape, or leaving it empty, stops.
 */
export function NextPassage({ block, points }: { block: Block; points: Point[] }) {
  const { sync } = useWorkspace();
  const adding = useAdding();
  const [written, setWritten] = useState(0);
  if (!adding || adding.fresh !== block.id) return null;
  return (
    <div className="flex flex-col gap-1.5">
      <WriteText
        key={written}
        label="Next passage"
        placeholder="Next passage"
        onDone={(text, enter) => {
          const words = text?.trim();
          if (words) {
            sync.create("Point", {
              topic_id: block.topic_id,
              block_id: block.id,
              text: words,
              seen: true,
              position: Math.max(-1, ...points.map((p) => p.position)) + 1,
            });
          }
          if (words && enter) setWritten(written + 1);
          else adding.settle();
        }}
      />
      <WritingKeys next />
    </div>
  );
}
