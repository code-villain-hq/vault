import { Dialog } from "@base-ui/react/dialog";
import type { ModelCreate, ModelInfo } from "@glyph/types";
import { type FormEvent, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { PageHeader } from "./layout";
import { CreateDialog, Empty } from "./page-parts";
import { isDesktop } from "./platform";
import { useModels } from "./runs";
import { Button, ErrorMessage, TextField } from "./ui";
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
  const [adding, setAdding] = useState(false);
  // The desktop app's own Glyph keeps its models in a file Settings writes;
  // a server's are in its models file.
  const editable = place.kind === "mac";

  // While one starts, ask again until it answers or stops.
  const starting = models?.some((m) => status(m).tone === "warn");
  useEffect(() => {
    if (!starting) return;
    const timer = setInterval(() => void reload(), 2000);
    return () => clearInterval(timer);
  }, [starting, reload]);

  const act = async (model: ModelInfo, action: "start" | "stop" | "remove") => {
    setBusy(model.name);
    setError(null);
    const path = `/api/models/${encodeURIComponent(model.name)}`;
    try {
      if (action === "remove") await api("DELETE", path, undefined, place);
      else await api("POST", `${path}/${action}`, undefined, place);
    } catch (e) {
      setError(messageOf(e));
    }
    await reload();
    setBusy(null);
  };

  return (
    <>
      <PageHeader title="Models">
        {editable && models && models.length > 0 && (
          <Button variant="secondary" onClick={() => setAdding(true)}>
            Add model
          </Button>
        )}
        <Button variant="secondary" onClick={() => void reload()}>
          Check again
        </Button>
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-6 px-6 py-8">
          <ErrorMessage>{error ?? loadError}</ErrorMessage>
          {models?.length === 0 && editable && (
            <Empty title="No models yet">
              <p>
                Glyph translates with models on this Mac. Add one from a file you have, a download,
                or a server you run.
              </p>
              <Button className="mt-4" onClick={() => setAdding(true)}>
                Add model
              </Button>
            </Empty>
          )}
          {models?.length === 0 && !editable && (
            <Empty title="No models yet">
              The server reads its models from a TOML file: <code>GLYPH_MODELS</code>, or{" "}
              <code>models.toml</code> next to it. Copy <code>models.example.toml</code> to start.
            </Empty>
          )}
          {models && models.length > 0 && (
            <section>
              <p className="mb-4 text-muted">
                {editable
                  ? "The models on this Mac. Any server with an OpenAI-style API works."
                  : "The server's models, from its models file. Any server with an OpenAI-style API works; the ones with a model file or a download can be started from here."}
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
                        {editable && !own && (
                          <Button
                            variant="secondary"
                            disabled={busy === model.name}
                            onClick={() => void act(model, "remove")}
                          >
                            Remove
                          </Button>
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
      <CreateDialog open={adding} onClose={() => setAdding(false)} width="32rem">
        <AddModelForm
          onCancel={() => setAdding(false)}
          onAdd={async (model) => {
            await api<ModelInfo>("POST", "/api/models", model, place);
            setAdding(false);
            await reload();
          }}
        />
      </CreateDialog>
    </>
  );
}

type Source = "file" | "download" | "server";

const SOURCES: { value: Source; label: string }[] = [
  { value: "file", label: "A file on this Mac" },
  { value: "download", label: "Download" },
  { value: "server", label: "A server you run" },
];

/** Opens the Mac's file picker for a .gguf model; null when nothing was chosen. */
async function chooseModelFile(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({
    title: "Choose a model",
    multiple: false,
    directory: false,
    filters: [{ name: "Model", extensions: ["gguf"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

/**
 * Add model: a name and where it comes from. Glyph picks the port, the
 * context size and the program that runs it.
 */
function AddModelForm({
  onAdd,
  onCancel,
}: {
  onAdd: (model: ModelCreate) => Promise<void>;
  onCancel: () => void;
}) {
  // Viking 7B, as in models.example.toml: a good first model for Nordic languages.
  const [name, setName] = useState("viking");
  const [source, setSource] = useState<Source>("download");
  const [modelPath, setModelPath] = useState("");
  const [hfRepo, setHfRepo] = useState("mradermacher/Viking_7B_V.4-GGUF");
  const [hfFile, setHfFile] = useState("Viking_7B_V.4.Q4_K_M.gguf");
  const [endpoint, setEndpoint] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await onAdd({
        name,
        model_path: source === "file" ? modelPath : "",
        hf_repo: source === "download" ? hfRepo : "",
        hf_file: source === "download" ? hfFile : "",
        endpoint: source === "server" ? endpoint : "",
      });
    } catch (e) {
      setError(messageOf(e));
      setBusy(false);
    }
  };

  return (
    <form
      onSubmit={(event) => void submit(event)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          void submit();
        }
      }}
    >
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>Models</span>
        <span>›</span>
        <Dialog.Title className="font-normal">Add model</Dialog.Title>
      </div>
      <div className="flex flex-col gap-4 px-4 py-4">
        <TextField label="Name" required value={name} onChange={(e) => setName(e.target.value)} />
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-muted">Where it comes from</span>
          <fieldset className="ui-segmented flex self-start" aria-label="Where it comes from">
            {SOURCES.map((s) => (
              <button
                key={s.value}
                type="button"
                aria-pressed={source === s.value}
                onClick={() => setSource(s.value)}
                className="ui-segment px-2 text-xs"
              >
                {s.label}
              </button>
            ))}
          </fieldset>
        </div>
        {source === "file" && (
          <div className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <TextField
                label="Model file"
                required
                placeholder="/Users/you/Models/model.gguf"
                value={modelPath}
                onChange={(e) => setModelPath(e.target.value)}
              />
            </div>
            {isDesktop && (
              <Button
                variant="secondary"
                className="mb-1"
                onClick={() => void chooseModelFile().then((path) => path && setModelPath(path))}
              >
                Choose…
              </Button>
            )}
          </div>
        )}
        {source === "download" && (
          <>
            <TextField
              label="Hugging Face repository"
              required
              value={hfRepo}
              onChange={(e) => setHfRepo(e.target.value)}
            />
            <TextField
              label="File"
              required
              hint="Downloaded the first time you start it."
              value={hfFile}
              onChange={(e) => setHfFile(e.target.value)}
            />
          </>
        )}
        {source === "server" && (
          <TextField
            label="Address"
            required
            placeholder="http://localhost:11434"
            hint="Any server with an OpenAI-style API."
            value={endpoint}
            onChange={(e) => setEndpoint(e.target.value)}
          />
        )}
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-2 px-4 py-3">
        <Button variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={busy || !name.trim()}>
          {busy ? "Adding…" : "Add"}
        </Button>
      </div>
    </form>
  );
}
