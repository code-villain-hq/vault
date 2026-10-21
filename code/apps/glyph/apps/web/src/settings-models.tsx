import type { ModelInfo } from "@glyph/types";
import { useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { PageHeader } from "./layout";
import { Empty } from "./page-parts";
import { useModels } from "./runs";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** Running, starting (the server's process is up but the model doesn't answer yet) or stopped. */
function status(model: ModelInfo): { label: string; tone: "ok" | "warn" | "neutral" } {
  const own = model.process && model.process.exited === null;
  if (model.reachable) return { label: own ? "Running" : "Running (external)", tone: "ok" };
  if (own) return { label: "Starting", tone: "warn" };
  return { label: "Stopped", tone: "neutral" };
}

export function ModelsPage() {
  const { place } = useWorkspace();
  const { models, error: loadError, reload } = useModels();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  // While one starts, ask again until it answers or stops.
  const starting = models?.some((m) => status(m).tone === "warn");
  useEffect(() => {
    if (!starting) return;
    const timer = setInterval(() => void reload(), 2000);
    return () => clearInterval(timer);
  }, [starting, reload]);

  const act = async (model: ModelInfo, action: "start" | "stop") => {
    setBusy(model.name);
    setError(null);
    try {
      await api(
        "POST",
        `/api/models/${encodeURIComponent(model.name)}/${action}`,
        undefined,
        place,
      );
    } catch (e) {
      setError(messageOf(e));
    }
    await reload();
    setBusy(null);
  };

  return (
    <>
      <PageHeader title="Models">
        <Button variant="secondary" onClick={() => void reload()}>
          Check again
        </Button>
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-6 px-6 py-8">
          <ErrorMessage>{error ?? loadError}</ErrorMessage>
          {models?.length === 0 && (
            <Empty title="No models yet">
              The server reads its models from a TOML file: <code>GLYPH_MODELS</code>, or{" "}
              <code>models.toml</code> next to it. Copy <code>models.example.toml</code> to start.
            </Empty>
          )}
          {models && models.length > 0 && (
            <section>
              <p className="mb-4 text-muted">
                The server's models, from its models file. Any server with an OpenAI-style API
                works; the ones with a model file or a download can be started from here.
              </p>
              <ul className="-mx-3 flex flex-col gap-0.5">
                {models.map((model) => {
                  const { label, tone } = status(model);
                  const own = model.process && model.process.exited === null;
                  const output = model.process?.recent_output ?? [];
                  return (
                    <li key={model.name} className="rounded-md px-3 py-2 hover:bg-hover">
                      <div className="flex items-center gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="truncate">{model.name}</div>
                          <div className="truncate font-mono text-xs text-muted">
                            {model.endpoint}
                          </div>
                        </div>
                        <span
                          className={`ui-status px-2 py-0.5 text-xs ${tone === "neutral" ? "" : `ui-status-${tone}`}`}
                        >
                          {label}
                        </span>
                        {own ? (
                          <Button
                            variant="secondary"
                            disabled={busy === model.name}
                            onClick={() => void act(model, "stop")}
                          >
                            Stop
                          </Button>
                        ) : (
                          model.can_start &&
                          !model.reachable && (
                            <Button
                              variant="secondary"
                              disabled={busy === model.name}
                              onClick={() => void act(model, "start")}
                            >
                              Start
                            </Button>
                          )
                        )}
                      </div>
                      {model.process?.exited && (
                        <p className="mt-1 text-xs text-muted">Stopped: {model.process.exited}</p>
                      )}
                      {output.length > 0 && (
                        <details className="mt-2">
                          <summary className="cursor-default text-xs text-faint">Output</summary>
                          <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-panel p-2 font-mono text-xs text-muted">
                            {output.join("\n")}
                          </pre>
                        </details>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          )}
        </div>
      </div>
    </>
  );
}
