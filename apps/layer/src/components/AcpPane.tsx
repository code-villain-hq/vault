import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type ClipboardEvent } from "react";
import { api } from "../lib/api";
import { heldConversation, holdConversation, pruneConversations } from "../lib/acpPool";
import { onAcpUpdate } from "../lib/acpUpdates";
import { pastedImages, readImage, type PastedImage } from "../lib/pastedImage";
import { useStore } from "../store";
import type { AcpEntry, AcpToolContent, AcpView, PaneInfo } from "../lib/types";
import { Markdown } from "./Markdown";
import { TerminalPane } from "./Terminal";

/**
 * Where a pane is shown: a conversation for an agent over ACP (§20), its
 * terminal otherwise. Memoised: the store keeps a pane's object until it
 * changes, so one agent's activity redraws its own pane and not the others.
 */
export const PaneView = memo(function PaneView({ pane, visible }: { pane: PaneInfo; visible: boolean }) {
  return pane.acp ? <AcpPane pane={pane} visible={visible} /> : <TerminalPane pane={pane} visible={visible} />;
});

/**
 * An agent over ACP, drawn as its conversation rather than a terminal: what
 * it said, the tools it ran with their diffs, its plan, and its permission
 * questions as buttons (ACP-5). Told of changes while on screen and caught up
 * when it comes back (ACP-7), as a terminal is.
 */
function AcpPane({ pane, visible }: { pane: PaneInfo; visible: boolean }) {
  const appActive = useStore((s) => s.appActive);
  const fail = useStore((s) => s.fail);
  const toast = useStore((s) => s.toast);
  const textSize = useStore((s) => s.settings?.ui.conversation_font_size ?? 14);
  const shown = visible && appActive;

  // Started from what the pane held when last on screen (`acpPool.ts`), so
  // coming back draws at once and asks only for what changed since.
  const [view, setView] = useState<AcpView | null>(() => heldConversation(pane.id).view);
  const [entries, setEntries] = useState<AcpEntry[]>(() => heldConversation(pane.id).entries);
  const pulling = useRef(false);
  const again = useRef(false);

  /** What changed since the last look. One at a time; a telling meanwhile asks once more. */
  const pull = useCallback(async () => {
    if (pulling.current) {
      again.current = true;
      return;
    }
    pulling.current = true;
    try {
      do {
        again.current = false;
        const v = await api.acpView(pane.id, heldConversation(pane.id).rev);
        const held = holdConversation(pane.id, v);
        setEntries(held.entries);
        setView(held.view);
      } while (again.current);
    } catch {
      // Closed under us: the pane list drops it next.
    } finally {
      pulling.current = false;
    }
  }, [pane.id]);

  useEffect(() => {
    if (!shown) return;
    // Not throttled here: the backend gathers a burst into one event
    // (`UPDATE_GATHER`), and a pull already running takes the next one in.
    // Throttling to animation frames stopped pulls while the window was
    // hidden, where WebKit runs none.
    const off = onAcpUpdate(pane.id, () => void pull());
    void pull();
    return () => {
      off();
      void api.ptyDetach(pane.id).catch(() => {});
    };
  }, [pane.id, shown, pull]);

  // Held at the bottom while it is there; left alone once scrolled up to read.
  const scroller = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && atBottom.current) el.scrollTop = el.scrollHeight;
  }, [entries, shown]);

  const [draft, setDraft] = useState("");
  const [images, setImages] = useState<PastedImage[]>([]);
  const send = async () => {
    const text = draft.trim();
    if (!text && images.length === 0) return;
    const sent = images;
    setDraft("");
    setImages([]);
    atBottom.current = true;
    try {
      await api.acpPrompt(pane.id, text, sent.map(({ mimeType, data }) => ({ mimeType, data })));
    } catch (e) {
      setDraft(text);
      setImages(sent);
      fail(e);
    }
  };
  const paste = (e: ClipboardEvent) => {
    const files = pastedImages(e.clipboardData);
    if (files.length === 0) return;
    // Not the link that came with it (ACP-16).
    e.preventDefault();
    if (!view?.images) {
      toast("error", `${view?.agent ?? "This agent"} does not take images.`);
      return;
    }
    void Promise.all(files.map(readImage)).then((read) => setImages((now) => [...now, ...read])).catch(fail);
  };
  const stop = () => void api.acpCancel(pane.id).catch(fail);
  const answer = (entry: number, option: string) => void api.acpAnswer(pane.id, entry, option).catch(fail);
  const set = (setting: string, value: string) => void api.acpSet(pane.id, setting, value).catch(fail);

  const busy = view?.busy ?? false;
  const over = !pane.running || (view?.exited ?? false);
  const commands = draft.startsWith("/") && !draft.includes(" ")
    ? (view?.commands ?? []).filter((c) => c.name.startsWith(draft.slice(1))).slice(0, 8)
    : [];

  return (
    <div
      className={`acp${visible ? "" : " hidden"}`}
      // Every size in the conversation is a multiple of this (ACP-15).
      style={{ ["--acp-text" as string]: `${textSize}px` }}
      onKeyDown={(e) => {
        // Esc stops the turn, as it does in a terminal (ACP-6).
        if (e.key === "Escape" && busy) {
          e.preventDefault();
          stop();
        }
      }}
    >
      <div
        className="acp-log"
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        <div className="acp-col">
          {entries.length === 0 && (
            <div className="acp-empty">
              <div className="acp-empty-title">{pane.title}</div>
              <div>{view?.ready ? "Ask it anything. Type / for its commands." : "Starting…"}</div>
            </div>
          )}
          {entries.map((e) => <Entry key={e.index} entry={e} onAnswer={answer} />)}
          {busy && <div className="acp-working"><span className="dot live" /> Working · Esc to stop</div>}
        </div>
      </div>

      <div className="acp-compose">
        <div className="acp-col">
          {commands.length > 0 && (
            <div className="acp-commands">
              {commands.map((c) => (
                <button key={c.name} className="acp-command" onClick={() => setDraft(`/${c.name} `)}>
                  <span className="mono">/{c.name}</span> <span className="muted">{c.hint ?? c.description}</span>
                </button>
              ))}
            </div>
          )}
          <div className="acp-box">
            {images.length > 0 && (
              <div className="acp-images">
                {images.map((img, i) => (
                  <div key={i} className="acp-image">
                    <img src={img.url} alt={`Pasted image ${i + 1}`} />
                    <button
                      className="acp-image-remove"
                      title="Remove"
                      aria-label={`Remove pasted image ${i + 1}`}
                      onClick={() => setImages((now) => now.filter((_, j) => j !== i))}
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}
            <textarea
              rows={3}
              value={draft}
              disabled={over}
              placeholder={over ? "The agent has exited." : busy ? "Sent when this turn ends…" : "Message the agent · Enter to send, Shift-Enter for a new line"}
              onChange={(e) => setDraft(e.target.value)}
              onPaste={paste}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  void send();
                }
              }}
            />
            <div className="acp-bar">
              {(view?.settings ?? []).map((s) => (
                <label key={s.id} className="acp-setting" title={s.options.find((o) => o.value === s.current)?.description ?? s.name}>
                  <span>{s.name}</span>
                  <select value={s.current} disabled={over} onChange={(e) => set(s.id, e.target.value)}>
                    {s.options.map((o) => <option key={o.value} value={o.value}>{o.name}</option>)}
                  </select>
                </label>
              ))}
              <span className="spacer" />
              {view?.usage && (
                <span className="muted" title="Context used, of the model's window">
                  {kilo(view.usage.used)} / {kilo(view.usage.size)}{view.usage.cost ? ` · ${view.usage.cost}` : ""}
                </span>
              )}
              {view?.agent && <span className="muted">{view.agent}</span>}
              {busy ? (
                <button className="btn btn-sm" onClick={stop}>Stop</button>
              ) : (
                <button className={`btn btn-sm${over || (!draft.trim() && images.length === 0) ? "" : " btn-primary"}`} disabled={over || (!draft.trim() && images.length === 0)} onClick={() => void send()}>Send</button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// A pane that has left the list has been closed, and what it held goes with
// it, as its terminal would (`Terminal.tsx`).
const unsubscribe = useStore.subscribe((s, prev) => {
  if (s.panes !== prev.panes) pruneConversations(s.panes.map((p) => p.id));
});
import.meta.hot?.dispose(unsubscribe);

function kilo(n: number): string {
  return n >= 1000 ? `${Math.round(n / 1000)}k` : String(n);
}

/** One entry. Redrawn only when it changed: a reply streams in token by token. */
const Entry = memo(function Entry({ entry, onAnswer }: {
  entry: AcpEntry;
  onAnswer: (entry: number, option: string) => void;
}) {
  switch (entry.kind) {
    case "user":
      return (
        <div className={`acp-user${entry.dropped ? " dropped" : ""}`}>
          <div className="acp-user-text">{entry.text}</div>
          {entry.queued && <div className="muted">Waiting for the turn before it to end</div>}
          {entry.dropped && <div className="muted">Not sent: the turn before it was stopped</div>}
        </div>
      );
    case "agent":
      return <div className="acp-agent"><Markdown text={entry.text} /></div>;
    case "thought":
      return (
        <details className="acp-thought">
          <summary>Thinking</summary>
          <div>{entry.text}</div>
        </details>
      );
    case "tool":
      return <Tool entry={entry} />;
    case "plan":
      return (
        <div className="acp-plan">
          <div className="acp-label">Plan</div>
          {entry.steps.map((s, i) => (
            <div key={i} className={`acp-step ${s.status}`}>
              <span className="mark">{s.status === "completed" ? "✓" : s.status === "in_progress" ? "◐" : "○"}</span>
              {s.content}
            </div>
          ))}
        </div>
      );
    case "question": {
      const chosen = entry.options.find((o) => o.id === entry.answer);
      return (
        <div className={`acp-question${entry.answer ? " answered" : ""}`}>
          <div className="acp-label">Allow {entry.title}?</div>
          {entry.answer ? (
            <div className="muted">→ {chosen?.name ?? (entry.answer === "cancelled" ? "Stopped" : entry.answer)}</div>
          ) : (
            <div className="row">
              {entry.options.map((o) => (
                <button
                  key={o.id}
                  className={`btn btn-sm${o.kind === "allow_once" ? " btn-primary" : o.kind.startsWith("reject") ? " btn-danger" : ""}`}
                  onClick={() => onAnswer(entry.index, o.id)}
                >
                  {o.name}
                </button>
              ))}
            </div>
          )}
        </div>
      );
    }
    case "note":
      return <div className={`acp-note${entry.error ? " error" : ""}`}>{entry.text}</div>;
  }
}, (a, b) => a.entry === b.entry);

const STATUS_MARK: Record<string, string> = { pending: "○", in_progress: "◐", completed: "✓", failed: "✗" };

function Tool({ entry }: { entry: Extract<AcpEntry, { kind: "tool" }> }) {
  // Its output is drawn once it is first opened, not before. A long
  // conversation holds hundreds of closed tool calls, each with up to 64KB
  // of output, and every one of them was in the page, diffs worked out,
  // whether or not anyone ever looked.
  const [opened, setOpened] = useState(false);
  const detail = entry.content.length > 0 || entry.locations.length > 0;
  const head = (
    <>
      <span className={`mark ${entry.status}`}>{STATUS_MARK[entry.status] ?? "○"}</span>
      <span className="acp-tool-title">{entry.title}</span>
      {entry.tool !== "other" && <span className="acp-tag">{entry.tool}</span>}
    </>
  );
  if (!detail) return <div className="acp-tool"><div className="acp-tool-head">{head}</div></div>;
  // Open while an edit waits, so a question about it is answered seeing what
  // it would write; and when it failed, to show why.
  const waiting = entry.status === "pending" && entry.content.some((c) => c.type === "diff");
  const open = waiting || entry.status === "failed";
  return (
    <details
      className="acp-tool"
      open={open || undefined}
      onToggle={(e) => { if (e.currentTarget.open) setOpened(true); }}
    >
      <summary className="acp-tool-head">{head}</summary>
      {(open || opened) && (
        <>
          {entry.locations.length > 0 && (
            <div className="acp-locations mono">{entry.locations.join("\n")}</div>
          )}
          {entry.content.map((c, i) => <ToolOutput key={i} content={c} />)}
        </>
      )}
    </details>
  );
}

function ToolOutput({ content }: { content: AcpToolContent }) {
  if (content.type === "text") return <pre className="acp-output">{content.text}</pre>;
  return (
    <div className="acp-diff">
      <div className="acp-diff-path mono">{content.path}{content.old === null ? " (new)" : ""}</div>
      <pre>
        {changedLines(content.old ?? "", content.new).map((l, i) => (
          <div key={i} className={l.mark === "+" ? "add" : l.mark === "-" ? "del" : "ctx"}>{l.mark} {l.text}</div>
        ))}
      </pre>
    </div>
  );
}

/**
 * The lines that changed, with up to three either side: the shared start and
 * end are cut down, and the middle shown as removed then added. Enough to
 * judge an edit by, which is what a permission question needs; the Diff tab
 * has the real thing.
 */
function changedLines(old: string, now: string): { mark: string; text: string }[] {
  const a = old === "" ? [] : old.split("\n");
  const b = now.split("\n");
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let end = 0;
  while (end < a.length - start && end < b.length - start && a[a.length - 1 - end] === b[b.length - 1 - end]) end++;
  const context = 3;
  const out: { mark: string; text: string }[] = [];
  for (const l of a.slice(Math.max(0, start - context), start)) out.push({ mark: " ", text: l });
  for (const l of a.slice(start, a.length - end)) out.push({ mark: "-", text: l });
  for (const l of b.slice(start, b.length - end)) out.push({ mark: "+", text: l });
  for (const l of b.slice(b.length - end, Math.min(b.length, b.length - end + context))) out.push({ mark: " ", text: l });
  return out;
}
