import DOMPurify from "dompurify";
import { marked } from "marked";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useDraft } from "./draft";
import { highlightMentions, useMentionNames } from "./mentions";
import { apiUrl, authHeaders, placeFor } from "./server";

/** Renders Markdown, sanitised: the text comes from people and agents. */
export function Markdown({ text, empty }: { text: string; empty?: string }) {
  const names = useMentionNames();
  const html = useMemo(
    () =>
      highlightMentions(
        DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true, breaks: true })),
        names,
      ),
    [text, names],
  );
  const container = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Again whenever the HTML, and so the DOM, is replaced.
    void html;
    if (container.current) {
      prepareMarkdown(container.current);
      return showAttachments(container.current);
    }
  }, [html]);
  if (!text.trim()) return <p className="text-faint">{empty ?? "Nothing here yet."}</p>;
  // biome-ignore lint/security/noDangerouslySetInnerHtml: sanitised by DOMPurify above
  return <div ref={container} className="markdown" dangerouslySetInnerHTML={{ __html: html }} />;
}

/** Preserve task state and make overflowing code reachable without a pointer. */
function prepareMarkdown(root: HTMLElement) {
  for (const checkbox of root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) {
    if (checkbox.hasAttribute("aria-label") || checkbox.hasAttribute("aria-labelledby")) continue;
    const item = checkbox.closest("li")?.cloneNode(true) as HTMLElement | undefined;
    // Nested tasks have their own names; don't repeat them in the parent's label.
    for (const child of item?.querySelectorAll("ul, ol, input") ?? []) child.remove();
    const label = item?.textContent?.replace(/\s+/g, " ").trim();
    checkbox.setAttribute("aria-label", label || "Task");
  }
  for (const block of root.querySelectorAll("pre")) {
    block.tabIndex = 0;
    block.setAttribute("role", "group");
    block.setAttribute("aria-label", "Code block");
    block.classList.add("outline-none", "focus-visible:ring-2", "focus-visible:ring-accent");
  }
}

/**
 * A workspace on this Mac or on a server the packaged app signed in to
 * isn't on the page's origin, and signs requests with a token, which images
 * can't send: fetch attached images with it and show them from memory, and
 * point attachment links at its place. Returns a cleanup. Nothing to do
 * when the page comes from the workspace's server.
 */
function showAttachments(root: HTMLElement): (() => void) | undefined {
  const urls: string[] = [];
  let cancelled = false;
  for (const link of root.querySelectorAll<HTMLAnchorElement>('a[href^="/api/"]')) {
    const href = link.getAttribute("href") ?? "";
    if (placeFor(href).kind !== "here") link.href = apiUrl(href);
  }
  for (const image of root.querySelectorAll<HTMLImageElement>('img[src^="/api/"]')) {
    const src = image.getAttribute("src") ?? "";
    const place = placeFor(src);
    if (place.kind === "here") continue;
    image.removeAttribute("src");
    void fetch(apiUrl(src, place), { headers: authHeaders(place) })
      .then((r) => (r.ok ? r.blob() : null))
      .then((blob) => {
        if (!blob || cancelled) return;
        const url = URL.createObjectURL(blob);
        urls.push(url);
        image.src = url;
      })
      .catch(() => {});
  }
  return () => {
    cancelled = true;
    for (const url of urls) URL.revokeObjectURL(url);
  };
}

/**
 * Markdown shown rendered; clicking into it opens the source, with the cursor
 * where the click landed, saved as you pause. Leaving the field or Esc shows
 * it rendered again.
 */
export function MarkdownField({
  label,
  hideLabel = false,
  value,
  onSave,
  empty,
  notes,
  view,
}: {
  label: string;
  /** For the one text a page is about, such as a spec's: its name would only repeat the page's. */
  hideLabel?: boolean;
  value: string;
  onSave: (value: string) => void;
  empty?: string;
  /** Shown above the text: about the draft while editing, the saved text otherwise. */
  notes?: (text: string) => ReactNode;
  /** How the saved text shows when not editing, instead of plain Markdown. */
  view?: (text: string) => ReactNode;
}) {
  const [editing, setEditing] = useState(false);
  const caret = useRef(0);
  const field = useRef<HTMLTextAreaElement>(null);
  const draft = useDraft(value, onSave);
  useEffect(() => {
    if (!editing || !field.current) return;
    // Stay where the reader was; the cursor goes where they clicked.
    field.current.focus({ preventScroll: true });
    field.current.setSelectionRange(caret.current, caret.current);
  }, [editing]);

  const edit = (at: number) => {
    caret.current = at;
    setEditing(true);
  };

  return (
    <section className="flex flex-col gap-2">
      <h2 className={hideLabel ? "sr-only" : "text-xs font-medium text-muted"}>{label}</h2>
      {!editing && (
        // For the keyboard; a click into the text does the same.
        <button
          type="button"
          onClick={() => edit(0)}
          className="sr-only self-start rounded text-xs text-muted focus-visible:not-sr-only focus-visible:ring-2 focus-visible:ring-accent"
        >
          Edit {label.toLowerCase()}
        </button>
      )}
      {notes?.(editing ? draft.value : value)}
      {editing ? (
        <textarea
          ref={field}
          aria-label={label}
          value={draft.value}
          onChange={(e) => draft.onChange(e.target.value)}
          onBlur={() => {
            draft.onBlur();
            setEditing(false);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Escape") return;
            // Done editing; Escape shouldn't also leave the page.
            e.stopPropagation();
            e.currentTarget.blur();
          }}
          className="field-sizing-content min-h-40 resize-none rounded-md ui-field p-3 font-mono text-xs leading-relaxed outline-none"
        />
      ) : (
        // biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: the Edit button above does the same from the keyboard
        <div
          className="cursor-text"
          onClick={(event) => {
            // Links open, buttons act, a requirement's title unfolds it, and
            // selecting text is for copying; any other click edits.
            const target = event.target as Element;
            if (target.closest("a, button, summary, input, textarea, select")) return;
            if (window.getSelection()?.toString()) return;
            edit(sourceOffset(value, event.clientX, event.clientY));
          }}
        >
          {view && value.trim() ? view(value) : <Markdown text={value} empty={empty} />}
        </div>
      )}
    </section>
  );
}

/**
 * Roughly where in the Markdown source a click on its rendered text landed:
 * the words around the click, found in the source. The start when they
 * can't be found.
 */
function sourceOffset(source: string, x: number, y: number): number {
  let node: Node | null = null;
  let offset = 0;
  if (document.caretPositionFromPoint) {
    const position = document.caretPositionFromPoint(x, y);
    node = position?.offsetNode ?? null;
    offset = position?.offset ?? 0;
  } else if (document.caretRangeFromPoint) {
    const range = document.caretRangeFromPoint(x, y);
    node = range?.startContainer ?? null;
    offset = range?.startOffset ?? 0;
  }
  if (node?.nodeType !== Node.TEXT_NODE) return 0;
  const text = node.textContent ?? "";
  const before = text.slice(Math.max(0, offset - 20), offset);
  const after = text.slice(offset, offset + 20);
  for (const [probe, shift] of [
    [before + after, before.length],
    [after, 0],
    [before, before.length],
  ] as const) {
    if (probe.trim().length < 4) continue;
    const found = source.indexOf(probe);
    if (found >= 0) return found + shift;
  }
  return 0;
}
