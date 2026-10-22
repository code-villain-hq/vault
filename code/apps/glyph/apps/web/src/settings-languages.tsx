import { uuidv7 } from "@code-villain/sync-client";
import { type Confirmation, ConfirmDialog } from "@code-villain/web";
import type { Language, Operation } from "@glyph/types";
import { type FormEvent, useState } from "react";
import { DEFAULT_THRESHOLD, languageName, useLanguages, useSource } from "./data";
import { PageHeader } from "./layout";
import { MoreMenu } from "./page-parts";
import { useModels } from "./runs";
import { Choice, NumberField } from "./select";
import { Button, ErrorMessage } from "./ui";
import { useIsAdmin, useRecords, useWorkspace } from "./workspace";

/** `sv`, `pt-BR`, `zh-Hant`: a BCP 47 language tag, as i18next names folders. */
const CODE = /^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$/;

/** A pair from the source to the language, unless there is one. */
function pairFor(
  source: string,
  target: string,
  pairs: { source_lang: string; target_lang: string }[],
): Operation[] {
  if (source === target || pairs.some((p) => p.source_lang === source && p.target_lang === target))
    return [];
  return [
    {
      action: "create",
      model: "LanguagePair",
      id: uuidv7(),
      data: { source_lang: source, target_lang: target },
    },
  ];
}

export function LanguagesPage() {
  const { sync } = useWorkspace();
  const isAdmin = useIsAdmin();
  const languages = useLanguages();
  const source = useSource();
  const pairs = useRecords("LanguagePair");
  const { models } = useModels();
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const add = (event: FormEvent) => {
    event.preventDefault();
    const tag = code.trim().replace("_", "-");
    if (!CODE.test(tag)) {
      setError("Use a language code such as sv, de or pt-BR.");
      return;
    }
    if (languages.some((l) => l.code === tag)) {
      setError(`${languageName(tag)} is already here.`);
      return;
    }
    setError(null);
    const first = languages.length === 0;
    sync.transact([
      {
        action: "create",
        model: "Language",
        id: uuidv7(),
        data: { code: tag, is_source: first },
      },
      ...(first ? [] : pairFor(source, tag, pairs)),
    ]);
    setCode("");
  };

  const makeSource = (language: Language) => {
    const old = languages.find((l) => l.is_source);
    sync.transact([
      ...(old
        ? [{ action: "update", model: "Language", id: old.id, data: { is_source: false } }]
        : []),
      { action: "update", model: "Language", id: language.id, data: { is_source: true } },
      ...languages.flatMap((l) => pairFor(language.code, l.code, pairs)),
    ] as Operation[]);
  };

  const modelOptions = [
    { value: "", label: models?.[0] ? `${models[0].name} (first)` : "First model" },
    ...(models ?? []).map((m) => ({ value: m.name, label: m.name })),
  ];

  return (
    <>
      <PageHeader title="Languages" />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-10 px-6 py-8">
          <section>
            <h2 className="mb-1 font-medium">Languages</h2>
            <p className="mb-4 text-muted">
              Keys are written in the source language first, then translated into the others. Each
              language has its own model and how sure that model must be before its text needs no
              look.
            </p>
            {languages.length > 0 && (
              <ul className="-mx-3 mb-4 flex flex-col gap-0.5">
                {languages.map((language) => {
                  const pair = pairs.find(
                    (p) => p.source_lang === source && p.target_lang === language.code,
                  );
                  const update = (data: Record<string, unknown>) =>
                    pair && sync.update("LanguagePair", pair.id, data);
                  return (
                    <li
                      key={language.id}
                      className="flex min-h-11 items-center gap-3 rounded-md px-3 py-1.5 hover:bg-hover"
                    >
                      <div className="min-w-0 flex-1 truncate">
                        {languageName(language.code)}{" "}
                        <span className="text-xs text-faint">{language.code}</span>
                      </div>
                      {language.is_source ? (
                        <span className="ui-chip px-2 py-0.5 text-xs">Source</span>
                      ) : (
                        <div className="flex items-center gap-2">
                          <Choice
                            label={`Model for ${languageName(language.code)}`}
                            value={pair?.model ?? ""}
                            options={
                              pair?.model && !modelOptions.some((o) => o.value === pair.model)
                                ? [...modelOptions, { value: pair.model, label: pair.model }]
                                : modelOptions
                            }
                            disabled={!isAdmin || !pair}
                            onChange={(model) => update({ model })}
                          />
                          <NumberField
                            label="Temperature: lower is more literal"
                            value={pair?.temperature ?? 0.2}
                            min={0}
                            max={2}
                            step={0.1}
                            disabled={!isAdmin || !pair}
                            onChange={(temperature) => update({ temperature })}
                          />
                          <NumberField
                            label="Confidence threshold: less sure than this needs a look"
                            value={pair?.confidence_threshold ?? DEFAULT_THRESHOLD}
                            min={0}
                            max={1}
                            step={0.05}
                            disabled={!isAdmin || !pair}
                            onChange={(confidence_threshold) => update({ confidence_threshold })}
                          />
                        </div>
                      )}
                      {isAdmin ? (
                        <MoreMenu
                          label={`Actions for ${languageName(language.code)}`}
                          items={[
                            ...(language.is_source
                              ? []
                              : [{ label: "Make source", onSelect: () => makeSource(language) }]),
                            {
                              label: "Remove language",
                              danger: true,
                              onSelect: () =>
                                setConfirmation({
                                  title: `Remove ${languageName(language.code)}?`,
                                  body: "Its texts, suggestions and glossary go with it, for everyone.",
                                  action: "Remove language",
                                  onConfirm: () => sync.delete("Language", language.id),
                                }),
                            },
                          ]}
                        />
                      ) : null}
                    </li>
                  );
                })}
              </ul>
            )}
            {isAdmin ? (
              <form onSubmit={add} className="flex flex-col gap-2">
                <div className="flex items-center gap-2">
                  <input
                    value={code}
                    onChange={(e) => setCode(e.target.value)}
                    placeholder={
                      languages.length === 0 ? "Source language, such as en" : "sv, de, pt-BR"
                    }
                    aria-label="Language code"
                    className="ui-field h-8 w-56 rounded-md px-2"
                  />
                  <Button type="submit" variant="secondary" disabled={!code.trim()}>
                    Add language
                  </Button>
                  {CODE.test(code.trim()) && (
                    <span className="text-xs text-faint">{languageName(code.trim())}</span>
                  )}
                </div>
                <ErrorMessage>{error}</ErrorMessage>
              </form>
            ) : (
              <p className="text-xs text-faint">Admins add and remove languages.</p>
            )}
          </section>
        </div>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}
