import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import type { AwsSetup, LogSource, S3Found } from "../lib/types";
import { Combo, Confirm, ErrorMessage, Field, useFailure } from "./ui";
import { useWorkspace } from "./SharedSettings";

/**
 * Settings → Observability → S3 log sources (OBS-8, OBS-9): logs kept in S3,
 * read with the user's own AWS profiles through the `aws` CLI. Each source
 * is a bucket and prefix, and optionally the Athena table over it.
 */
/** The daily scan limit unless a source sets another (OBS-14). */
const DEFAULT_DAILY_GB = 10;

function size(bytes: number): string {
  if (bytes >= 2 ** 30) return `${(bytes / 2 ** 30).toFixed(1)} GB`;
  if (bytes >= 2 ** 20) return `${(bytes / 2 ** 20).toFixed(1)} MB`;
  return bytes >= 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${bytes} B`;
}

export function S3LogSources() {
  const sources = useStore((s) => s.settings?.log_sources) ?? [];
  const scanned = useStore((s) => s.settings?.log_scanned_today);
  const refreshSettings = useStore((s) => s.refreshSettings);
  // Looking for the CLI fails in the card, as it opens or on Check again;
  // removing fails in its dialog (NOTE-6).
  const looking = useFailure();
  const [setup, setSetup] = useState<AwsSetup | null>(null);
  const [editing, setEditing] = useState<LogSource | null>(null);
  const [removing, setRemoving] = useState<LogSource | null>(null);
  // The sources are the workspace's; a member picks only their AWS profile (SET-7).
  const workspace = useWorkspace();
  const locked = workspace?.admin === false;

  const look = () => {
    looking.clear();
    void api.awsSetup().then(setSetup).catch(looking.fail);
  };
  useEffect(look, []); // eslint-disable-line react-hooks/exhaustive-deps -- once, and on Check again

  async function remove(s: LogSource) {
    await api.s3LogSourceRemove(s.id);
    await refreshSettings();
    setRemoving(null);
  }

  return (
    <div className="card" style={{ marginBottom: 14 }}>
      <h3>S3 log sources</h3>
      <p className="muted" style={{ marginTop: 0 }}>
        Logs kept in S3, read with your own AWS profiles through the <code>aws</code> CLI: the app keeps no
        AWS keys. Agents browse and read the files, and query them with SQL when Athena has a table over them.
      </p>

      {setup && !setup.cli && (
        <div className="row s3-missing">
          <span>
            The <code>aws</code> CLI is not on your PATH. Install it (<code>brew install awscli</code>) and sign in
            (<code>aws sso login</code>), then check again.
          </span>
          <div className="spacer" />
          <button className="ui-button" onClick={look}>Check again</button>
        </div>
      )}
      <ErrorMessage>{looking.error}</ErrorMessage>
      {locked && (
        <p className="settings-alone">
          {sources.length > 0
            ? `${workspace.name}'s, so admins add and remove them. Which of your AWS profiles reads each is yours.`
            : `${workspace.name} has no S3 log sources yet. Admins add them.`}
        </p>
      )}

      {sources.length > 0 && (
        <div className="s3-sources">
          {sources.map((s) => (
            <div key={s.id} className="s3-source">
              <div className="s3-source-text">
                <div className="s3-source-name">{s.name}</div>
                <div className="mono muted">s3://{s.bucket}/{s.prefix}</div>
                <div className="row s3-source-chips">
                  <span className="ui-chip">{s.profile}{s.region ? ` · ${s.region}` : ""}</span>
                  {s.athena ? (
                    <>
                      <span className="ui-status" title={`Workgroup ${s.athena.workgroup}`}>Athena: {s.athena.database}.{s.athena.table}</span>
                      <span className="ui-chip" title="What agents' queries scanned today, of the source's daily limit. Athena charges $5 a TB.">
                        today {size(scanned?.[s.id] ?? 0)} of {s.athena.daily_limit_gb ?? DEFAULT_DAILY_GB} GB
                      </span>
                    </>
                  ) : (
                    <span className="ui-chip">files only</span>
                  )}
                </div>
              </div>
              <button className="ui-button" disabled={!!editing} onClick={() => setEditing(s)}>{locked ? "Your profile" : "Edit"}</button>
              {!locked && <button className="ui-button ui-button-danger" onClick={() => setRemoving(s)}>Remove</button>}
            </div>
          ))}
        </div>
      )}

      {editing ? (
        <SourceEditor
          key={editing.id || "new"}
          start={editing}
          setup={setup}
          locked={locked}
          onDone={() => setEditing(null)}
        />
      ) : locked ? null : (
        <button
          className="ui-button ui-button-primary"
          disabled={!setup?.cli || setup.profiles.length === 0}
          title={setup?.cli && setup.profiles.length === 0 ? "~/.aws/config names no profile yet" : undefined}
          onClick={() => setEditing(blank(setup))}
        >
          Add a source
        </button>
      )}

      {removing && (
        <Confirm
          title={`Remove ${removing.name}?`}
          confirmLabel="Remove"
          body={
            <>
              Agents will no longer read <span className="mono">s3://{removing.bucket}/{removing.prefix}</span>. Nothing in S3 changes. This can't be undone.
            </>
          }
          onCancel={() => setRemoving(null)}
          onConfirm={() => remove(removing)}
        />
      )}
    </div>
  );
}

/**
 * The workgroup a table's queries likely run in: one named after its
 * database (`logs_db` or `logs-db`), else the only one besides `primary`.
 */
function workgroupFor(database: string, workgroups: string[] | null): string | undefined {
  const named = workgroups?.find((w) => w === database.replace(/_/g, "-") || w === database);
  if (named) return named;
  const others = workgroups?.filter((w) => w !== "primary") ?? [];
  return others.length === 1 ? others[0] : undefined;
}

/** What Save waits for, said beside it rather than left to a greyed button. */
function missing(s: LogSource, workgroup: string): string {
  if (!s.name.trim()) return "Give it a name to add it.";
  if (!s.bucket.trim()) return "Give the bucket to add it.";
  if (s.athena?.table && !workgroup.trim()) return "Give the Athena workgroup to add it.";
  return "";
}

function blank(setup: AwsSetup | null): LogSource {
  const p = setup?.profiles[0];
  return { id: "", name: "", profile: p?.name ?? "", region: null, bucket: "", prefix: "", athena: null };
}

/**
 * Adding or changing one source. "Find" looks in the bucket with the
 * profile: its folders, to narrow the prefix, and the Athena tables over it,
 * to pick one. Save checks the profile can list the bucket before keeping it.
 */
function SourceEditor({ start, setup, locked, onDone }: { start: LogSource; setup: AwsSetup | null; locked: boolean; onDone: () => void }) {
  const refreshSettings = useStore((s) => s.refreshSettings);
  // Find fails under its button, saving under the Save row (NOTE-6).
  const finding = useFailure();
  const saving = useFailure();
  const [s, setS] = useState<LogSource>(start);
  const [found, setFound] = useState<S3Found | null>(null);
  const [busy, setBusy] = useState<"find" | "save" | null>(null);
  const set = (patch: Partial<LogSource>) => setS((now) => ({ ...now, ...patch }));
  const profile = setup?.profiles.find((p) => p.name === s.profile);
  const athena = s.athena ?? { workgroup: "", database: "", table: "", daily_limit_gb: null };
  const setAthena = (patch: Partial<typeof athena>) => set({ athena: { ...athena, ...patch } });

  async function find() {
    setBusy("find");
    finding.clear();
    try {
      const f = await api.s3LogSourceFind(s.profile, s.region, s.bucket.trim(), s.prefix);
      setFound(f);
      // The only table there is, and the workgroup it is likely to need,
      // when nothing was picked yet.
      if (!s.athena?.table && f.tables.length === 1) {
        const t = f.tables[0];
        setAthena({ database: t.database, table: t.table, workgroup: athena.workgroup || workgroupFor(t.database, f.workgroups) || "" });
      }
    } catch (e) {
      setFound(null);
      finding.fail(e);
    } finally {
      setBusy(null);
    }
  }

  async function save() {
    setBusy("save");
    saving.clear();
    try {
      await api.s3LogSourceSave(s);
      await refreshSettings();
      onDone();
    } catch (e) {
      saving.fail(e);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="s3-editor">
      <fieldset className="settings-locked" disabled={locked}>
      <Field label="Name" hint="What agents ask for it by, e.g. Production request logs.">
        <input className="ui-field" value={s.name} onChange={(e) => set({ name: e.target.value })} placeholder="Production request logs" autoFocus={!locked} />
      </Field>
      </fieldset>
      <div className="row s3-editor-pair">
        <Field label="AWS profile" hint="From ~/.aws/config. SSO profiles sign in with aws sso login.">
          <select className="ui-field" value={s.profile} onChange={(e) => { set({ profile: e.target.value }); setFound(null); }}>
            {setup?.profiles.map((p) => <option key={p.name} value={p.name}>{p.name}</option>)}
          </select>
        </Field>
        <Field label="Region" hint={profile?.region ? `The profile's is ${profile.region}.` : "The profile sets none: give one."}>
          <input className="ui-field" disabled={locked} value={s.region ?? ""} onChange={(e) => set({ region: e.target.value || null })} placeholder={profile?.region ?? "eu-central-1"} />
        </Field>
      </div>
      <fieldset className="settings-locked" disabled={locked}>
      <div className="row s3-editor-pair">
        <Field label="Bucket">
          <input value={s.bucket} onChange={(e) => { set({ bucket: e.target.value }); setFound(null); }} placeholder="acme-request-logs" className="ui-field mono" />
        </Field>
        <Field label="Prefix" hint="Optional: agents read only under it.">
          <input value={s.prefix} onChange={(e) => set({ prefix: e.target.value })} placeholder="all of the bucket" className="ui-field mono" />
        </Field>
      </div>
      {!locked && (
      <button className="ui-button" disabled={!!busy || !s.bucket.trim() || !s.profile} onClick={() => void find()}>
        {busy === "find" ? "Looking…" : "Find"}
      </button>
      )}
      <ErrorMessage>{finding.error}</ErrorMessage>

      {found && (
        <div className="s3-found">
          <div className="muted">
            {found.folders.length + found.files === 0
              ? "Nothing under this prefix yet."
              : `${found.folders.length} folder${found.folders.length === 1 ? "" : "s"}${found.files ? `, ${found.files} file${found.files === 1 ? "" : "s"}` : ""}${found.more ? ", and more" : ""}:`}
          </div>
          {found.folders.length > 0 && (
            <div className="s3-folders">
              {found.folders.slice(0, 24).map((f) => (
                <button key={f} className="ui-chip s3-folder mono" title="Read only under this folder" onClick={() => set({ prefix: f })}>
                  {f.slice(s.prefix.length) || f}
                </button>
              ))}
            </div>
          )}
          <div className="s3-tables">
            <div className="muted">
              {found.tables.length === 0 ? "No Athena table over this bucket: agents will browse and read the files." : "Athena table, to query with SQL:"}
            </div>
            {found.tables.length > 0 && (
              <>
                <label className="s3-table">
                  <input type="radio" checked={!s.athena?.table} onChange={() => set({ athena: null })} />
                  <span>None: files only</span>
                </label>
                {found.tables.map((t) => (
                  <label key={`${t.database}.${t.table}`} className="s3-table">
                    <input
                      type="radio"
                      checked={athena.database === t.database && athena.table === t.table}
                      onChange={() => setAthena({
                        database: t.database,
                        table: t.table,
                        workgroup: workgroupFor(t.database, found.workgroups) ?? athena.workgroup,
                      })}
                    />
                    <span>
                      <span className="mono">{t.database}.{t.table}</span>
                      {t.partition_keys.length > 0 && <span className="muted"> · split by {t.partition_keys.map((k) => k.name).join(", ")}</span>}
                    </span>
                  </label>
                ))}
              </>
            )}
          </div>
        </div>
      )}

      {s.athena?.table && (
        <Field
          label="Athena workgroup"
          hint={found && found.workgroups === null
            ? "This profile may not list workgroups, and none is named after the table's database: type its name."
            : "Queries run, and are paid for, in it."}
        >
          <Combo value={athena.workgroup} onChange={(v) => setAthena({ workgroup: v })} options={found?.workgroups ?? []} placeholder="Its name, e.g. primary" />
        </Field>
      )}

      {s.athena?.table && (
        <Field
          label="Most data scanned a day (GB)"
          hint={`Athena charges $5 a TB scanned. Past this, agents' queries are refused until tomorrow. Empty is ${DEFAULT_DAILY_GB} GB.`}
        >
          <input className="ui-field"
            type="number"
            min={1}
            value={athena.daily_limit_gb ?? ""}
            placeholder={String(DEFAULT_DAILY_GB)}
            onChange={(e) => setAthena({ daily_limit_gb: e.target.value ? Math.max(1, Math.round(Number(e.target.value))) : null })}
            style={{ width: 120 }}
          />
        </Field>
      )}
      </fieldset>

      <div className="row" style={{ marginTop: 12 }}>
        <button
          className="ui-button ui-button-primary"
          disabled={!!busy || !s.name.trim() || !s.bucket.trim() || !s.profile || (!!s.athena?.table && !athena.workgroup.trim())}
          onClick={() => void save()}
        >
          {busy === "save" ? "Checking…" : start.id ? "Save" : "Add source"}
        </button>
        <button className="ui-button" disabled={busy === "save"} onClick={onDone}>Cancel</button>
        {!busy && <span className="muted">{missing(s, athena.workgroup)}</span>}
      </div>
      <ErrorMessage>{saving.error}</ErrorMessage>
    </div>
  );
}
