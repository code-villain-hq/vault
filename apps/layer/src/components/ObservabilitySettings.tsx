import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { GrafanaDatasource } from "../lib/types";
import { useStore } from "../store";
import { Field } from "./ui";
import { S3LogSources } from "./S3LogSources";

/**
 * Settings → Observability: where agents read your services' logs (§23):
 * Grafana's Loki, and logs kept in S3, each a card of its own.
 */
export function ObservabilitySettings() {
  return (
    <>
      <p className="muted" style={{ marginTop: 0 }}>
        Agents read your services' logs through the app. They never see the tokens or the
        AWS keys, and nothing is changed where the logs are kept.
      </p>
      <GrafanaSource />
      <S3LogSources />
    </>
  );
}

/** Grafana's Loki (OBS-1). */
function GrafanaSource() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const toast = useStore((s) => s.toast);
  const fail = useStore((s) => s.fail);
  const connected = settings?.grafana_connected ?? false;
  const [url, setUrl] = useState(settings?.grafana?.base_url ?? "");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [sources, setSources] = useState<GrafanaDatasource[] | null>(null);
  const baseUrl = settings?.grafana?.base_url;

  // Asked of the site, not kept: a datasource added in Grafana shows up here
  // the next time the tab opens. Reconnecting can change the site, so it
  // asks again.
  useEffect(() => {
    if (!connected) return setSources(null);
    let live = true;
    api.grafanaListDatasources().then(
      (s) => live && setSources(s),
      () => live && setSources(null),
    );
    return () => {
      live = false;
    };
  }, [connected, baseUrl]);

  async function choose(uid: string) {
    try {
      await api.grafanaSetDatasource(uid || null);
      await refreshSettings();
    } catch (e) {
      fail(e);
    }
  }

  async function connect() {
    setBusy(true);
    try {
      const loki = await api.grafanaConnect(url.trim(), token.trim());
      setToken("");
      if (loki === 0) {
        toast("info", "Grafana connected, but the token can see no Loki datasource.");
      } else {
        toast("success", `Grafana connected: ${loki} Loki datasource${loki === 1 ? "" : "s"}.`);
      }
      await refreshSettings();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    try {
      await api.disconnect("grafana");
      await refreshSettings();
      toast("info", "grafana disconnected");
    } catch (e) {
      fail(e);
    }
  }

  return (
    <div className="card" style={{ marginBottom: 14 }}>
      <h3>Grafana</h3>
      {connected && (
        <div className="row" style={{ marginBottom: 14 }}>
          <span className="chip add">connected</span>
          <span className="muted">{settings?.grafana?.base_url}</span>
          <div className="spacer" />
          <button className="btn btn-sm btn-danger" onClick={() => void disconnect()}>
            Disconnect
          </button>
        </div>
      )}
      <p className="muted" style={{ marginTop: 0 }}>
        Logs in Loki, read with LogQL or with a Grafana Explore link from a ticket.
      </p>
      {sources && sources.length > 1 && (
        <Field
          label="Your services' logs"
          hint="The Loki datasource agents read when they don't name one. Grafana Cloud adds datasources of its own beside yours. One missing here is one the token may not query: give its service account access to it in Grafana."
        >
          <select value={settings?.grafana?.datasource ?? ""} onChange={(e) => void choose(e.target.value)}>
            <option value="">Not chosen: agents name one each time</option>
            {sources.map((s) => (
              <option key={s.uid} value={s.uid}>
                {s.name}
              </option>
            ))}
          </select>
        </Field>
      )}
      <Field label="Site URL" hint="Where you open Grafana, e.g. https://acme.grafana.net.">
        <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://acme.grafana.net" />
      </Field>
      <Field
        label="Service account token"
        hint="Administration → Users and access → Service accounts. Its role has to read and query datasources. Stored in your keychain."
      >
        <input
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder={connected ? "•••••••• (leave blank to keep)" : "glsa_…"}
        />
      </Field>
      <button
        className="btn btn-primary"
        disabled={busy || !url.trim() || (!token.trim() && !connected)}
        onClick={() => void connect()}
      >
        {busy ? "Verifying…" : connected ? "Save & reconnect" : "Connect"}
      </button>
    </div>
  );
}
