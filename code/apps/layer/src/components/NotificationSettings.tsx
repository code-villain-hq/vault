import { useState } from "react";
import { api } from "../lib/api";
import type { SlackConfig } from "../lib/types";
import { useStore } from "../store";
import { heldText, Theirs, useHeld, useWorkspace, Yours } from "./SharedSettings";
import { ErrorMessage, Field, Switch, useFailure } from "./ui";
import { useUiPrefs } from "./useUiPrefs";

/**
 * Settings → Notifications: what the app tells you, as a banner on this Mac
 * (§11) and as a post to a Slack channel. Banners sat on Appearance and the
 * posts on Messaging, so neither was where you looked for the other.
 */
export function NotificationSettings({ toMessaging }: { toMessaging: () => void }) {
  const { ui, saveUi, failedUnder } = useUiPrefs<"banners">();
  const bot = useStore((s) => s.settings?.slack_bot ?? null);
  if (!ui) return null;
  return (
    <>
      <div className="card" style={{ marginBottom: 14 }}>
        <h3>On this Mac</h3>
        <Field
          label="Banners"
          hint="Only while the window is in the background. Opening the app does not announce what was already waiting."
        >
          <div className="switch-list">
            <Switch
              label="New reviews and tickets"
              detail="A banner when a pull request starts waiting on your review, or a ticket is assigned to you."
              checked={ui.system_notifications}
              onChange={(v) => void saveUi({ system_notifications: v }, "banners")}
            />
            <Switch
              label="Agents waiting on you"
              detail="A banner when an agent goes quiet, asks to trust its folder, runs out of budget or exits — and the number waiting on the dock icon."
              checked={ui.notify_waiting_agents}
              onChange={(v) => void saveUi({ notify_waiting_agents: v }, "banners")}
            />
          </div>
          {failedUnder("banners")}
        </Field>
      </div>

      {bot ? (
        <InSlack botName={bot.bot_name} />
      ) : (
        <div className="card" style={{ marginBottom: 14 }}>
          <h3>In Slack</h3>
          <p className="muted" style={{ margin: "0 0 10px" }}>
            Your Slack bot posts them to a channel. Make it under Messaging first.
          </p>
          <button className="ui-button" onClick={toMessaging}>
            Set up the bot
          </button>
        </div>
      )}
    </>
  );
}

/**
 * Where the bot posts the app's notifications (§11), and which. It posts
 * only where it has been invited, so choosing a channel posts a test
 * message there first.
 */
function InSlack({ botName }: { botName: string }) {
  const slack = useStore((s) => s.settings?.slack ?? null);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const failure = useFailure();
  const workspace = useWorkspace();
  // The channel is the workspace's (SET-7): a member posts their test there.
  const heldChannel = heldText(useHeld("slack"), "channel");
  const [channel, setChannel] = useState(slack?.channel || heldChannel || "");
  const [busy, setBusy] = useState(false);
  const chosen = slack !== null && slack.channel !== "";
  const memberToTest =
    workspace?.admin === false && heldChannel !== null && slack?.channel !== heldChannel;

  async function saveChannel() {
    setBusy(true);
    failure.clear();
    try {
      // No toast (NOTE-7): the test is there to see in the channel.
      await api.setSlackChannel(channel.trim());
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
      <h3>In Slack</h3>
      <Theirs kind="slack" what="Slack" rows={[["Channel", channel]]}>
        <Field
          label="Channel"
          hint={
            <>
              A channel name or ID. The bot posts only where it is invited:{" "}
              <code>/invite @{botName}</code> there first.
            </>
          }
        >
          <div className="row">
            <input
              className="ui-field"
              value={channel}
              onChange={(e) => setChannel(e.target.value)}
              placeholder="#eng-agents"
            />
            <button
              className="ui-button"
              disabled={busy || !channel.trim() || channel.trim() === slack?.channel}
              onClick={() => void saveChannel()}
            >
              {busy ? "Posting…" : "Save & test"}
            </button>
          </div>
        </Field>
      </Theirs>
      {memberToTest && (
        <button
          className="ui-button"
          disabled={busy}
          onClick={() => void saveChannel()}
          style={{ marginBottom: 14 }}
        >
          {busy ? "Posting…" : "Post a test there"}
        </button>
      )}
      {chosen && (
        <Yours>
          <Field
            label="What gets posted"
            hint="Muting is enforced in the backend, so an agent cannot route around it."
          >
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
        </Yours>
      )}
      <ErrorMessage>{failure.error}</ErrorMessage>
    </div>
  );
}
