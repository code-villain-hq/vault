// Translating with the server's models: runs that suggest or fill in keys,
// and the models themselves.
import type {
  ModelInfo,
  StartRunRequest,
  TranslationRun,
  Variants,
  VariantsRequest,
} from "@glyph/types";
import { useCallback, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { useRecords, useWorkspace } from "./workspace";

/** How many keys one Suggest run proposes at most. */
export const SUGGEST_LIMIT = 50;

export function useRuns() {
  const { workspace } = useWorkspace();
  const base = `/api/w/${workspace.id}`;
  return {
    start: (request: StartRunRequest) => api<TranslationRun>("POST", `${base}/runs`, request),
    cancel: (id: string) => api<TranslationRun>("POST", `${base}/runs/${id}/cancel`),
    variants: (request: VariantsRequest) =>
      api<Variants>("POST", `${base}/glossary/variants`, request),
  };
}

const going = (run: TranslationRun) => run.status === "queued" || run.status === "running";

/** The run working on a direction now, if any; the latest first. */
export function useActiveRun(source: string, target: string): TranslationRun | undefined {
  return useRecords("TranslationRun")
    .filter((r) => going(r) && r.source_lang === source && r.target_lang === target)
    .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
}

/** The latest run for a direction that ended badly, so its page can say why. */
export function useFailedRun(source: string, target: string): TranslationRun | undefined {
  const latest = useRecords("TranslationRun")
    .filter((r) => r.source_lang === source && r.target_lang === target)
    .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
  return latest?.status === "failed" ? latest : undefined;
}

/** "Suggesting 12 of 50" and the like. */
export function runProgress(run: TranslationRun): string {
  const verb = run.kind === "suggest" ? "Suggesting" : "Translating";
  if (run.status === "queued") return `${verb}: waiting for the model`;
  return `${verb} ${run.done + run.failed} of ${run.total}`;
}

/** The server's models, and whether each answers; refreshed on demand. */
/** The models of the open workspace's place: this Mac's, or its server's. */
export function useModels() {
  const { place } = useWorkspace();
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      setModels(await api<ModelInfo[]>("GET", "/api/models", undefined, place));
      setError(null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [place]);
  useEffect(() => {
    void reload();
  }, [reload]);
  return { models, error, reload };
}
