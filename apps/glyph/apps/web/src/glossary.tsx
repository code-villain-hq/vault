import type { GlossaryEntry } from "@glyph/types";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Trash2, X } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { messageOf } from "./api";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { languageName, plural, useLanguages, useSource } from "./data";
import { PageHeader } from "./layout";
import { Empty, RowAction } from "./page-parts";
import { useRuns } from "./runs";
import { Choice } from "./select";
import { Button, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

export interface GlossarySearch {
  target?: string;
}

/** Adds forms not already there, ignoring case; running it twice changes nothing. */
export function mergeVariants(current: string[], found: string[]): string[] {
  const seen = new Set(current.map((v) => v.toLowerCase()));
  const merged = [...current];
  for (const variant of found.map((v) => v.trim()).filter(Boolean)) {
    if (!seen.has(variant.toLowerCase())) {
      seen.add(variant.toLowerCase());
      merged.push(variant);
    }
  }
  return merged;
}

/** `/$workspace/glossary`: the terms each language must use, which every model call is told. */
export function GlossaryPage() {
  const search = useSearch({ strict: false }) as GlossarySearch;
  const navigate = useNavigate();
  const { sync, workspace } = useWorkspace();
  const languages = useLanguages();
  const source = useSource();
  const all = useRecords("GlossaryEntry");
  const [filter, setFilter] = useState("");
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const targets = languages.filter((l) => l.code !== source);
  const target = targets.find((l) => l.code === search.target)?.code ?? targets[0]?.code;
  const entries = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return all
      .filter((e) => e.source_lang === source && e.target_lang === target)
      .filter(
        (e) =>
          !q ||
          [e.source_term, e.target_term, e.note, ...e.target_variants].some((t) =>
            t.toLowerCase().includes(q),
          ),
      )
      .sort((a, b) => a.source_term.localeCompare(b.source_term));
  }, [all, source, target, filter]);

  if (!target) {
    return (
      <>
        <PageHeader title="Glossary" />
        <Empty title="No languages to translate into">
          An admin adds the workspace's languages in Settings.
        </Empty>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Glossary"
        meta={<span className="text-xs text-faint">{plural(entries.length, "term")}</span>}
      >
        <input
          type="search"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter"
          aria-label="Filter terms"
          className="ui-field h-7 w-44 rounded-md px-2 text-xs"
        />
        <Choice
          label="Language"
          value={target}
          options={targets.map((l) => ({
            value: l.code,
            label: `${languageName(source)} → ${languageName(l.code)}`,
          }))}
          onChange={(lang) =>
            void navigate({
              to: "/$workspace/glossary",
              params: { workspace: workspace.url_key },
              search: { target: lang },
              replace: true,
            })
          }
        />
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-3xl flex-col gap-6 px-6 py-8">
          <p className="text-muted">
            Preferred terms for {languageName(target)}. When a text has a term, the model is told to
            use its translation, and proposals that don't are flagged in Review. Other accepted
            forms, such as a definite form, count as the term. Notes are for reviewers only.
          </p>
          <TermForm
            key={target}
            source={source}
            target={target}
            submitLabel="Add term"
            onSave={(fields) => {
              const taken = all.some(
                (e) =>
                  e.source_lang === source &&
                  e.target_lang === target &&
                  e.source_term.toLowerCase() === fields.source_term.toLowerCase(),
              );
              if (taken) throw new Error(`“${fields.source_term}” already has a term.`);
              sync.create("GlossaryEntry", { source_lang: source, target_lang: target, ...fields });
            }}
          />
          {entries.length > 0 && (
            <ul className="-mx-3 flex flex-col gap-0.5">
              {entries.map((entry) => (
                <TermRow
                  key={entry.id}
                  entry={entry}
                  onDelete={() =>
                    setConfirmation({
                      title: `Delete the term “${entry.source_term}”?`,
                      body: "Models stop being told to use it.",
                      action: "Delete term",
                      onConfirm: () => sync.delete("GlossaryEntry", entry.id),
                    })
                  }
                />
              ))}
            </ul>
          )}
        </div>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

interface TermFields {
  source_term: string;
  target_term: string;
  target_variants: string[];
  note: string;
}

function TermRow({ entry, onDelete }: { entry: GlossaryEntry; onDelete: () => void }) {
  const { sync } = useWorkspace();
  const [editing, setEditing] = useState(false);
  if (editing) {
    return (
      <li className="rounded-md bg-panel px-3 py-3">
        <TermForm
          source={entry.source_lang}
          target={entry.target_lang}
          initial={entry}
          submitLabel="Save"
          onCancel={() => setEditing(false)}
          onSave={(fields) => {
            sync.update("GlossaryEntry", entry.id, fields);
            setEditing(false);
          }}
        />
      </li>
    );
  }
  return (
    <li className="group flex items-start gap-3 rounded-md px-3 py-2 hover:bg-hover">
      <button
        type="button"
        onClick={() => setEditing(true)}
        className="grid min-w-0 flex-1 grid-cols-2 gap-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-accent"
      >
        <span className="truncate">{entry.source_term}</span>
        <span className="flex min-w-0 flex-col gap-1">
          <span className="truncate font-medium">{entry.target_term}</span>
          {entry.target_variants.length > 0 && (
            <span className="truncate text-xs text-muted">
              also {entry.target_variants.join(", ")}
            </span>
          )}
          {entry.note && <span className="truncate text-xs text-faint">{entry.note}</span>}
        </span>
      </button>
      <span className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
        <RowAction label="Delete term" onClick={onDelete}>
          <Trash2 size={15} strokeWidth={1.75} aria-hidden="true" />
        </RowAction>
      </span>
    </li>
  );
}

function TermForm({
  source,
  target,
  initial,
  submitLabel,
  onSave,
  onCancel,
}: {
  source: string;
  target: string;
  initial?: TermFields;
  submitLabel: string;
  onSave: (fields: TermFields) => void;
  onCancel?: () => void;
}) {
  const runs = useRuns();
  const [sourceTerm, setSourceTerm] = useState(initial?.source_term ?? "");
  const [targetTerm, setTargetTerm] = useState(initial?.target_term ?? "");
  const [variants, setVariants] = useState<string[]>(initial?.target_variants ?? []);
  const [variant, setVariant] = useState("");
  const [note, setNote] = useState(initial?.note ?? "");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [finding, setFinding] = useState(false);

  const addVariant = () => {
    if (variant.trim()) setVariants(mergeVariants(variants, [variant]));
    setVariant("");
  };

  const find = async () => {
    setFinding(true);
    setError(null);
    setInfo(null);
    try {
      const found = await runs.variants({
        source_lang: source,
        target_lang: target,
        source_term: sourceTerm.trim(),
        target_term: targetTerm.trim(),
      });
      const merged = mergeVariants(variants, found.variants);
      if (merged.length === variants.length) setInfo("The model found no other forms.");
      setVariants(merged);
    } catch (e) {
      setError(messageOf(e));
    }
    setFinding(false);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    try {
      onSave({
        source_term: sourceTerm.trim(),
        target_term: targetTerm.trim(),
        target_variants: mergeVariants(variants, [variant]),
        note: note.trim(),
      });
      if (!initial) {
        setSourceTerm("");
        setTargetTerm("");
        setVariants([]);
        setNote("");
      }
      setVariant("");
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-2">
      <div className="grid grid-cols-2 gap-3">
        <input
          value={sourceTerm}
          onChange={(e) => setSourceTerm(e.target.value)}
          placeholder={`${languageName(source)} term`}
          aria-label={`${languageName(source)} term`}
          className="ui-field h-8 rounded-md px-2"
        />
        <input
          value={targetTerm}
          onChange={(e) => setTargetTerm(e.target.value)}
          placeholder={`${languageName(target)} term`}
          aria-label={`${languageName(target)} term`}
          className="ui-field h-8 rounded-md px-2"
        />
      </div>
      {(sourceTerm.trim() || initial) && (
        <>
          <div className="flex flex-wrap items-center gap-1.5">
            {variants.map((v) => (
              <span key={v} className="ui-chip flex h-6 items-center gap-1 pr-1 pl-2 text-xs">
                {v}
                <button
                  type="button"
                  aria-label={`Remove ${v}`}
                  onClick={() => setVariants(variants.filter((x) => x !== v))}
                  className="rounded-full p-0.5 hover:bg-hover"
                >
                  <X size={12} strokeWidth={1.75} aria-hidden="true" />
                </button>
              </span>
            ))}
            <input
              value={variant}
              onChange={(e) => setVariant(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === ",") {
                  e.preventDefault();
                  addVariant();
                }
              }}
              placeholder="Other accepted forms"
              aria-label="Other accepted forms"
              className="h-6 min-w-40 flex-1 bg-transparent text-xs outline-none placeholder:text-faint"
            />
            <Button
              variant="secondary"
              disabled={finding || !sourceTerm.trim() || !targetTerm.trim()}
              onClick={() => void find()}
            >
              {finding ? "Finding…" : "Find variants"}
            </Button>
          </div>
          <input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="Note for reviewers (optional)"
            aria-label="Note for reviewers"
            className="ui-field h-8 rounded-md px-2 text-xs"
          />
        </>
      )}
      {info && <p className="text-xs text-muted">{info}</p>}
      <ErrorMessage>{error}</ErrorMessage>
      <div className="flex items-center gap-2">
        <Button type="submit" disabled={!sourceTerm.trim() || !targetTerm.trim()}>
          {submitLabel}
        </Button>
        {onCancel && (
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
        )}
      </div>
    </form>
  );
}
