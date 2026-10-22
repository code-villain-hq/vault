import { uuidv7 } from "@code-villain/sync-client";
import { shortcutFree } from "@code-villain/web";
import type { Suggestion } from "@glyph/types";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { messageOf } from "./api";
import { acceptSuggestion, transactInBatches } from "./bulk";
import {
  type Entry,
  entryId,
  isMissing,
  languageName,
  plural,
  textOf,
  useCatalog,
  useLanguages,
  useSource,
  useThresholds,
} from "./data";
import { PageHeader } from "./layout";
import { Empty } from "./page-parts";
import { runProgress, SUGGEST_LIMIT, useActiveRun, useFailedRun, useRuns } from "./runs";
import { Button, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

export interface ReviewSearch {
  target?: string;
}

/** Why a proposal needs a look before it's taken, if it does. */
export function concerns(suggestion: Suggestion, threshold: number): string[] {
  const reasons = suggestion.glossary_violations.map(
    (v) => `Glossary: “${v.source_term}” should be “${v.target_term}”`,
  );
  if (suggestion.placeholders_broken) reasons.push("A {{placeholder}} went missing");
  if (suggestion.confidence < threshold) reasons.push("The model wasn't sure");
  return reasons;
}

/** Proposals that need a look first, then by key. */
export function orderSuggestions(suggestions: Suggestion[], threshold: number): Suggestion[] {
  return [...suggestions].sort((a, b) => {
    const looks = concerns(b, threshold).length - concerns(a, threshold).length;
    if (looks !== 0) return looks;
    return `${a.namespace}.${a.key}`.localeCompare(`${b.namespace}.${b.key}`);
  });
}

/** For back-filling the source: the language with text for most of its empty keys. */
function backFillFrom(entries: Entry[], source: string, others: string[]): string | undefined {
  let best: string | undefined;
  let most = 0;
  for (const lang of others) {
    const count = entries.filter((e) => !textOf(e, source).trim() && textOf(e, lang).trim()).length;
    if (count > most) {
      best = lang;
      most = count;
    }
  }
  return best;
}

type Item =
  | { kind: "suggestion"; id: string; suggestion: Suggestion; entry: Entry | undefined }
  | { kind: "missing"; id: string; entry: Entry };

/** `/$workspace/review`: a tab per language with missing keys; proposals to accept, then the rest. */
export function ReviewPage() {
  const search = useSearch({ strict: false }) as ReviewSearch;
  const navigate = useNavigate();
  const { sync, workspace } = useWorkspace();
  const languages = useLanguages();
  const source = useSource();
  const catalog = useCatalog();
  const threshold = useThresholds(source);
  const suggestions = useRecords("Suggestion");
  const runs = useRuns();
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [writing, setWriting] = useState<string | null>(null);

  const entries = useMemo(() => [...catalog.entries.values()], [catalog]);
  const codes = useMemo(() => languages.map((l) => l.code), [languages]);

  const tabs = useMemo(
    () =>
      codes
        .map((lang) => ({
          lang,
          missing: entries.filter((e) => isMissing(e, lang, source)).length,
          waiting: suggestions.filter((s) => s.lang === lang).length,
        }))
        .filter((t) => t.missing > 0 || t.waiting > 0),
    [codes, entries, source, suggestions],
  );

  const target =
    tabs.find((t) => t.lang === search.target)?.lang ??
    [...tabs].sort((a, b) => b.waiting - a.waiting || b.missing - a.missing)[0]?.lang;
  const isBackFill = target === source;
  const from = isBackFill
    ? backFillFrom(
        entries,
        source,
        codes.filter((c) => c !== source),
      )
    : source;
  const run = useActiveRun(from ?? source, target ?? source);
  const failed = useFailedRun(from ?? source, target ?? source);

  const items = useMemo<Item[]>(() => {
    if (!target) return [];
    const q = filter.trim().toLowerCase();
    const matches = (ns: string, key: string, texts: string[]) =>
      !q ||
      `${ns}.${key}`.toLowerCase().includes(q) ||
      texts.some((t) => t.toLowerCase().includes(q));
    const waiting = orderSuggestions(
      suggestions.filter((s) => s.lang === target),
      threshold(target),
    );
    const proposed = new Set(waiting.map((s) => entryId(s.namespace, s.key)));
    return [
      ...waiting
        .filter((s) => matches(s.namespace, s.key, [s.value, s.source_text]))
        .map(
          (s): Item => ({
            kind: "suggestion",
            id: s.id,
            suggestion: s,
            entry: catalog.entries.get(entryId(s.namespace, s.key)),
          }),
        ),
      ...entries
        .filter(
          (e) =>
            isMissing(e, target, source) &&
            !proposed.has(entryId(e.namespace, e.key)) &&
            (!isBackFill || (from && textOf(e, from).trim())) &&
            matches(e.namespace, e.key, [textOf(e, from ?? source)]),
        )
        .map((e): Item => ({ kind: "missing", id: entryId(e.namespace, e.key), entry: e })),
    ];
  }, [target, filter, suggestions, threshold, catalog, entries, source, isBackFill, from]);

  const proposals = items.filter((i) => i.kind === "suggestion");
  const ready = proposals.filter(
    (i) => i.kind === "suggestion" && concerns(i.suggestion, threshold(target ?? "")).length === 0,
  );
  const current = items[Math.min(selected, items.length - 1)];

  const accept = (suggestion: Suggestion, value?: string) => {
    const existing = catalog.entries
      .get(entryId(suggestion.namespace, suggestion.key))
      ?.values.get(suggestion.lang);
    sync.transact(acceptSuggestion(suggestion, existing, value ?? suggestion.value, uuidv7));
  };
  const reject = (suggestion: Suggestion) => sync.delete("Suggestion", suggestion.id);
  const write = (entry: Entry, value: string) => {
    if (!target || !value.trim()) return;
    const existing = entry.values.get(target);
    if (existing) {
      sync.update("Translation", existing.id, { value, confidence: null, translated_by: "human" });
    } else {
      sync.create("Translation", {
        lang: target,
        namespace: entry.namespace,
        key: entry.key,
        value,
      });
    }
  };

  const suggest = async (keys?: Entry[]) => {
    if (!target || !from) return;
    setError(null);
    try {
      await runs.start({
        kind: "suggest",
        source_lang: from,
        target_lang: target,
        keys: keys?.map((e) => ({ namespace: e.namespace, key: e.key })),
        limit: SUGGEST_LIMIT,
      });
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const acceptReady = () => {
    const operations = ready.flatMap((i) =>
      i.kind === "suggestion"
        ? acceptSuggestion(i.suggestion, i.entry?.values.get(i.suggestion.lang), undefined, uuidv7)
        : [],
    );
    transactInBatches(sync, operations);
  };

  // j/k move, ↵ suggests or accepts, w writes, x rejects, s skips, and Esc
  // goes back, as it does everywhere: it never deletes. An open dialog (the
  // shortcuts list, the command menu) keeps them, so Esc only closes it.
  const keys = useRef({ current, items, selected, accept, reject, suggest });
  keys.current = { current, items, selected, accept, reject, suggest };
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!shortcutFree(event)) return;
      const { current, items, selected, accept, reject, suggest } = keys.current;
      const move = (by: number) =>
        setSelected(Math.max(0, Math.min(items.length - 1, selected + by)));
      if (event.key === "j" || event.key === "ArrowDown") move(1);
      else if (event.key === "k" || event.key === "ArrowUp") move(-1);
      else if (event.key === "s") move(1);
      else if (event.key === "Escape") window.history.back();
      else if (!current) return;
      else if (event.key === "Enter") {
        if (current.kind === "suggestion") accept(current.suggestion);
        else void suggest([current.entry]);
      } else if (event.key === "x" && current.kind === "suggestion") {
        reject(current.suggestion);
      } else if (event.key === "w") {
        setWriting(current.id);
      } else return;
      event.preventDefault();
      if (event.key === "s") setSkipped((s) => new Set(s).add(current?.id ?? ""));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Keep the selected row in view.
  useEffect(() => {
    document.querySelector(`[data-review-row="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const openTab = (lang: string) => {
    setSelected(0);
    setSkipped(new Set());
    void navigate({
      to: "/$workspace/review",
      params: { workspace: workspace.url_key },
      search: { target: lang },
      replace: true,
    });
  };

  if (!target) {
    return (
      <>
        <PageHeader title="Review" />
        <Empty title="Nothing to review">
          Every key has a text in every language, and no proposals are waiting.
        </Empty>
      </>
    );
  }

  const missingCount = items.length - proposals.length;
  return (
    <>
      <PageHeader title="Review">
        <input
          type="search"
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setSelected(0);
          }}
          placeholder="Filter"
          aria-label="Filter by text or key"
          className="ui-field h-7 w-44 rounded-md px-2 text-xs"
        />
        {run ? (
          <span className="flex shrink-0 items-center gap-2 text-xs text-muted">
            <span className="ui-live-dot" aria-hidden="true" />
            {runProgress(run)}
            <Button variant="secondary" onClick={() => void runs.cancel(run.id)}>
              Stop
            </Button>
          </span>
        ) : (
          missingCount > 0 &&
          from && (
            <Button
              variant={ready.length > 0 ? "secondary" : "primary"}
              className="shrink-0"
              onClick={() => void suggest()}
            >
              Suggest {Math.min(missingCount, SUGGEST_LIMIT)}
            </Button>
          )
        )}
        {ready.length > 0 && (
          <Button className="shrink-0" onClick={acceptReady}>
            Accept {ready.length}
          </Button>
        )}
      </PageHeader>
      <nav aria-label="Languages" className="ui-tabs shrink-0 overflow-x-auto px-4">
        {tabs.map((tab) => (
          <button
            key={tab.lang}
            type="button"
            aria-selected={tab.lang === target}
            role="tab"
            onClick={() => openTab(tab.lang)}
            className="ui-tab shrink-0"
          >
            {languageName(tab.lang)}
            {tab.lang === source && from && (
              <span className="text-faint">from {languageName(from)}</span>
            )}
            <span className="ui-tab-count">{tab.waiting || tab.missing}</span>
          </button>
        ))}
      </nav>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-4xl flex-col gap-6 px-6 py-6">
          {failed && !run && (
            <ErrorMessage>The last run stopped: {failed.error || "it failed."}</ErrorMessage>
          )}
          <ErrorMessage>{error}</ErrorMessage>
          {proposals.length > 0 && (
            <section>
              <h2 className="mb-2 text-xs text-faint">
                To accept · {plural(proposals.length, "proposal")}
              </h2>
              <ul className="-mx-3 flex flex-col gap-0.5">
                {items.map(
                  (item, index) =>
                    item.kind === "suggestion" && (
                      <ProposalRow
                        key={item.id}
                        index={index}
                        suggestion={item.suggestion}
                        sourceText={item.suggestion.source_text}
                        reasons={concerns(item.suggestion, threshold(target))}
                        threshold={threshold(target)}
                        selected={current?.id === item.id}
                        skipped={skipped.has(item.id)}
                        onSelect={() => setSelected(index)}
                        onAccept={(value) => accept(item.suggestion, value)}
                        onReject={() => reject(item.suggestion)}
                      />
                    ),
                )}
              </ul>
            </section>
          )}
          {missingCount > 0 && (
            <section>
              <h2 className="mb-2 text-xs text-faint">Missing · {plural(missingCount, "key")}</h2>
              <ul className="-mx-3 flex flex-col gap-0.5">
                {items.map(
                  (item, index) =>
                    item.kind === "missing" && (
                      <MissingRow
                        key={item.id}
                        index={index}
                        entry={item.entry}
                        sourceText={textOf(item.entry, from ?? source)}
                        selected={current?.id === item.id}
                        skipped={skipped.has(item.id)}
                        writing={writing === item.id}
                        onSelect={() => setSelected(index)}
                        onWrite={() => setWriting(item.id)}
                        onSave={(value) => {
                          write(item.entry, value);
                          setWriting(null);
                        }}
                        onCancel={() => setWriting(null)}
                        onSuggest={() => void suggest([item.entry])}
                      />
                    ),
                )}
              </ul>
            </section>
          )}
          {items.length === 0 && <Empty title="Nothing matches" />}
          <p className="text-xs text-faint">
            <kbd className="ui-kbd">j</kbd> <kbd className="ui-kbd">k</kbd> move ·{" "}
            <kbd className="ui-kbd">↵</kbd> suggest or accept · <kbd className="ui-kbd">w</kbd>{" "}
            write · <kbd className="ui-kbd">x</kbd> reject · <kbd className="ui-kbd">s</kbd> skip ·{" "}
            <kbd className="ui-kbd">esc</kbd> back
          </p>
        </div>
      </div>
    </>
  );
}

const row = (selected: boolean, skipped: boolean) =>
  `rounded-md px-3 py-2 ${selected ? "bg-selected" : "hover:bg-hover"} ${skipped ? "opacity-60" : ""}`;

function KeyName({ namespace, name }: { namespace: string; name: string }) {
  return (
    <span className="truncate font-mono text-xs">
      <span className="text-faint">{namespace}.</span>
      {name}
    </span>
  );
}

function ProposalRow({
  index,
  suggestion,
  sourceText,
  reasons,
  threshold,
  selected,
  skipped,
  onSelect,
  onAccept,
  onReject,
}: {
  index: number;
  suggestion: Suggestion;
  sourceText: string;
  reasons: string[];
  threshold: number;
  selected: boolean;
  skipped: boolean;
  onSelect: () => void;
  onAccept: (value: string) => void;
  onReject: () => void;
}) {
  const [value, setValue] = useState(suggestion.value);
  const tone =
    suggestion.confidence < 0.5 ? "danger" : suggestion.confidence < threshold ? "warn" : "ok";
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: j and k select rows from the keyboard
    <li data-review-row={index} className={row(selected, skipped)} onClick={onSelect}>
      <div className="mb-1.5 flex items-center gap-2">
        <KeyName namespace={suggestion.namespace} name={suggestion.key} />
        <span className={`ui-status ui-status-${tone} px-2 py-0.5 text-xs tabular-nums`}>
          {Math.round(suggestion.confidence * 100)}%
        </span>
        <span className="ui-chip px-2 py-0.5 text-xs">by {suggestion.model}</span>
        <span className="ml-auto flex items-center gap-2">
          <Button variant="secondary" onClick={onReject}>
            Reject
          </Button>
          <Button onClick={() => onAccept(value)} disabled={!value.trim()}>
            Accept
          </Button>
        </span>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <p className="py-1.5 whitespace-pre-wrap text-muted">{sourceText}</p>
        <textarea
          aria-label={`Proposal for ${suggestion.key}`}
          value={value}
          rows={1}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onAccept(value);
            if (e.key === "Escape") e.currentTarget.blur();
          }}
          className="ui-field field-sizing-content min-h-8 w-full resize-none rounded-md px-2 py-1.5"
        />
      </div>
      {reasons.length > 0 && (
        <ul className="mt-1.5 flex flex-wrap gap-x-3 text-xs text-warn-ink">
          {reasons.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
      )}
    </li>
  );
}

function MissingRow({
  index,
  entry,
  sourceText,
  selected,
  skipped,
  writing,
  onSelect,
  onWrite,
  onSave,
  onCancel,
  onSuggest,
}: {
  index: number;
  entry: Entry;
  sourceText: string;
  selected: boolean;
  skipped: boolean;
  writing: boolean;
  onSelect: () => void;
  onWrite: () => void;
  onSave: (value: string) => void;
  onCancel: () => void;
  onSuggest: () => void;
}) {
  const [value, setValue] = useState("");
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: j and k select rows from the keyboard
    <li data-review-row={index} className={row(selected, skipped)} onClick={onSelect}>
      <div className="flex items-center gap-3">
        <span className="w-64 shrink-0 truncate">
          <KeyName namespace={entry.namespace} name={entry.key} />
        </span>
        <span className="min-w-0 flex-1 truncate text-muted">{sourceText}</span>
        {!writing && (
          <span className="flex shrink-0 items-center gap-2">
            <Button variant="secondary" onClick={onWrite}>
              Write
            </Button>
            <Button variant="secondary" onClick={onSuggest}>
              Suggest
            </Button>
          </span>
        )}
      </div>
      {writing && (
        <form
          className="mt-2 flex items-start gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            onSave(value);
          }}
        >
          <textarea
            // biome-ignore lint/a11y/noAutofocus: w opens it to type
            autoFocus
            aria-label={`Text for ${entry.key}`}
            value={value}
            rows={1}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onSave(value);
              if (e.key === "Escape") onCancel();
            }}
            className="ui-field field-sizing-content min-h-8 flex-1 resize-none rounded-md px-2 py-1.5"
          />
          <Button type="submit" disabled={!value.trim()}>
            Save
          </Button>
        </form>
      )}
    </li>
  );
}
