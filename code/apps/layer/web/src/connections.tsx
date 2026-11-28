import { type Confirmation, ConfirmDialog } from "@code-villain/web";
import type { Connection, ConnectionKind, ConnectionSettings, GithubSettings } from "@layer/types";
import { type FormEvent, useState } from "react";
import { PageBody, PageHeader } from "./layout";
import { MoreMenu } from "./page-parts";
import { Button, Section, TextField } from "./ui";
import { useIsAdmin, useRecords, useWorkspace } from "./workspace";

interface FieldSpec {
  key: string;
  label: string;
  placeholder?: string;
  hint?: string;
  required?: boolean;
  mono?: boolean;
}

/** What each kind shares, in the order Layer's Settings asks for it. */
const KINDS: { kind: ConnectionKind; name: string; about: string; fields: FieldSpec[] }[] = [
  {
    kind: "jira",
    name: "Jira",
    about: "Where tasks come from, and where their tickets move as the work does.",
    fields: [
      {
        key: "base_url",
        label: "Site",
        placeholder: "https://acme.atlassian.net",
        required: true,
        mono: true,
      },
      { key: "project_key", label: "Project", placeholder: "ACME", mono: true },
      {
        key: "jql",
        label: "Task list",
        placeholder: "assignee = currentUser() AND statusCategory != Done",
        hint: "JQL. Layer's own when empty.",
        mono: true,
      },
      {
        key: "epic_field",
        label: "Epic field",
        placeholder: "customfield_10014",
        hint: "Only for a site that keeps the epic link in a field of its own.",
        mono: true,
      },
    ],
  },
  {
    kind: "github",
    name: "GitHub",
    about: "Pull requests, reviews and checks for the workspace's repositories.",
    fields: [
      {
        key: "api_url",
        label: "API",
        placeholder: "https://api.github.com",
        required: true,
        mono: true,
      },
      {
        key: "web_url",
        label: "Web",
        placeholder: "https://github.com",
        required: true,
        mono: true,
      },
      {
        key: "review_team",
        label: "Review team",
        placeholder: "acme/frontend",
        hint: "Its review queue is listed beside each person's own.",
        mono: true,
      },
    ],
  },
  {
    kind: "slack",
    name: "Slack",
    about: "Where Layer posts when a task's work is ready.",
    fields: [{ key: "channel", label: "Channel", placeholder: "#dev", required: true, mono: true }],
  },
  {
    kind: "grafana",
    name: "Grafana",
    about: "Logs agents may read while they work.",
    fields: [
      {
        key: "base_url",
        label: "Site",
        placeholder: "https://acme.grafana.net",
        required: true,
        mono: true,
      },
      {
        key: "datasource",
        label: "Logs",
        placeholder: "grafanacloud-logs",
        hint: "The Loki datasource agents read when they name none.",
        mono: true,
      },
    ],
  },
];

/**
 * `/$workspace/connections`: where the workspace's Jira, GitHub, Slack and
 * Grafana are. Admins set them; everyone uses them, each with their own
 * token, which stays in Layer on their Mac.
 */
export function ConnectionsPage() {
  const connections = useRecords("Connection");
  const admin = useIsAdmin();

  return (
    <>
      <PageHeader title="Connections" />
      <PageBody>
        <p className="max-w-lg text-muted">
          Where the workspace's tools are, for everyone in it. Each person's token, and their Jira
          email, stays in Layer on their own Mac.
          {!admin && " Admins change these."}
        </p>
        {KINDS.map((spec) =>
          spec.kind === "github" ? (
            <GithubConnections
              key={spec.kind}
              spec={spec}
              connections={connections}
              admin={admin}
            />
          ) : (
            <ConnectionSection
              key={spec.kind}
              spec={spec}
              connection={connections.find((c) => c.kind === spec.kind)}
              admin={admin}
            />
          ),
        )}
      </PageBody>
    </>
  );
}

function hostOf(source: GithubSettings): string {
  try {
    return new URL(source.web_url).hostname.toLowerCase().replace(/\.+$/, "");
  } catch {
    return "";
  }
}

function GithubConnections({
  spec,
  connections,
  admin,
}: {
  spec: (typeof KINDS)[number];
  connections: readonly Connection[];
  admin: boolean;
}) {
  const { sync } = useWorkspace();
  const legacy = connections.find((c) => c.kind === "github");
  const extra = connections.find((c) => c.kind === "github_sources");
  const original = legacy?.settings && "web_url" in legacy.settings ? legacy.settings : null;
  const additional = extra?.settings && "sources" in extra.settings ? extra.settings.sources : [];
  const sources = [
    ...(legacy && original ? [{ record: legacy, source: original, old: true }] : []),
    ...(extra ? additional.map((source) => ({ record: extra, source, old: false })) : []),
  ];
  const add = (settings: GithubSettings, from?: GithubSettings) => {
    const host = hostOf(settings);
    if (!host) throw new Error("Enter a GitHub web URL with a host.");
    if (sources.some((s) => s.source !== from && hostOf(s.source) === host)) {
      throw new Error(`${host} is already a GitHub source.`);
    }
    return settings;
  };
  const saveExtra = (settings: GithubSettings[]) => {
    if (extra) sync.update("Connection", extra.id, { settings: { sources: settings } });
    else sync.create("Connection", { kind: "github_sources", settings: { sources: settings } });
  };
  return (
    <>
      {sources.map(({ record, source, old }) => (
        <ConnectionSection
          key={hostOf(source) || source.web_url}
          spec={{ ...spec, name: hostOf(source) || "GitHub" }}
          connection={{ ...record, settings: source }}
          admin={admin}
          onSave={(settings) => {
            const next = add(settings as GithubSettings, source);
            if (old) sync.update("Connection", record.id, { settings: next });
            else saveExtra(additional.map((s) => (s === source ? next : s)));
          }}
          onDisconnect={() => {
            if (old) sync.delete("Connection", record.id);
            else saveExtra(additional.filter((s) => s !== source));
          }}
        />
      ))}
      {(admin || sources.length === 0) && (
        <ConnectionSection
          spec={{ ...spec, name: "GitHub sources" }}
          connection={undefined}
          admin={admin}
          addLabel="Add GitHub source"
          onSave={(settings) => saveExtra([...additional, add(settings as GithubSettings)])}
        />
      )}
    </>
  );
}

function ConnectionSection({
  spec,
  connection,
  admin,
  addLabel = "Connect",
  onSave,
  onDisconnect,
}: {
  spec: (typeof KINDS)[number];
  connection: Connection | undefined;
  admin: boolean;
  addLabel?: string;
  onSave?: (settings: ConnectionSettings) => void;
  onDisconnect?: () => void;
}) {
  const { sync } = useWorkspace();
  const [editing, setEditing] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const settings = (connection?.settings ?? {}) as Record<string, unknown>;

  const aside =
    admin && connection && !editing ? (
      <MoreMenu
        label={`${spec.name} actions`}
        items={[
          { label: "Change", onSelect: () => setEditing(true) },
          {
            label: "Disconnect",
            danger: true,
            onSelect: () =>
              setConfirmation({
                title: `Disconnect ${spec.name}?`,
                body: `Nobody in the workspace reaches ${spec.name} from Layer until it is connected again. Tokens stay on each Mac.`,
                action: "Disconnect",
                onConfirm: onDisconnect ?? (() => sync.delete("Connection", connection.id)),
              }),
          },
        ]}
      />
    ) : null;

  return (
    <Section title={spec.name} aside={aside}>
      {editing ? (
        <ConnectionForm
          spec={spec}
          connection={connection}
          onDone={() => setEditing(false)}
          onSave={onSave}
        />
      ) : connection ? (
        <dl className="grid max-w-lg grid-cols-[8rem_1fr] gap-x-4 gap-y-2">
          {spec.fields.map((field) => (
            <div key={field.key} className="contents">
              <dt className="text-muted">{field.label}</dt>
              <dd className={`truncate ${field.mono ? "font-mono text-xs leading-5" : ""}`}>
                {typeof settings[field.key] === "string" && settings[field.key] ? (
                  String(settings[field.key])
                ) : (
                  <span className="font-sans text-sm text-faint">Not set</span>
                )}
              </dd>
            </div>
          ))}
        </dl>
      ) : (
        <div className="flex max-w-lg items-center gap-3">
          <p className="flex-1 text-muted">{spec.about}</p>
          {admin && (
            <Button variant="secondary" onClick={() => setEditing(true)}>
              {addLabel}
            </Button>
          )}
        </div>
      )}
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </Section>
  );
}

function ConnectionForm({
  spec,
  connection,
  onDone,
  onSave,
}: {
  spec: (typeof KINDS)[number];
  connection: Connection | undefined;
  onDone: () => void;
  onSave?: (settings: ConnectionSettings) => void;
}) {
  const { sync } = useWorkspace();
  const [error, setError] = useState<string | null>(null);
  const saved = (connection?.settings ?? {}) as Record<string, unknown>;
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      spec.fields.map((f) => [f.key, typeof saved[f.key] === "string" ? String(saved[f.key]) : ""]),
    ),
  );
  const complete = spec.fields.every((f) => !f.required || values[f.key]?.trim());

  const save = (event: FormEvent) => {
    event.preventDefault();
    // The settings go whole. What the page doesn't show (Jira's ticket flow)
    // is kept; an empty field is left out, which is "not set".
    const settings: Record<string, unknown> = { ...saved };
    for (const field of spec.fields) {
      const value = (values[field.key] ?? "").trim();
      if (value) settings[field.key] = value;
      else delete settings[field.key];
    }
    try {
      if (onSave) {
        onSave(settings as ConnectionSettings);
      } else if (connection) {
        sync.update("Connection", connection.id, { settings: settings as ConnectionSettings });
      } else {
        sync.create("Connection", { kind: spec.kind, settings: settings as ConnectionSettings });
      }
      onDone();
    } catch (value) {
      setError(value instanceof Error ? value.message : String(value));
    }
  };

  return (
    <form onSubmit={save} className="flex max-w-lg flex-col gap-4">
      {spec.fields.map((field, index) => (
        <TextField
          key={field.key}
          label={field.label}
          placeholder={field.placeholder}
          hint={field.hint}
          required={field.required}
          autoFocus={index === 0}
          className={field.mono ? "font-mono" : ""}
          value={values[field.key] ?? ""}
          onChange={(e) => setValues({ ...values, [field.key]: e.target.value })}
        />
      ))}
      {error && (
        <p role="alert" className="text-danger">
          {error}
        </p>
      )}
      <div className="flex gap-2">
        <Button type="submit" disabled={!complete}>
          {connection ? "Save" : "Connect"}
        </Button>
        <Button variant="secondary" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
