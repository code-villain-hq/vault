// The text field for descriptions and comments: "@" suggests people and
// agents to mention, and pasted or dropped files are uploaded and linked in the
// Markdown. Also highlights mentions in rendered text. Mentions are plain
// text ("@Ada Lovelace"), which the server matches against members' names.
import type { User } from "@forge/types";
import {
  forwardRef,
  type KeyboardEvent,
  type TextareaHTMLAttributes,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { messageOf, uploadAttachment } from "./api";
import { useUsers } from "./data";
import { Avatar } from "./icons";
import { useWorkspace } from "./workspace";

/** The "@query" just before the caret, if the caret is in one. */
function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const match = /(^|\s)@([^\s@]*(?: [^\s@]*)?)$/.exec(text.slice(0, caret));
  if (!match) return null;
  const query = match[2] ?? "";
  return { start: caret - query.length - 1, query };
}

type Props = Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "value" | "onChange"> & {
  value: string;
  issueId?: string;
  teamId?: string;
  onChange: (value: string) => void;
  /** Told when someone starts or stops choosing whom to mention. */
  onMentioning?: (active: boolean) => void;
  /** Told while files are uploading, e.g. to hold off sending. */
  onUploading?: (busy: boolean) => void;
  /**
   * Called instead of `onChange` when an upload lands after the field lost
   * focus, so the parent can save the text then and there.
   */
  onCommit?: (value: string) => void;
  /**
   * Shows the suggestions in the flow, pushing what follows down, for a field
   * in a container that scrolls (a dialog) and would cut a floating list off.
   */
  inlineSuggestions?: boolean;
};

/** A textarea where "@" offers the workspace's people and agents. */
export const MentionTextarea = forwardRef<HTMLTextAreaElement | null, Props>(
  function MentionTextarea(
    {
      value,
      issueId,
      teamId,
      onChange,
      onMentioning,
      onUploading,
      onCommit,
      inlineSuggestions,
      onKeyDown,
      onBlur,
      ...props
    },
    ref,
  ) {
    const field = useRef<HTMLTextAreaElement>(null);
    useImperativeHandle(ref, () => field.current as HTMLTextAreaElement);
    const people = useUsers();
    const [caret, setCaret] = useState<number | null>(null);
    const [active, setActive] = useState(0);
    const [dismissed, setDismissed] = useState<number | null>(null);

    const mention = caret === null ? null : mentionAt(value, caret);
    const matches = useMemo(() => {
      if (!mention) return [];
      const q = mention.query.toLowerCase();
      return people
        .filter((p) => p.name.toLowerCase().includes(q))
        .sort(
          (a, b) =>
            Number(!a.name.toLowerCase().startsWith(q)) -
              Number(!b.name.toLowerCase().startsWith(q)) || a.name.localeCompare(b.name),
        )
        .slice(0, 6);
    }, [mention, people]);
    const open = mention !== null && matches.length > 0 && dismissed !== mention.start;
    useEffect(() => onMentioning?.(open), [open, onMentioning]);
    // Not choosing any more once the field is gone.
    useEffect(() => () => onMentioning?.(false), [onMentioning]);
    const { workspace } = useWorkspace();
    const [uploading, setUploading] = useState(0);
    useEffect(() => onUploading?.(uploading > 0), [uploading, onUploading]);
    const [problem, setProblem] = useState<string | null>(null);
    // The latest text, for uploads that finish after more typing.
    const latest = useRef(value);
    latest.current = value;

    /** Uploads pasted or dropped files and puts their Markdown at the caret. */
    const attach = async (files: File[]) => {
      if (files.length === 0) return;
      setProblem(null);
      setUploading((n) => n + files.length);
      const at = field.current?.selectionStart ?? latest.current.length;
      const added: string[] = [];
      for (const file of files) {
        try {
          added.push(await uploadAttachment(workspace.id, file, { issueId, teamId }));
        } catch (e) {
          setProblem(messageOf(e));
        } finally {
          setUploading((n) => n - 1);
        }
      }
      if (added.length === 0) return;
      const text = latest.current;
      const before = text.slice(0, at);
      const gap = before === "" || before.endsWith("\n") ? "" : "\n";
      const inserted = `${gap}${added.join("\n")}\n`;
      const next = before + inserted + text.slice(at);
      const focused = field.current !== null && document.activeElement === field.current;
      if (!focused && onCommit) {
        onCommit(next);
        return;
      }
      onChange(next);
      // Carry on typing after what was added.
      const caretAfter = at + inserted.length;
      requestAnimationFrame(() => field.current?.setSelectionRange(caretAfter, caretAfter));
    };

    const choose = (person: User) => {
      if (!mention || caret === null) return;
      const inserted = `@${person.name} `;
      const next = value.slice(0, mention.start) + inserted + value.slice(caret);
      onChange(next);
      const position = mention.start + inserted.length;
      setCaret(position);
      requestAnimationFrame(() => field.current?.setSelectionRange(position, position));
    };

    const keyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (open) {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          const step = event.key === "ArrowDown" ? 1 : -1;
          setActive((i) => (i + step + matches.length) % matches.length);
          return;
        }
        if ((event.key === "Enter" || event.key === "Tab") && !event.metaKey && !event.ctrlKey) {
          const person = matches[Math.min(active, matches.length - 1)];
          if (person) {
            event.preventDefault();
            choose(person);
            return;
          }
        }
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          setDismissed(mention?.start ?? null);
          return;
        }
      }
      onKeyDown?.(event);
    };

    return (
      <div className="relative flex flex-col">
        <textarea
          ref={field}
          {...props}
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            setCaret(e.target.selectionStart);
            setActive(0);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onKeyDown={keyDown}
          onPaste={(e) => {
            const files = [...e.clipboardData.files];
            // Office apps put a picture of copied text next to the text: paste the text.
            if (files.length === 0 || e.clipboardData.getData("text/plain")) return;
            e.preventDefault();
            void attach(files);
          }}
          onDragOver={(e) => {
            if (e.dataTransfer.types.includes("Files")) e.preventDefault();
          }}
          onDrop={(e) => {
            const files = [...e.dataTransfer.files];
            if (files.length === 0) return;
            e.preventDefault();
            void attach(files);
          }}
          onBlur={(e) => {
            setCaret(null);
            onBlur?.(e);
          }}
        />
        {(uploading > 0 || problem) && (
          <p className={`mt-1 text-xs ${problem ? "text-danger-ink" : "text-muted"}`}>
            {problem ?? `Uploading ${uploading === 1 ? "a file" : `${uploading} files`}…`}
          </p>
        )}
        {open && (
          <div
            role="listbox"
            aria-label="People to mention"
            className={`${inlineSuggestions ? "" : "absolute top-full left-0 z-20 "}mt-1 w-60 rounded-md bg-surface p-1`}
          >
            {matches.map((person, i) => (
              <div
                key={person.id}
                role="option"
                aria-selected={i === active}
                tabIndex={-1}
                // Keeps focus in the textarea, so the caret stays put.
                onMouseDown={(e) => {
                  e.preventDefault();
                  choose(person);
                }}
                onMouseMove={() => setActive(i)}
                className={`ui-option flex cursor-default items-center gap-2 px-2 py-1.5 ${i === active ? "is-active" : ""}`}
              >
                <Avatar user={person} size="size-4" />
                <span className="truncate">{person.name}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  },
);

/** Wraps "@Name" for each of `names` in rendered HTML, outside code and links. */
export function highlightMentions(html: string, names: string[]): string {
  if (names.length === 0 || !html.includes("@")) return html;
  const escaped = [...names]
    .sort((a, b) => b.length - a.length)
    .map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(`(^|[^\\w@])@(${escaped.join("|")})(?![\\w])`, "gi");
  const template = document.createElement("template");
  template.innerHTML = html;
  const walker = document.createTreeWalker(template.content, NodeFilter.SHOW_TEXT);
  const texts: Text[] = [];
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    if (!node.parentElement?.closest("code, pre, a")) texts.push(node);
  }
  for (const node of texts) {
    const text = node.data;
    pattern.lastIndex = 0;
    if (!pattern.test(text)) continue;
    pattern.lastIndex = 0;
    const fragment = document.createDocumentFragment();
    let last = 0;
    for (const match of text.matchAll(pattern)) {
      const start = (match.index ?? 0) + (match[1]?.length ?? 0);
      fragment.append(text.slice(last, start));
      const span = document.createElement("span");
      span.className = "mention";
      span.textContent = `@${match[2]}`;
      fragment.append(span);
      last = start + 1 + (match[2]?.length ?? 0);
    }
    fragment.append(text.slice(last));
    node.replaceWith(fragment);
  }
  return template.innerHTML;
}

/**
 * Everything members (people and agents) can be mentioned by: full names, and
 * first names only one member has, as the server reads mentions.
 */
export function useMentionNames(): string[] {
  const users = useUsers();
  return useMemo(() => {
    const firstOf = (name: string) => name.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
    const names = new Set<string>();
    for (const user of users) {
      names.add(user.name);
      const first = user.name.trim().split(/\s+/)[0];
      const shared = users.some((u) => u.id !== user.id && firstOf(u.name) === firstOf(user.name));
      if (first && !shared) names.add(first);
    }
    return [...names];
  }, [users]);
}
