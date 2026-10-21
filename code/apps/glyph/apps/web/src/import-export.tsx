import { uuidv7 } from "@code-villain/sync-client";
import { useMemo, useState } from "react";
import { messageOf } from "./api";
import { transactInBatches } from "./bulk";
import {
  DEFAULT_NAMESPACE,
  languageName,
  plural,
  useCatalog,
  useLanguages,
  useSource,
} from "./data";
import { exportFile, exportMeta, type ParsedKey, parseFile, planImport } from "./import";
import { PageHeader } from "./layout";
import { Empty } from "./page-parts";
import { Choice } from "./select";
import { Button, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/** Saves text as a file in Downloads. */
function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** A file's name without `.json`, as a namespace, when it's a plausible one. */
function namespaceOf(fileName: string): string | undefined {
  const name = fileName.replace(/\.json$/i, "");
  return /^[A-Za-z0-9._-]+$/.test(name) ? name : undefined;
}

/** `/$workspace/import`: bring JSON files in, take them out. */
export function ImportExportPage() {
  const languages = useLanguages();
  if (languages.length === 0) {
    return (
      <>
        <PageHeader title="Import & export" />
        <Empty title="No languages yet">
          An admin adds the workspace's languages in Settings; then their files can come in.
        </Empty>
      </>
    );
  }
  return (
    <>
      <PageHeader title="Import & export" />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-12 px-6 py-8">
          <ImportSection />
          <ExportSection />
        </div>
      </div>
    </>
  );
}

function ImportSection() {
  const { sync } = useWorkspace();
  const languages = useLanguages();
  const source = useSource();
  const translations = useRecords("Translation");
  const [lang, setLang] = useState(source);
  const [fallback, setFallback] = useState(DEFAULT_NAMESPACE);
  const [files, setFiles] = useState<{ name: string; text: string }[]>([]);
  const [replace, setReplace] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  // One file per namespace (`common.json`) names its namespace; one file
  // alone uses the namespace chosen here.
  const parsed = useMemo(() => {
    if (files.length === 0) return null;
    try {
      const keys: ParsedKey[] = files.flatMap((f) =>
        parseFile(f.text, (files.length > 1 && namespaceOf(f.name)) || fallback),
      );
      return { keys, plan: planImport(keys, lang, translations, replace, uuidv7) };
    } catch (e) {
      return { error: messageOf(e) };
    }
  }, [files, fallback, lang, translations, replace]);

  const choose = async (list: FileList | null) => {
    setError(null);
    setDone(null);
    if (!list) return;
    // Copied first: the input empties its live list once it's reset.
    const picked = [...list];
    setFiles(await Promise.all(picked.map(async (f) => ({ name: f.name, text: await f.text() }))));
    const [only] = picked.length === 1 ? picked : [];
    if (only) setFallback(namespaceOf(only.name) ?? DEFAULT_NAMESPACE);
  };

  const run = () => {
    if (!parsed || !("plan" in parsed) || !parsed.plan) return;
    const { plan } = parsed;
    try {
      transactInBatches(sync, plan.operations);
      setDone(
        `Imported into ${languageName(lang)}: ${plan.added} new, ${plan.changed} changed${plan.kept ? `, ${plan.kept} kept as they were` : ""}.`,
      );
      setFiles([]);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const plan = parsed && "plan" in parsed ? parsed.plan : undefined;
  return (
    <section>
      <h2 className="mb-1 font-medium">Import</h2>
      <p className="mb-4 text-muted">
        i18next JSON files, flat or nested. Keys written <code>billing:invoice.due</code> go to
        their namespace; the others to the namespace below, or to the file's name when you pick
        several files.
      </p>
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <label className="ui-button cursor-pointer">
            {files.length ? plural(files.length, "file") : "Choose files"}
            <input
              type="file"
              accept=".json,application/json"
              multiple
              className="sr-only"
              onChange={(e) => {
                void choose(e.target.files);
                e.target.value = "";
              }}
            />
          </label>
          <span className="text-xs text-muted">into</span>
          <Choice
            label="Language"
            value={lang}
            options={languages.map((l) => ({ value: l.code, label: languageName(l.code) }))}
            onChange={setLang}
          />
          {files.length <= 1 && (
            <>
              <span className="text-xs text-muted">namespace</span>
              <input
                value={fallback}
                onChange={(e) => setFallback(e.target.value.trim() || DEFAULT_NAMESPACE)}
                aria-label="Namespace for keys without one"
                className="ui-field h-7 w-40 rounded-md px-2 font-mono text-xs"
              />
            </>
          )}
        </div>
        <label className="flex items-center gap-2 text-xs text-muted">
          <input type="checkbox" checked={replace} onChange={(e) => setReplace(e.target.checked)} />
          Replace texts that are already there
        </label>
        {parsed && "error" in parsed && <ErrorMessage>{parsed.error}</ErrorMessage>}
        {plan && (
          <div className="rounded-md bg-panel p-3">
            <p className="mb-2">
              {plural(plan.added, "new key")}, {plural(plan.changed, "change")}
              {plan.kept > 0 && `, ${plan.kept} kept`}
              {plan.same > 0 && <span className="text-muted"> · {plan.same} already the same</span>}
            </p>
            <ul className="mb-3 flex flex-wrap gap-1.5">
              {plan.namespaces.map((ns) => (
                <li key={ns.namespace} className="ui-chip px-2 py-0.5 font-mono text-xs">
                  {ns.namespace} <span className="text-faint">{ns.count}</span>
                </li>
              ))}
            </ul>
            <Button disabled={plan.operations.length === 0} onClick={run}>
              Import {plan.operations.length || ""}
            </Button>
          </div>
        )}
        <ErrorMessage>{error}</ErrorMessage>
        {done && <p className="text-ok-ink">{done}</p>}
      </div>
    </section>
  );
}

function ExportSection() {
  const languages = useLanguages();
  const catalog = useCatalog();
  const translations = useRecords("Translation");
  const [lang, setLang] = useState(languages[0]?.code ?? "");
  const ALL = "\u0000all";
  const [namespace, setNamespace] = useState(ALL);
  const ns = namespace === ALL ? undefined : namespace;

  return (
    <section>
      <h2 className="mb-1 font-medium">Export</h2>
      <p className="mb-4 text-muted">
        One namespace gives the file i18next loads (<code>sv/common.json</code>). Every namespace
        gives one file with <code>namespace:key</code> keys, which imports back as it was.
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <Choice
          label="Language"
          value={lang}
          options={languages.map((l) => ({ value: l.code, label: languageName(l.code) }))}
          onChange={setLang}
        />
        <Choice
          label="Namespace"
          value={namespace}
          options={[
            { value: ALL, label: "Every namespace" },
            ...[...catalog.namespaces.keys()].map((n) => ({ value: n, label: n })),
          ]}
          onChange={setNamespace}
        />
        <Button
          onClick={() =>
            download(ns ? `${lang}-${ns}.json` : `${lang}.json`, exportFile(translations, lang, ns))
          }
        >
          Download
        </Button>
        <Button
          variant="secondary"
          onClick={() => download("translations.meta.json", exportMeta(translations))}
        >
          Download meta
        </Button>
      </div>
    </section>
  );
}
