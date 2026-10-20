import { Dialog } from "@base-ui/react/dialog";
import type { Translation } from "@glyph/types";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { ChevronDown, Copy, Search, Sparkles, Trash2, X } from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { messageOf } from "./api";
import { deleteKey, deleteNamespace, renameNamespace, transactInBatches } from "./bulk";
import { type Confirmation, ConfirmDialog } from "./confirm";
import {
  DEFAULT_NAMESPACE,
  type Entry,
  entryId,
  isMissing,
  isUnsure,
  languageName,
  plural,
  textOf,
  useCatalog,
  useLanguages,
  useSource,
  useThresholds,
} from "./data";
import { PageHeader } from "./layout";
import { CreateDialog, Empty, MoreMenu, NewButton, RowAction } from "./page-parts";
import { runProgress, useActiveRun, useRuns } from "./runs";
import { Choice } from "./select";
import { Button, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

export type Show = "all" | "missing" | "unsure";

export interface TranslationsSearch {
  lang?: string;
  show?: Show;
  ns?: string;
  q?: string;
}

/** Groups that are open, remembered on this device. */
const OPEN_KEY = "glyph:translations:open";
/** Rows shown in a group before "Show all". */
const PAGE = 200;

function readOpen(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(OPEN_KEY) ?? "[]") as string[]);
  } catch {
    return new Set();
  }
}

function saveOpen(open: Set<string>) {
  try {
    localStorage.setItem(OPEN_KEY, JSON.stringify([...open]));
  } catch {
    // Not remembered, then.
  }
}

/** Writes text to the clipboard, falling back for webviews without the API. */
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

/** `/$workspace/translations`: every key, by namespace, with its text in each language. */
export function TranslationsPage() {
  const search = useSearch({ strict: false }) as TranslationsSearch;
  const navigate = useNavigate();
  const { sync, workspace } = useWorkspace();
  const languages = useLanguages();
  const source = useSource();
  const catalog = useCatalog();
  const threshold = useThresholds(source);
  const translations = useRecords("Translation");
  const suggestions = useRecords("Suggestion");
  const runs = useRuns();

  const targets = languages.filter((l) => l.code !== source);
  const target = targets.find((l) => l.code === search.lang)?.code ?? targets[0]?.code ?? source;
  const show: Show = search.show ?? "all";
  const query = (search.q ?? "").trim().toLowerCase();
  const run = useActiveRun(source, target);

  const setSearch = (patch: Partial<TranslationsSearch>) =>
    void navigate({
      to: "/$workspace/translations",
      params: { workspace: workspace.url_key },
      search: { ...search, ...patch },
      replace: true,
    });

  const [open, setOpen] = useState(readOpen);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [creating, setCreating] = useState<"key" | "namespace" | null>(null);
  const [managing, setManaging] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // A link to one namespace opens it.
  useEffect(() => {
    if (search.ns) setOpen((o) => new Set(o).add(search.ns as string));
  }, [search.ns]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      if (event.key === "f") {
        event.preventDefault();
        searchRef.current?.focus();
      } else if (event.key === "n") {
        event.preventDefault();
        setCreating("key");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const unsure = (entry: Entry) => isUnsure(entry.values.get(target), threshold(target));
  const counts = useMemo(() => {
    let missing = 0;
    let low = 0;
    for (const entry of catalog.entries.values()) {
      if (isMissing(entry, target, source)) missing++;
      if (isUnsure(entry.values.get(target), threshold(target))) low++;
    }
    return { all: catalog.entries.size, missing, unsure: low };
  }, [catalog, target, source, threshold]);

  const groups = useMemo(() => {
    const matches = (entry: Entry) => {
      if (show === "missing" && !isMissing(entry, target, source)) return false;
      if (show === "unsure" && !isUnsure(entry.values.get(target), threshold(target))) return false;
      if (!query) return true;
      if (`${entry.namespace}.${entry.key}`.toLowerCase().includes(query)) return true;
      return [...entry.values.values()].some((t) => t.value.toLowerCase().includes(query));
    };
    return [...catalog.namespaces]
      .map(([ns, entries]) => ({ ns, all: entries, shown: entries.filter(matches) }))
      .filter((g) => g.shown.length > 0 || (show === "all" && !query));
  }, [catalog, show, query, target, source, threshold]);

  const toggle = (ns: string) => {
    const next = new Set(open);
    if (next.has(ns)) next.delete(ns);
    else next.add(ns);
    setOpen(next);
    saveOpen(next);
  };

  const fill = async () => {
    setError(null);
    try {
      await runs.start({ kind: "fill", source_lang: source, target_lang: target });
    } catch (e) {
      setError(messageOf(e));
    }
  };

  if (languages.length === 0) {
    return (
      <>
        <PageHeader title="Translations" />
        <Empty title="No languages yet">An admin adds the workspace's languages in Settings.</Empty>
      </>
    );
  }

  const segments: { value: Show; label: string; count: number }[] = [
    { value: "all", label: "All", count: counts.all },
    { value: "missing", label: "Missing", count: counts.missing },
    { value: "unsure", label: "Low confidence", count: counts.unsure },
  ];

  return (
    <>
      <PageHeader title="Translations">
        <label className="relative flex items-center">
          <Search
            size={15}
            strokeWidth={1.75}
            aria-hidden="true"
            className="pointer-events-none absolute left-2 text-faint"
          />
          <input
            ref={searchRef}
            type="search"
            value={search.q ?? ""}
            onChange={(e) => setSearch({ q: e.target.value || undefined })}
            placeholder="Search keys and text"
            aria-label="Search keys and text (⌘F)"
            className="ui-field h-7 w-52 rounded-md pr-2 pl-7 text-xs"
          />
        </label>
        <fieldset className="ui-segmented flex shrink-0" aria-label="Show">
          {segments.map((s) => (
            <button
              key={s.value}
              type="button"
              aria-pressed={show === s.value}
              onClick={() => setSearch({ show: s.value === "all" ? undefined : s.value })}
              className="ui-segment flex items-center gap-1.5 px-2 text-xs"
            >
              {s.label}
              <span className="tabular-nums text-faint">{s.count}</span>
            </button>
          ))}
        </fieldset>
        {targets.length > 0 && (
          <Choice
            label="Language"
            value={target}
            options={targets.map((l) => ({ value: l.code, label: languageName(l.code) }))}
            onChange={(lang) => setSearch({ lang })}
          />
        )}
        <MoreMenu
          label="Namespace actions"
          items={[
            { label: "New namespace…", onSelect: () => setCreating("namespace") },
            { label: "Manage namespaces…", onSelect: () => setManaging(true) },
          ]}
        />
        <NewButton label="New key (⌘N)" onClick={() => setCreating("key")} />
        {run ? (
          <span className="flex shrink-0 items-center gap-2 text-xs text-muted">
            <span className="ui-live-dot" aria-hidden="true" />
            {runProgress(run)}
            <Button variant="secondary" onClick={() => void runs.cancel(run.id)}>
              Stop
            </Button>
          </span>
        ) : (
          target !== source &&
          counts.missing > 0 && (
            <Button className="shrink-0" onClick={() => void fill()}>
              Fill {counts.missing} missing
            </Button>
          )
        )}
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="px-4 py-3">
          <ErrorMessage>{error}</ErrorMessage>
        </div>
        {groups.length === 0 ? (
          <Empty title={query ? "Nothing matches" : "Nothing here"}>
            {query
              ? "No key or text has that in it."
              : show === "missing"
                ? `Every key has a ${languageName(target)} text.`
                : show === "unsure"
                  ? "No translation needs a second look."
                  : "Add a key, or import your JSON files."}
          </Empty>
        ) : (
          <div className="flex flex-col pb-10">
            {groups.map((group) => (
              <NamespaceGroup
                key={group.ns}
                ns={group.ns}
                all={group.all}
                shown={group.shown}
                open={open.has(group.ns) || Boolean(query)}
                onToggle={() => toggle(group.ns)}
                languages={languages.map((l) => l.code)}
                source={source}
                target={target}
                threshold={threshold}
                unsure={unsure}
                openKey={openKey}
                onOpenKey={setOpenKey}
                onDelete={(entry) =>
                  setConfirmation({
                    title: `Delete “${entry.key}”?`,
                    body: "It's deleted in every language, for everyone.",
                    action: "Delete key",
                    onConfirm: () =>
                      transactInBatches(
                        sync,
                        deleteKey(translations, suggestions, entry.namespace, entry.key),
                      ),
                  })
                }
              />
            ))}
          </div>
        )}
      </div>
      <CreateDialog open={creating !== null} onClose={() => setCreating(null)}>
        <NewKeyForm
          mode={creating ?? "key"}
          namespaces={[...catalog.namespaces.keys()]}
          defaultNamespace={search.ns}
          exists={(ns, key) => catalog.entries.has(entryId(ns, key))}
          onCreate={(ns, key, value, context) => {
            sync.create("Translation", { lang: source, namespace: ns, key, value, context });
            const next = new Set(open).add(ns);
            setOpen(next);
            saveOpen(next);
            setOpenKey(entryId(ns, key));
            setCreating(null);
          }}
        />
      </CreateDialog>
      <CreateDialog open={managing} onClose={() => setManaging(false)}>
        <ManageNamespaces
          onConfirm={setConfirmation}
          onRename={(from, to) =>
            transactInBatches(sync, renameNamespace(translations, suggestions, from, to))
          }
          onDelete={(ns) => transactInBatches(sync, deleteNamespace(translations, suggestions, ns))}
        />
      </CreateDialog>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

function NamespaceGroup({
  ns,
  all,
  shown,
  open,
  onToggle,
  languages,
  source,
  target,
  threshold,
  unsure,
  openKey,
  onOpenKey,
  onDelete,
}: {
  ns: string;
  all: Entry[];
  shown: Entry[];
  open: boolean;
  onToggle: () => void;
  languages: string[];
  source: string;
  target: string;
  threshold: (target: string) => number;
  unsure: (entry: Entry) => boolean;
  openKey: string | null;
  onOpenKey: (id: string | null) => void;
  onDelete: (entry: Entry) => void;
}) {
  const [limit, setLimit] = useState(PAGE);
  const missing = all.filter((e) => isMissing(e, target, source)).length;
  const low = all.filter(unsure).length;
  return (
    <section aria-label={ns}>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="sticky top-0 z-10 flex h-9 w-full items-center gap-2 bg-bg px-4 text-left outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
      >
        <ChevronDown
          size={15}
          strokeWidth={1.75}
          aria-hidden="true"
          className={`shrink-0 text-faint transition-transform ${open ? "" : "-rotate-90"}`}
        />
        <span className="font-mono text-xs font-medium">{ns}</span>
        <span className="text-xs text-faint">{plural(all.length, "key")}</span>
        <span className="ml-auto flex items-center gap-3 text-xs text-faint">
          {missing > 0 && <span>{missing} missing</span>}
          {low > 0 && <span className="text-warn-ink">{low} unsure</span>}
        </span>
      </button>
      {open && (
        <ul className="flex flex-col">
          {shown.slice(0, limit).map((entry) => {
            const id = entryId(entry.namespace, entry.key);
            return (
              <KeyRow
                key={id}
                entry={entry}
                languages={languages}
                source={source}
                threshold={threshold}
                open={openKey === id}
                onToggle={() => onOpenKey(openKey === id ? null : id)}
                onDelete={() => onDelete(entry)}
              />
            );
          })}
          {shown.length > limit && (
            <li className="px-4 py-2">
              <button
                type="button"
                className="ui-link text-xs"
                onClick={() => setLimit(shown.length)}
              >
                Show all {shown.length}
              </button>
            </li>
          )}
        </ul>
      )}
    </section>
  );
}

/** A language's state for a key, as a dot: done, missing (a ring) or unsure. */
function StateDot({
  entry,
  lang,
  source,
  threshold,
}: {
  entry: Entry;
  lang: string;
  source: string;
  threshold: number;
}) {
  const text = textOf(entry, lang).trim();
  const state = !text
    ? isMissing(entry, lang, source)
      ? "missing"
      : "empty"
    : isUnsure(entry.values.get(lang), threshold)
      ? "unsure"
      : "done";
  const label = `${languageName(lang)}: ${
    { missing: "missing", empty: "no text", unsure: "low confidence", done: "translated" }[state]
  }`;
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={`size-2 shrink-0 rounded-full ${
        state === "done"
          ? "bg-ok"
          : state === "unsure"
            ? "bg-warn"
            : "shadow-[inset_0_0_0_1.5px_var(--faint)]"
      }`}
    />
  );
}

function KeyRow({
  entry,
  languages,
  source,
  threshold,
  open,
  onToggle,
  onDelete,
}: {
  entry: Entry;
  languages: string[];
  source: string;
  threshold: (target: string) => number;
  open: boolean;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <li className={open ? "bg-panel" : ""}>
      <div className="group flex h-9 items-center gap-3 px-4 hover:bg-hover">
        <button
          type="button"
          aria-expanded={open}
          onClick={onToggle}
          className="flex min-w-0 flex-1 items-center gap-3 self-stretch text-left outline-none focus-visible:ring-2 focus-visible:ring-accent"
        >
          <span className="w-64 shrink-0 truncate pl-6 font-mono text-xs">{entry.key}</span>
          <span className="min-w-0 flex-1 truncate text-muted">{textOf(entry, source)}</span>
        </button>
        <span
          className={`flex items-center gap-1 ${open ? "" : "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"}`}
        >
          <RowAction
            label={copied ? "Copied" : "Copy key"}
            onClick={() => {
              void copyText(entry.key).then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1200);
              });
            }}
          >
            <Copy size={15} strokeWidth={1.75} aria-hidden="true" />
          </RowAction>
          <RowAction label="Delete key" onClick={onDelete}>
            <Trash2 size={15} strokeWidth={1.75} aria-hidden="true" />
          </RowAction>
        </span>
        <span className="flex shrink-0 items-center gap-1.5">
          {languages.map((lang) => (
            <StateDot
              key={lang}
              entry={entry}
              lang={lang}
              source={source}
              threshold={threshold(lang)}
            />
          ))}
        </span>
      </div>
      {open && (
        <KeyEditor entry={entry} languages={languages} source={source} threshold={threshold} />
      )}
    </li>
  );
}

/** An open key: the note for translators, then its text in each language. */
function KeyEditor({
  entry,
  languages,
  source,
  threshold,
}: {
  entry: Entry;
  languages: string[];
  source: string;
  threshold: (target: string) => number;
}) {
  const { sync } = useWorkspace();
  const runs = useRuns();
  const [error, setError] = useState<string | null>(null);
  const sourceRecord = entry.values.get(source);

  const save = (lang: string, value: string) => {
    const existing = entry.values.get(lang);
    if (existing) {
      if (existing.value === value) return;
      sync.update("Translation", existing.id, { value, confidence: null, translated_by: "human" });
    } else if (value.trim()) {
      sync.create("Translation", { lang, namespace: entry.namespace, key: entry.key, value });
    }
  };

  const saveContext = (context: string) => {
    if (sourceRecord) {
      if (sourceRecord.context !== context)
        sync.update("Translation", sourceRecord.id, { context });
    } else if (context.trim()) {
      sync.create("Translation", {
        lang: source,
        namespace: entry.namespace,
        key: entry.key,
        value: "",
        context,
      });
    }
  };

  const translate = async (from: string, to: string) => {
    setError(null);
    try {
      await runs.start({
        kind: "fill",
        source_lang: from,
        target_lang: to,
        keys: [{ namespace: entry.namespace, key: entry.key }],
      });
    } catch (e) {
      setError(messageOf(e));
    }
  };

  // Back-fills the source from the first language that has text.
  const backFrom = languages.find((l) => l !== source && textOf(entry, l).trim());

  return (
    <div className="flex flex-col gap-3 px-4 pt-1 pb-4 pl-10">
      <ErrorMessage>{error}</ErrorMessage>
      <AutoText
        label="Note for translators"
        placeholder="Where this text appears, for translators and the model"
        value={sourceRecord?.context ?? ""}
        onSave={saveContext}
        quiet
      />
      {languages.map((lang) => {
        const record = entry.values.get(lang);
        const isSource = lang === source;
        return (
          <LanguageText
            key={lang}
            lang={lang}
            record={record}
            threshold={threshold(lang)}
            isSource={isSource}
            onSave={(value) => save(lang, value)}
            actions={
              isSource ? (
                <>
                  {!textOf(entry, lang).trim() && backFrom && (
                    <RowAction
                      label={`Translate from ${languageName(backFrom)}`}
                      onClick={() => void translate(backFrom, source)}
                    >
                      <Sparkles size={15} strokeWidth={1.75} aria-hidden="true" />
                    </RowAction>
                  )}
                  <RowAction
                    label="Copy text"
                    disabled={!record?.value}
                    onClick={() => void copyText(record?.value ?? "")}
                  >
                    <Copy size={15} strokeWidth={1.75} aria-hidden="true" />
                  </RowAction>
                </>
              ) : (
                <>
                  <RowAction
                    label={`Translate into ${languageName(lang)}`}
                    disabled={!textOf(entry, source).trim()}
                    onClick={() => void translate(source, lang)}
                  >
                    <Sparkles size={15} strokeWidth={1.75} aria-hidden="true" />
                  </RowAction>
                  <RowAction
                    label="Clear"
                    disabled={!record}
                    onClick={() => record && sync.delete("Translation", record.id)}
                  >
                    <X size={15} strokeWidth={1.75} aria-hidden="true" />
                  </RowAction>
                </>
              )
            }
          />
        );
      })}
    </div>
  );
}

function LanguageText({
  lang,
  record,
  threshold,
  isSource,
  onSave,
  actions,
}: {
  lang: string;
  record: Translation | undefined;
  threshold: number;
  isSource: boolean;
  onSave: (value: string) => void;
  actions: ReactNode;
}) {
  const confidence = record?.value.trim() ? record.confidence : null;
  const tone =
    confidence === null
      ? null
      : confidence < 0.5
        ? "danger"
        : confidence < threshold
          ? "warn"
          : "ok";
  return (
    <div className="grid grid-cols-[9rem_1fr_auto] items-start gap-3">
      <div className="pt-1.5 text-xs text-muted">
        {languageName(lang)}
        {isSource && <span className="text-faint"> · source</span>}
      </div>
      <AutoText label={`${languageName(lang)} text`} value={record?.value ?? ""} onSave={onSave} />
      <div className="flex items-center gap-1.5 pt-0.5">
        {tone && confidence !== null && (
          <span
            className={`ui-status ui-status-${tone} px-2 py-0.5 text-xs tabular-nums`}
            title="How sure the model was"
          >
            {Math.round(confidence * 100)}%
          </span>
        )}
        {record?.value.trim() && record.translated_by !== "human" && (
          <span className="ui-chip px-2 py-0.5 text-xs">by {record.translated_by}</span>
        )}
        {actions}
      </div>
    </div>
  );
}

/** A textarea that grows with its text and saves when you leave it; ⌘↵ saves too. */
function AutoText({
  label,
  value,
  placeholder,
  onSave,
  quiet = false,
}: {
  label: string;
  value: string;
  placeholder?: string;
  onSave: (value: string) => void;
  quiet?: boolean;
}) {
  const [draft, setDraft] = useState(value);
  const [focused, setFocused] = useState(false);
  // Someone else's change arrives while you're not typing here.
  useEffect(() => {
    if (!focused) setDraft(value);
  }, [value, focused]);
  return (
    <textarea
      aria-label={label}
      value={draft}
      placeholder={placeholder}
      rows={1}
      onChange={(e) => setDraft(e.target.value)}
      onFocus={() => setFocused(true)}
      onBlur={() => {
        setFocused(false);
        if (draft !== value) onSave(draft);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) e.currentTarget.blur();
        if (e.key === "Escape") {
          setDraft(value);
          requestAnimationFrame(() => e.currentTarget?.blur());
        }
      }}
      className={`ui-field field-sizing-content min-h-8 w-full resize-none rounded-md px-2 py-1.5 ${quiet ? "text-xs text-muted" : ""}`}
    />
  );
}

/** A namespace or a key name: no spaces, and no `:` (locize's namespace mark). */
const NAME = /^[^\s:]+$/;

function NewKeyForm({
  mode,
  namespaces,
  defaultNamespace,
  exists,
  onCreate,
}: {
  mode: "key" | "namespace";
  namespaces: string[];
  defaultNamespace?: string;
  exists: (ns: string, key: string) => boolean;
  onCreate: (ns: string, key: string, value: string, context: string) => void;
}) {
  const NEW = "\u0000new";
  const initial =
    mode === "namespace"
      ? NEW
      : defaultNamespace && namespaces.includes(defaultNamespace)
        ? defaultNamespace
        : (namespaces[0] ?? NEW);
  const [ns, setNs] = useState(initial);
  const [newNs, setNewNs] = useState(namespaces.length === 0 ? DEFAULT_NAMESPACE : "");
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [context, setContext] = useState("");
  const [error, setError] = useState<string | null>(null);
  const namespace = ns === NEW ? newNs.trim() : ns;

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const name = key.trim();
    if (!NAME.test(namespace)) return setError("A namespace needs a name without spaces or “:”.");
    if (!NAME.test(name)) return setError("A key needs a name without spaces or “:”.");
    if (exists(namespace, name)) return setError(`“${name}” is already in ${namespace}.`);
    onCreate(namespace, name, value, context);
  };

  return (
    <form
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          submit();
        }
      }}
    >
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>Translations</span>
        <span>›</span>
        <Dialog.Title className="font-normal">
          {mode === "namespace" ? "New namespace" : "New key"}
        </Dialog.Title>
      </div>
      <div className="flex flex-col gap-3 px-4 py-3">
        <div className="flex items-center gap-2">
          {namespaces.length > 0 && (
            <Choice
              label="Namespace"
              value={ns}
              options={[
                ...namespaces.map((n) => ({ value: n, label: n })),
                { value: NEW, label: "New namespace…" },
              ]}
              onChange={setNs}
            />
          )}
          {ns === NEW && (
            <input
              // biome-ignore lint/a11y/noAutofocus: the dialog opens to name the namespace
              autoFocus={mode === "namespace"}
              value={newNs}
              onChange={(e) => setNewNs(e.target.value)}
              placeholder="Namespace, such as billing"
              aria-label="New namespace"
              className="ui-field h-7 w-56 rounded-md px-2 font-mono text-xs"
            />
          )}
        </div>
        <input
          // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a key
          autoFocus={mode === "key"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="Key, such as login.submit"
          aria-label="Key"
          className="bg-transparent font-mono text-title font-bold outline-none placeholder:text-faint"
        />
        <textarea
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="Text in the source language"
          aria-label="Source text"
          rows={2}
          className="resize-none bg-transparent outline-none placeholder:text-faint"
        />
        <input
          value={context}
          onChange={(e) => setContext(e.target.value)}
          placeholder="Note for translators (optional)"
          aria-label="Note for translators"
          className="bg-transparent text-xs text-muted outline-none placeholder:text-faint"
        />
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-3 px-4 py-3">
        <span className="text-xs text-faint">⌘↵ to create</span>
        <Button type="submit" disabled={!key.trim() || !namespace}>
          Create key
        </Button>
      </div>
    </form>
  );
}

function ManageNamespaces({
  onRename,
  onDelete,
  onConfirm,
}: {
  onRename: (from: string, to: string) => void;
  onDelete: (ns: string) => void;
  onConfirm: (confirmation: Confirmation) => void;
}) {
  const catalog = useCatalog();
  const translations = useRecords("Translation");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const texts = useMemo(() => {
    const count = new Map<string, number>();
    for (const t of translations) {
      if (t.value.trim()) count.set(t.namespace, (count.get(t.namespace) ?? 0) + 1);
    }
    return count;
  }, [translations]);

  const rename = (from: string) => {
    const to = name.trim();
    if (to === from) return setRenaming(null);
    if (!NAME.test(to)) return setError("A namespace needs a name without spaces or “:”.");
    try {
      onRename(from, to);
      setRenaming(null);
      setError(null);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <div>
      <div className="px-4 pt-4 text-xs text-muted">
        <Dialog.Title className="font-normal">Namespaces</Dialog.Title>
      </div>
      <div className="flex flex-col gap-2 px-4 py-3">
        <ErrorMessage>{error}</ErrorMessage>
        {catalog.namespaces.size === 0 && <p className="text-muted">No namespaces yet.</p>}
        <ul className="-mx-2 flex max-h-[60vh] flex-col gap-0.5 overflow-y-auto">
          {[...catalog.namespaces].map(([ns, entries]) => (
            <li key={ns} className="flex h-9 items-center gap-3 rounded-md px-2 hover:bg-hover">
              {renaming === ns ? (
                <form
                  className="flex flex-1 items-center gap-2"
                  onSubmit={(e) => {
                    e.preventDefault();
                    rename(ns);
                  }}
                >
                  <input
                    // biome-ignore lint/a11y/noAutofocus: renaming starts by typing
                    autoFocus
                    onFocus={(e) => e.target.select()}
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    onKeyDown={(e) => e.key === "Escape" && setRenaming(null)}
                    aria-label={`New name for ${ns}`}
                    className="ui-field h-7 flex-1 rounded-md px-2 font-mono text-xs"
                  />
                  <Button type="submit">Rename</Button>
                </form>
              ) : (
                <>
                  <span className="flex-1 truncate font-mono text-xs">{ns}</span>
                  <span className="text-xs text-faint">
                    {plural(entries.length, "key")} · {plural(texts.get(ns) ?? 0, "text")}
                  </span>
                  <button
                    type="button"
                    className="h-7 rounded-md px-2 text-xs text-muted hover:bg-hover hover:text-fg"
                    onClick={() => {
                      setRenaming(ns);
                      setName(ns);
                      setError(null);
                    }}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    className="h-7 rounded-md px-2 text-xs text-danger-ink hover:bg-hover"
                    onClick={() =>
                      onConfirm({
                        title: `Delete the namespace “${ns}”?`,
                        body: `Its ${plural(entries.length, "key")} are deleted in every language, for everyone.`,
                        action: "Delete namespace",
                        onConfirm: () => onDelete(ns),
                      })
                    }
                  >
                    Delete
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
