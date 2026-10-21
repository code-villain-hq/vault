import { Dialog } from "@base-ui/react/dialog";
import type { App, AppKey } from "@raven/types";
import { type FormEvent, useState } from "react";
import { api, messageOf } from "./api";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { PageHeader, useApps, useStatuses } from "./layout";
import { CreateDialog } from "./page-parts";
import { Button, ErrorMessage, TextField } from "./ui";
import { useIsAdmin, useWorkspace } from "./workspace";

/**
 * Settings › Apps: what the workspace watches. Admins add, change and
 * remove apps (synced, so it works offline) and make the key each app
 * sends its calls with (from the server, shown once).
 */
export function AppsPage() {
  const { place, workspace, sync } = useWorkspace();
  const isAdmin = useIsAdmin();
  const apps = useApps();
  const { byId } = useStatuses();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<App | null>(null);
  const [shown, setShown] = useState<{ app: App; key: string } | null>(null);
  // Apps given a key here, before the statuses next say they send.
  const [keyed, setKeyed] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const newKey = async (app: App) => {
    setError(null);
    try {
      const made = await api<AppKey>(
        "POST",
        `/api/w/${workspace.id}/apps/${encodeURIComponent(app.key)}/key`,
      );
      setKeyed((before) => new Set(before).add(app.id));
      setShown({ app, key: made.key });
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <>
      <PageHeader title="Apps">
        {isAdmin && <Button onClick={() => setAdding(true)}>Add app</Button>}
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-6 px-6 py-8">
          <p className="text-muted">
            The apps this workspace watches. An app sends its calls with its key.
          </p>
          <ErrorMessage>{error}</ErrorMessage>
          <ul className="-mx-3 flex flex-col gap-0.5">
            {apps.map((app) => {
              const sending = keyed.has(app.id) || byId.get(app.id)?.sending;
              const where = app.is_self
                ? place.kind === "mac"
                  ? "this Mac"
                  : "this server"
                : (app.health_url ?? "no health check");
              return (
                <li
                  key={app.id}
                  className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-hover"
                >
                  <div className="min-w-0 flex-1">
                    {isAdmin ? (
                      <button
                        type="button"
                        className="truncate text-left hover:underline"
                        title={`Change ${app.name}`}
                        onClick={() => setEditing(app)}
                      >
                        {app.name}
                      </button>
                    ) : (
                      <div className="truncate">{app.name}</div>
                    )}
                    <div className="truncate text-xs text-muted">
                      <span className="font-mono">{app.key}</span> · {where}
                    </div>
                  </div>
                  {app.is_self ? (
                    <span className="ui-status px-2 py-0.5 text-xs">Raven itself</span>
                  ) : (
                    <>
                      <span
                        className={`ui-status px-2 py-0.5 text-xs ${sending ? "ui-status-ok" : ""}`}
                      >
                        {sending ? "Sends data" : "No key yet"}
                      </span>
                      {isAdmin && (
                        <>
                          <Button
                            variant="secondary"
                            onClick={() =>
                              sending
                                ? setConfirmation({
                                    title: `Make a new key for ${app.name}?`,
                                    body: `${app.name}'s key stops working at once; set the new one where it runs.`,
                                    action: "New key",
                                    onConfirm: () => void newKey(app),
                                  })
                                : void newKey(app)
                            }
                          >
                            New key
                          </Button>
                          <Button
                            variant="secondary"
                            onClick={() =>
                              setConfirmation({
                                title: `Remove ${app.name}?`,
                                body: "Raven stops watching it, and what it sent goes with it.",
                                action: "Remove",
                                onConfirm: () => sync.delete("App", app.id),
                              })
                            }
                          >
                            Remove
                          </Button>
                        </>
                      )}
                    </>
                  )}
                </li>
              );
            })}
          </ul>
          {apps.length === 0 && (
            <p className="text-muted">
              {isAdmin ? "Nothing yet. Add the first app." : "Nothing yet. An admin adds apps."}
            </p>
          )}
        </div>
      </div>

      <CreateDialog open={adding} onClose={() => setAdding(false)} width="28rem">
        <AppForm apps={apps} onDone={() => setAdding(false)} />
      </CreateDialog>
      <CreateDialog open={editing !== null} onClose={() => setEditing(null)} width="28rem">
        {editing && <AppForm apps={apps} app={editing} onDone={() => setEditing(null)} />}
      </CreateDialog>
      <CreateDialog open={shown !== null} onClose={() => setShown(null)} width="28rem">
        {shown && <KeyShown app={shown.app} appKey={shown.key} onDone={() => setShown(null)} />}
      </CreateDialog>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

const SERVICE_NAME = /^[a-z0-9][a-z0-9-]{0,63}$/;

/** "Forge dev" → "forge-dev": what the app likely calls itself. */
function serviceNameOf(name: string) {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

/** Adds an app, or changes one (its service name stays: its calls carry it). */
function AppForm({ apps, app, onDone }: { apps: App[]; app?: App; onDone: () => void }) {
  const { sync } = useWorkspace();
  const [name, setName] = useState(app?.name ?? "");
  const [key, setKey] = useState(app?.key ?? "");
  const [keyTyped, setKeyTyped] = useState(Boolean(app));
  const [healthUrl, setHealthUrl] = useState(app?.health_url ?? "");
  const [error, setError] = useState<string | null>(null);
  const serviceName = keyTyped ? key : serviceNameOf(name);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const url = healthUrl.trim();
    if (!app && (!SERVICE_NAME.test(serviceName) || serviceName === "settings")) {
      setError("A service name is lowercase letters, digits and dashes, like forge.");
      return;
    }
    if (!app && apps.some((a) => a.key === serviceName)) {
      setError(`This workspace watches ${serviceName} already.`);
      return;
    }
    if (url && !/^https?:\/\/[^/\s]/.test(url)) {
      setError("A health address starts with http:// or https://.");
      return;
    }
    if (app) {
      // `null` stops the checks; the generated type only knows strings.
      sync.update("App", app.id, {
        name: name.trim(),
        health_url: (url || null) as string | undefined,
      });
    } else {
      sync.create("App", {
        key: serviceName,
        name: name.trim(),
        health_url: url || undefined,
        position: Math.max(0, ...apps.map((a) => a.position + 1)),
      });
    }
    onDone();
  };

  return (
    <form
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) submit(event);
      }}
    >
      <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
        <span>Apps</span>
        <span>›</span>
        <Dialog.Title className="font-normal">{app ? app.name : "Add app"}</Dialog.Title>
      </div>
      <div className="flex flex-col gap-4 px-4 py-4">
        <TextField
          label="Name"
          required
          autoFocus
          placeholder="Forge"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <TextField
          label="Service name"
          required
          disabled={Boolean(app)}
          placeholder="forge"
          hint={
            app
              ? "The service.name its calls carry. It can't change."
              : "The service.name its calls carry."
          }
          className="font-mono"
          value={serviceName}
          onChange={(e) => {
            setKeyTyped(true);
            setKey(e.target.value);
          }}
        />
        {!app?.is_self && (
          <TextField
            label="Health address"
            type="url"
            placeholder="https://forge.example.com/api/health"
            hint="Raven checks it every 30 seconds. Leave it empty to only take calls."
            value={healthUrl}
            onChange={(e) => setHealthUrl(e.target.value)}
          />
        )}
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-2 px-4 py-3">
        <Dialog.Close render={<Button variant="secondary" />}>Cancel</Dialog.Close>
        <Button type="submit" disabled={!name.trim()}>
          {app ? "Save" : "Add"}
        </Button>
      </div>
    </form>
  );
}

/** A new key, shown this once, to copy to where the app runs. */
function KeyShown({ app, appKey, onDone }: { app: App; appKey: string; onDone: () => void }) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(appKey);
      setCopied(true);
    } catch {
      setError("Couldn't copy; select the key and copy it yourself.");
    }
  };
  return (
    <div>
      <div className="px-4 pt-4">
        <Dialog.Title className="font-medium">{app.name}'s key</Dialog.Title>
      </div>
      <div className="flex flex-col gap-3 px-4 py-4">
        <div className="flex items-center gap-2">
          <input
            readOnly
            value={appKey}
            aria-label={`${app.name}'s key`}
            onFocus={(e) => e.target.select()}
            className="ui-field h-8 min-w-0 flex-1 rounded-md px-2 font-mono text-xs"
          />
          <Button variant="secondary" onClick={() => void copy()}>
            {copied ? "Copied" : "Copy"}
          </Button>
        </div>
        <p className="text-muted">
          Set it as {app.name}'s Raven key. It's shown only now; a new key replaces it.
        </p>
        <ErrorMessage>{error}</ErrorMessage>
      </div>
      <div className="flex items-center justify-end gap-2 px-4 py-3">
        <Button onClick={onDone}>Done</Button>
      </div>
    </div>
  );
}
