import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { GrafanaDatasource } from "../lib/types";
import { useStore } from "../store";
import { S3LogSources } from "./S3LogSources";
import {
  ChangeRow,
  ConnectRow,
  heldText,
  IN_KEYCHAIN,
  Theirs,
  useFold,
  useHeld,
  useTheirsLocked,
  Yours,
} from "./SharedSettings";
import { ErrorMessage, Field, useFailure } from "./ui";

/**
 * Settings → Logs: where agents read your services' logs (§23):
 * Grafana's Loki, and logs kept in S3, each a card of its own.
 */
export function ObservabilitySettings() {
  return (
    <>
      <p className="muted" style={{ marginTop: 0 }}>
        Agents read your services' logs through the app. They never see the tokens or the AWS keys,
        and nothing is changed where the logs are kept.
      </p>
      <GrafanaSource />
      <S3LogSources />
    </>
  );
}

/**
 * Grafana's Loki (OBS-1). Connected, the connection is text and its form
 * opens behind Change… (SET-14); the datasource agents read stays out, as
 * it is chosen day to day.
 */
function GrafanaSource() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  // Disconnecting fails under its row, choosing under the list, connecting
  // under its button (NOTE-6).
  const unlink = useFailure();
  const choosing = useFailure();
  const connecting = useFailure();
  const connected = settings?.grafana_connected ?? false;
  const held = useHeld("grafana");
  // Not connected here: the workspace's site, so connecting takes only your token (SET-8).
  const [url, setUrl] = useState(settings?.grafana?.base_url ?? heldText(held, "base_url") ?? "");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [sources, setSources] = useState<GrafanaDatasource[] | null>(null);
  const baseUrl = settings?.grafana?.base_url;
  const { folded, change, fold } = useFold(connected);
  const locked = useTheirsLocked("grafana");

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
    choosing.clear();
    try {
      await api.grafanaSetDatasource(uid || null);
      await refreshSettings();
    } catch (e) {
      choosing.fail(e);
    }
  }

  function cancel() {
    setUrl(baseUrl ?? heldText(held, "base_url") ?? "");
    setToken("");
    connecting.clear();
    fold();
  }

  async function connect() {
    setBusy(true);
    connecting.clear();
    try {
      await api.grafanaConnect(url.trim(), token.trim());
      setToken("");
      await refreshSettings();
      fold();
    } catch (e) {
      connecting.fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    unlink.clear();
    try {
      await api.disconnect("grafana");
      await refreshSettings();
    } catch (e) {
      unlink.fail(e);
    }
  }

  const site = baseUrl ?? heldText(held, "base_url");
  const datasource =
    sources?.find((s) => s.uid === settings?.grafana?.datasource)?.name ??
    heldText(held, "datasource");
  // Saved as it changes, so it stays out of the fold.
  const choice = sources && sources.length > 1 && (
    <Field
      label="Your services' logs"
      hint="The Loki datasource agents read when they don't name one. Grafana Cloud adds datasources of its own beside yours. One missing here is one the token may not query: give its service account access to it in Grafana."
    >
      <select
        className="ui-field"
        value={settings?.grafana?.datasource ?? ""}
        onChange={(e) => void choose(e.target.value)}
      >
        <option value="">Not chosen: agents name one each time</option>
        {sources.map((s) => (
          <option key={s.uid} value={s.uid}>
            {s.name}
          </option>
        ))}
      </select>
      <ErrorMessage>{choosing.error}</ErrorMessage>
    </Field>
  );

  return (
    <div className="card" style={{ marginBottom: 14 }}>
      <h3>Grafana</h3>
      {connected && (
        <div className="row" style={{ marginBottom: 14 }}>
          <span className="ui-status">connected</span>
          <span className="muted">{settings?.grafana?.base_url}</span>
          <div className="spacer" />
          <button className="ui-button ui-button-danger" onClick={() => void disconnect()}>
            Disconnect
          </button>
        </div>
      )}
      <ErrorMessage>{unlink.error}</ErrorMessage>
      {/* Said here rather than in a toast as it connects (NOTE-7): it stays
          true, and worth seeing, until the token is given one. */}
      {connected && sources?.length === 0 && (
        <p className="ui-tint-warn error-message">
          The token can see no Loki datasource. Give its service account access to one in Grafana.
        </p>
      )}
      <p className="muted" style={{ marginTop: 0 }}>
        Logs in Loki, read with LogQL or with a Grafana Explore link from a ticket.
      </p>
      <Theirs
        kind="grafana"
        what="Grafana"
        rows={
          // An admin's datasource is the list kept beside the text, as today.
          locked
            ? [
                ["Site", site],
                ["Your services' logs", datasource],
              ]
            : [["Site", site]]
        }
        folded={folded}
        keep={choice}
      >
        {choice}
        <Field label="Site URL" hint="Where you open Grafana, e.g. https://acme.grafana.net.">
          <input
            className="ui-field"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://acme.grafana.net"
          />
        </Field>
      </Theirs>
      <Yours rows={[["Service account token", IN_KEYCHAIN]]} folded={folded}>
        <Field
          label="Service account token"
          hint="Administration → Users and access → Service accounts. Its role has to read and query datasources. Stored in your keychain."
        >
          <input
            className="ui-field"
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder={connected ? "•••••••• (leave blank to keep)" : "glsa_…"}
          />
        </Field>
      </Yours>
      {folded ? (
        <ChangeRow
          note={
            locked
              ? "Your token: it is tried before it is kept."
              : "The site or the token: they are tried together."
          }
          onClick={change}
        />
      ) : (
        <ConnectRow
          connected={connected}
          busy={busy}
          disabled={!url.trim() || (!token.trim() && !connected)}
          onConnect={() => void connect()}
          onCancel={cancel}
        />
      )}
      <ErrorMessage>{connecting.error}</ErrorMessage>
    </div>
  );
}
