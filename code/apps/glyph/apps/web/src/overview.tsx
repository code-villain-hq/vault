import { Link } from "@tanstack/react-router";
import { useMemo } from "react";
import {
  type Coverage,
  coverage,
  languageName,
  plural,
  useCatalog,
  useLanguages,
  useSource,
  useThresholds,
} from "./data";
import { PageHeader } from "./layout";
import { Empty } from "./page-parts";
import { buttonClass } from "./ui";
import { useIsAdmin, useRecords, useWorkspace } from "./workspace";

/** `/$workspace`: how far each language has come, overall and per namespace. */
export function OverviewPage() {
  const { workspace } = useWorkspace();
  const isAdmin = useIsAdmin();
  const languages = useLanguages();
  const source = useSource();
  const catalog = useCatalog();
  const threshold = useThresholds(source);
  const suggestions = useRecords("Suggestion");
  const params = { workspace: workspace.url_key };

  const overall = useMemo(
    () => coverage(catalog, languages, source, threshold),
    [catalog, languages, source, threshold],
  );
  const namespaces = useMemo(
    () =>
      [...catalog.namespaces.keys()].map((ns) => ({
        ns,
        rows: coverage(catalog, languages, source, threshold, ns),
      })),
    [catalog, languages, source, threshold],
  );
  const waiting = useMemo(() => {
    const byLang = new Map<string, number>();
    for (const s of suggestions) byLang.set(s.lang, (byLang.get(s.lang) ?? 0) + 1);
    return byLang;
  }, [suggestions]);

  if (languages.length === 0) {
    return (
      <>
        <PageHeader title="Overview" />
        <Empty title="No languages yet">
          {isAdmin ? (
            <>
              Add the language keys are written in, then the ones to translate into.
              <div className="mt-4">
                <Link to="/$workspace/settings/languages" params={params} className={buttonClass()}>
                  Add languages
                </Link>
              </div>
            </>
          ) : (
            "An admin adds the workspace's languages in Settings."
          )}
        </Empty>
      </>
    );
  }

  const keys = [...catalog.entries.values()].length;
  return (
    <>
      <PageHeader
        title="Overview"
        meta={
          <span className="text-xs text-faint">
            {plural(keys, "key")} in {plural(catalog.namespaces.size, "namespace")} ·{" "}
            {languageName(source)} is the source
          </span>
        }
      />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-3xl flex-col gap-10 px-6 py-8">
          {keys === 0 ? (
            <Empty title="No keys yet">
              Import your JSON files, or add keys on the Translations page.
              <div className="mt-4 flex justify-center gap-2">
                <Link to="/$workspace/import" params={params} className={buttonClass()}>
                  Import
                </Link>
                <Link
                  to="/$workspace/translations"
                  params={params}
                  className={buttonClass("secondary")}
                >
                  Add a key
                </Link>
              </div>
            </Empty>
          ) : (
            <>
              <section aria-label="Languages">
                <ul className="-mx-3 flex flex-col gap-0.5">
                  {overall.map((row) => (
                    <CoverageRow key={row.lang} row={row} waiting={waiting.get(row.lang) ?? 0} />
                  ))}
                </ul>
              </section>
              {namespaces.length > 1 && (
                <section>
                  <h2 className="mb-2 font-medium">By namespace</h2>
                  <div className="flex flex-col gap-6">
                    {namespaces.map(({ ns, rows }) => (
                      <div key={ns}>
                        <h3 className="mb-1 font-mono text-xs text-muted">{ns}</h3>
                        <ul className="-mx-3 flex flex-col gap-0.5">
                          {rows.map((row) => (
                            <CoverageRow key={row.lang} row={row} namespace={ns} compact />
                          ))}
                        </ul>
                      </div>
                    ))}
                  </div>
                </section>
              )}
            </>
          )}
        </div>
      </div>
    </>
  );
}

function CoverageRow({
  row,
  waiting = 0,
  namespace,
  compact = false,
}: {
  row: Coverage;
  waiting?: number;
  namespace?: string;
  compact?: boolean;
}) {
  const { workspace } = useWorkspace();
  const percent = row.total === 0 ? 100 : Math.floor((row.translated / row.total) * 100);
  return (
    <li>
      <Link
        to="/$workspace/translations"
        params={{ workspace: workspace.url_key }}
        search={{ lang: row.lang, show: row.missing > 0 ? "missing" : undefined, ns: namespace }}
        className={`flex items-center gap-4 rounded-md px-3 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent ${compact ? "h-8" : "h-11"}`}
      >
        <span className={`w-40 shrink-0 truncate ${compact ? "text-muted" : ""}`}>
          {languageName(row.lang)}
        </span>
        <span
          className="h-1.5 min-w-16 flex-1 overflow-hidden rounded-full bg-chip"
          role="progressbar"
          aria-valuenow={percent}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={`${languageName(row.lang)}: ${percent}% translated`}
        >
          <span className="block h-full rounded-full bg-muted" style={{ width: `${percent}%` }} />
        </span>
        <span className="w-12 shrink-0 text-right text-xs tabular-nums text-muted">{percent}%</span>
        <span className="hidden w-56 shrink-0 gap-3 text-xs text-faint sm:flex">
          {row.missing > 0 ? <span>{row.missing} missing</span> : <span>Complete</span>}
          {row.unsure > 0 && <span className="text-warn-ink">{row.unsure} unsure</span>}
          {waiting > 0 && <span>{waiting} to review</span>}
        </span>
      </Link>
    </li>
  );
}
