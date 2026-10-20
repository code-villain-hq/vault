import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { BOT_NAME_MAX, botManifest, newAppUrl, useSlackBot } from "../lib/slackBot";
import { useStore } from "../store";
import type { AgentStatus, SlackBotConfig, SlackBotStatus, SlackConfig } from "../lib/types";
import { ErrorMessage, Field, Switch, useFailure } from "./ui";

const STATE: Record<SlackBotStatus["state"], [string, string]> = {
  connected: ["connected", "ui-status ui-status-ok"],
  connecting: ["connecting…", "ui-status ui-status-warn"],
  failing: ["not connected", "ui-status ui-status-danger"],
  off: ["off", "ui-chip"],
};

/**
 * Settings → Messaging: the Slack bot (§24), a Slack app of your own, named by you,
 * that hands a thread to a chat when you mention it and posts the chat's
 * replies back to the thread; and the notifications it posts to a channel
 * (§11).
 */
export function SlackSettings() {
  const settings = useStore((s) => s.settings);
  const agents = useStore((s) => s.agents);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const toast = useStore((s) => s.toast);
  // Each card's actions fail in that card, the connect under its button
  // (NOTE-6).
  const card = useFailure();
  const voice = useFailure();
  const connecting = useFailure();
  const view = useSlackBot();
  const bot = settings?.slack_bot ?? null;
  const conversational = agents.filter((a) => a.installed && a.acp);

  const [name, setName] = useState(bot?.bot_name || "Layer");
  const [showManifest, setShowManifest] = useState(false);
  const [botToken, setBotToken] = useState("");
  const [appToken, setAppToken] = useState("");
  const [owner, setOwner] = useState(bot?.owner ?? "");
  const [agentId, setAgentId] = useState(bot?.agent_id || conversational[0]?.id || "");
  const [busy, setBusy] = useState(false);
  const [personality, setPersonality] = useState(bot?.personality ?? "");
  const personalityChanged = personality.trim() !== (bot?.personality ?? "");

  async function connect() {
    setBusy(true);
    connecting.clear();
    try {
      await api.slackBotConnect(botToken.trim(), appToken.trim(), owner.trim(), bot?.agent_id || agentId);
      setBotToken("");
      setAppToken("");
      toast("success", "Slack bot connected. Mention it in a thread to try it.");
      await refreshSettings();
    } catch (e) {
      connecting.fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function setOn(on: boolean) {
    card.clear();
    try {
      await api.setSlackBotOn(on);
      await refreshSettings();
    } catch (e) {
      card.fail(e);
    }
  }

  async function setAgent(id: string, model: string) {
    card.clear();
    try {
      await api.setSlackBotAgent(id, model);
      await refreshSettings();
    } catch (e) {
      card.fail(e);
    }
  }

  async function setAuto(on: boolean) {
    card.clear();
    try {
      await api.setSlackBotAuto(on);
      await refreshSettings();
    } catch (e) {
      card.fail(e);
    }
  }

  async function savePersonality() {
    voice.clear();
    try {
      await api.setSlackBotPersonality(personality);
      await refreshSettings();
      toast("success", "Saved. Threads handed over from now on get the new voice.");
    } catch (e) {
      voice.fail(e);
    }
  }

  async function disconnect() {
    card.clear();
    try {
      await api.disconnect("slack");
      await refreshSettings();
      toast("info", "Slack disconnected");
    } catch (e) {
      card.fail(e);
    }
  }

  const link = (href: string, text: string) => (
    <a className="ui-link" href="#" onClick={(e) => { e.preventDefault(); void openUrl(href); }}>
      {text}
    </a>
  );
  const status = view?.status;
  const [label, tone] = STATE[status?.state ?? "off"];

  return (
    <>
      <p className="muted" style={{ marginTop: 0 }}>
        Mention your bot in a Slack thread, or send it a direct message, and a chat opens here with
        the thread and what you asked. Its replies go back to the thread, and you can answer it there
        or here. It listens to you alone, and only while this app is open. It also posts the app's
        notifications to a channel you choose.
      </p>

      {bot && (
        <div className="card" style={{ marginBottom: 14 }}>
          <div className="row">
            <span className={tone}>{label}</span>
            <span className="muted">@{bot.bot_name}</span>
            <div className="spacer" />
            <button className="ui-button ui-button-danger" onClick={() => void disconnect()}>
              Disconnect
            </button>
          </div>
          {status?.error && (
            <div className="hint" style={{ color: "var(--status-danger-ink)", marginTop: 8 }}>{status.error}</div>
          )}
          <div className="switch-list" style={{ marginTop: 10 }}>
            <Switch
              label="Listen in Slack"
              detail="Off closes the connection and keeps the tokens and threads."
              checked={bot.enabled}
              onChange={(v) => void setOn(v)}
            />
            <Switch
              label="Work without asking"
              detail="Its chats start in the agent's auto mode, since you are not at the Mac to answer. Claude Code still stops at what it judges dangerous."
              checked={bot.auto}
              onChange={(v) => void setAuto(v)}
            />
          </div>
          <AgentAndModel bot={bot} conversational={conversational} onChange={(id, model) => void setAgent(id, model)} />
          {view && view.threads.length > 0 && (
            <div className="muted" style={{ marginTop: 10, lineHeight: 1.7 }}>
              Recent threads:{" "}
              {view.threads.slice(0, 5).map((t, i) => (
                <span key={t.room}>
                  {i > 0 && " · "}
                  {t.link ? link(t.link, `${t.label || "a thread"} ↗`) : t.label}
                </span>
              ))}
            </div>
          )}
          <ErrorMessage>{card.error}</ErrorMessage>
        </div>
      )}

      {bot && <Notifications botName={bot.bot_name} />}

      {bot && (
        <div className="card" style={{ marginBottom: 14 }}>
          <h3>Personality</h3>
          <Field
            label="Who it is"
            hint="Its replies in Slack are written in this voice. It changes how it sounds, not how carefully it works. A chat already open keeps the voice it started with."
          >
            <textarea className="ui-field"
              rows={4}
              maxLength={2000}
              value={personality}
              onChange={(e) => setPersonality(e.target.value)}
              placeholder="You are Doctor Doom, monarch of Latveria. Answer as Doom: grand, imperious, speaking of yourself as Doom. Keep every fact exact."
            />
          </Field>
          <button className="ui-button" disabled={!personalityChanged} onClick={() => void savePersonality()}>
            Save personality
          </button>
          <ErrorMessage>{voice.error}</ErrorMessage>
        </div>
      )}

      <div className="card" style={{ marginBottom: 14 }}>
        <h3>Making the bot</h3>
        <Field label="Its name" hint="What you will type after the @. Slack keeps the name; change it there later.">
          <input className="ui-field" value={name} maxLength={BOT_NAME_MAX} onChange={(e) => setName(e.target.value)} placeholder="drdoom" />
        </Field>
        <div className="muted" style={{ lineHeight: 1.7 }}>
          <ol style={{ margin: "0 0 0", paddingLeft: 18 }}>
            <li>
              {link(newAppUrl(name), "Create the app in Slack ↗")}, pick your workspace, and create it from
              the manifest filled in. (Or <b>Create New App</b> → <b>From an app manifest</b> and paste
              the one below.)
            </li>
            <li><b>Install to Workspace</b>. Your admin may have to approve it.</li>
            <li>
              Copy the <b>Bot User OAuth Token</b> (<code>xoxb-</code>) from <b>OAuth &amp; Permissions</b>.
            </li>
            <li>
              Under <b>Basic Information</b> → <b>App-Level Tokens</b>, generate one with the{" "}
              <code>connections:write</code> scope (<code>xapp-</code>).
            </li>
            <li>
              Your member ID: in Slack, open your profile, then <b>⋯</b> → <b>Copy member ID</b>.
            </li>
            <li>Invite the bot to the channels you want it in: <code>/invite @{name.trim() || "layer"}</code>.</li>
          </ol>
        </div>
        <div className="row" style={{ marginTop: 10 }}>
          <button className="ui-button" onClick={() => setShowManifest((v) => !v)}>
            {showManifest ? "Hide manifest" : "Show app manifest"}
          </button>
        </div>
        {showManifest && (
          <textarea className="ui-field"
            readOnly
            rows={16}
            value={JSON.stringify(botManifest(name), null, 2)}
            onFocus={(e) => e.currentTarget.select()}
            style={{ marginTop: 10, fontSize: "var(--type-ui-small-size)" }}
          />
        )}
      </div>

      <Field label="Bot token" hint="OAuth & Permissions → Bot User OAuth Token. Stored in your keychain.">
        <input className="ui-field"
          type="password"
          value={botToken}
          onChange={(e) => setBotToken(e.target.value)}
          placeholder={bot ? "•••••••• (leave blank to keep)" : "xoxb-…"}
        />
      </Field>
      <Field label="App-level token" hint="Basic Information → App-Level Tokens, with connections:write. Stored in your keychain.">
        <input className="ui-field"
          type="password"
          value={appToken}
          onChange={(e) => setAppToken(e.target.value)}
          placeholder={bot ? "•••••••• (leave blank to keep)" : "xapp-…"}
        />
      </Field>
      <Field label="Your member ID" hint="The one person the bot takes requests from.">
        <input className="ui-field" value={owner} onChange={(e) => setOwner(e.target.value)} placeholder="U0123ABCD" />
      </Field>
      {!bot && (
        <Field label="Agent" hint="Who a thread is handed to: an agent that runs as a conversation (ACP).">
          <select className="ui-field" value={agentId} onChange={(e) => setAgentId(e.target.value)}>
            {conversational.length === 0 && <option value="">No agent on your PATH runs over ACP</option>}
            {conversational.map((a) => (
              <option key={a.id} value={a.id}>{a.name}</option>
            ))}
          </select>
        </Field>
      )}
      <button
        className="ui-button ui-button-primary"
        disabled={busy || !owner.trim() || !(bot?.agent_id || agentId) || (!bot && (!botToken.trim() || !appToken.trim()))}
        onClick={() => void connect()}
      >
        {busy ? "Checking with Slack…" : bot ? "Save & reconnect" : "Connect"}
      </button>
      <ErrorMessage>{connecting.error}</ErrorMessage>
    </>
  );
}

/**
 * Who a thread is handed to and the model its chats run on (BOT-17). An
 * agent names its models only to a conversation, so the list is what it
 * offered when one last opened.
 */
function AgentAndModel({ bot, conversational, onChange }: {
  bot: SlackBotConfig;
  conversational: AgentStatus[];
  onChange: (agentId: string, model: string) => void;
}) {
  const offered = useStore((s) => s.settings?.agent_models[bot.agent_id]) ?? [];
  const agentName = conversational.find((a) => a.id === bot.agent_id)?.name ?? bot.agent_id;
  // A model saved that the agent no longer lists still shows as chosen.
  const models = bot.model && !offered.some((m) => m.value === bot.model)
    ? [...offered, { value: bot.model, name: bot.model }]
    : offered;
  return (
    <div className="row" style={{ marginTop: 10, gap: 12, alignItems: "flex-start" }}>
      <Field label="Agent" hint="Who a thread is handed to.">
        <select className="ui-field" value={bot.agent_id} onChange={(e) => onChange(e.target.value, "")}>
          {!conversational.some((a) => a.id === bot.agent_id) && <option value={bot.agent_id}>{agentName}</option>}
          {conversational.map((a) => (
            <option key={a.id} value={a.id}>{a.name}</option>
          ))}
        </select>
      </Field>
      <Field
        label="Model"
        hint={offered.length === 0
          ? `Open a chat with ${agentName} once and its models are listed here.`
          : "For chats opened from now on."}
      >
        <select className="ui-field" value={bot.model} onChange={(e) => onChange(bot.agent_id, e.target.value)}>
          <option value="">{agentName} decides</option>
          {models.map((m) => (
            <option key={m.value} value={m.value}>{m.name}</option>
          ))}
        </select>
      </Field>
    </div>
  );
}

/**
 * Where the bot posts the app's notifications (§11), and which. It posts
 * only where it has been invited, so choosing a channel posts a test
 * message there first.
 */
function Notifications({ botName }: { botName: string }) {
  const slack = useStore((s) => s.settings?.slack ?? null);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const toast = useStore((s) => s.toast);
  const failure = useFailure();
  const [channel, setChannel] = useState(slack?.channel ?? "");
  const [busy, setBusy] = useState(false);
  const chosen = slack !== null && slack.channel !== "";

  async function saveChannel() {
    setBusy(true);
    failure.clear();
    try {
      await api.setSlackChannel(channel.trim());
      toast("success", "Posted a test message: check the channel.");
      await refreshSettings();
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function save(prefs: SlackConfig) {
    failure.clear();
    try {
      await api.setSlackPrefs(prefs);
      await refreshSettings();
    } catch (e) {
      failure.fail(e);
    }
  }

  return (
    <div className="card" style={{ marginBottom: 14 }}>
      <h3>Notifications</h3>
      <Field
        label="Channel"
        hint={<>A channel name or ID. The bot posts only where it is invited: <code>/invite @{botName}</code> there first.</>}
      >
        <div className="row">
          <input className="ui-field" value={channel} onChange={(e) => setChannel(e.target.value)} placeholder="#eng-agents" />
          <button
            className="ui-button"
            disabled={busy || !channel.trim() || channel.trim() === slack?.channel}
            onClick={() => void saveChannel()}
          >
            {busy ? "Posting…" : "Save & test"}
          </button>
        </div>
      </Field>
      {chosen && (
        <Field label="What gets posted" hint="Muting is enforced in the backend, so an agent cannot route around it.">
          <div className="switch-list">
            <Switch
              label="Send anything to Slack"
              detail="Master switch. Off means silence, without disconnecting."
              checked={slack.enabled}
              onChange={(v) => void save({ ...slack, enabled: v })}
            />
            <Switch
              label="When an agent finishes"
              checked={slack.notify_on_done}
              disabled={!slack.enabled}
              onChange={(v) => void save({ ...slack, notify_on_done: v })}
            />
            <Switch
              label="When pull requests open"
              checked={slack.notify_on_prs}
              disabled={!slack.enabled}
              onChange={(v) => void save({ ...slack, notify_on_prs: v })}
            />
            <Switch
              label="Messages agents send themselves"
              detail="The slack_post tool. Agents post unattended, so this is separate."
              checked={slack.allow_agent_posts}
              disabled={!slack.enabled}
              onChange={(v) => void save({ ...slack, allow_agent_posts: v })}
            />
          </div>
        </Field>
      )}
      <ErrorMessage>{failure.error}</ErrorMessage>
    </div>
  );
}
