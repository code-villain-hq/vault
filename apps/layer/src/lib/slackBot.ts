import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { SlackBotView } from "./types";

/**
 * The Slack bot's connection and threads (§24), kept current by
 * `slackbot:changed`: connecting, connected, failing, a thread handed over.
 */
export function useSlackBot(): SlackBotView | null {
  const [view, setView] = useState<SlackBotView | null>(null);
  useEffect(() => {
    let current = true;
    const read = () => {
      api.slackBotView().then((v) => { if (current) setView(v); }).catch(() => {});
    };
    read();
    const un = listen("slackbot:changed", read);
    return () => {
      current = false;
      void un.then((f) => f());
    };
  }, []);
  return view;
}

/** Slack's limit on an app's name. */
export const BOT_NAME_MAX = 35;

/**
 * The Slack app for a bot called `name` (BOT-1): Socket Mode, so nothing has
 * to reach this Mac; the events of a mention, a direct message and a reply
 * in a channel it is in; the scopes to read a thread, name its people,
 * reply and react; and interactivity, for the buttons on a permission card
 * (BOT-7), which come over the same socket.
 */
export function botManifest(name: string): object {
  const n = name.trim().slice(0, BOT_NAME_MAX) || "layer";
  return {
    display_information: {
      name: n,
      description: "Your Layer, in Slack: mention it in a thread to hand the thread to an agent.",
      background_color: "#0c0c11",
    },
    features: {
      app_home: { home_tab_enabled: false, messages_tab_enabled: true, messages_tab_read_only_enabled: false },
      bot_user: { display_name: n, always_online: false },
    },
    oauth_config: {
      scopes: {
        bot: [
          "app_mentions:read",
          "channels:history",
          "channels:read",
          "chat:write",
          "groups:history",
          "groups:read",
          "im:history",
          "im:read",
          "mpim:history",
          "mpim:read",
          "reactions:write",
          "users:read",
        ],
      },
    },
    settings: {
      event_subscriptions: {
        bot_events: ["app_mention", "message.channels", "message.groups", "message.im", "message.mpim"],
      },
      interactivity: { is_enabled: true },
      org_deploy_enabled: false,
      socket_mode_enabled: true,
      token_rotation_enabled: false,
    },
  };
}

/** api.slack.com, with a new app from the manifest filled in. */
export function newAppUrl(name: string): string {
  return `https://api.slack.com/apps?new_app=1&manifest_json=${encodeURIComponent(JSON.stringify(botManifest(name)))}`;
}
