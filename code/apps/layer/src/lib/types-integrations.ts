/**
 * The integrations' settings, mirrored from `config/integrations.rs`:
 * where each one is and how it is set. Split from `types.ts` at its size
 * ceiling; import from there.
 */
import type { TicketFlow } from "./types";

export interface JiraConfig {
  base_url: string;
  email: string;
  project_key: string | null;
  jql: string | null;
  /** Keyed by Jira project key. */
  flow: Record<string, TicketFlow>;
}

export interface GithubConfig {
  api_url: string;
  web_url: string;
  /** `@fe` or `org/fe`. Null lists only reviews requested of you. */
  review_team: string | null;
}

export interface SlackConfig {
  channel: string;
  enabled: boolean;
  notify_on_done: boolean;
  notify_on_prs: boolean;
  allow_agent_posts: boolean;
}

/** The Slack bot (§24): a Slack app of the user's own. Tokens are in the keychain. */
export interface SlackBotConfig {
  /** The one member it takes requests from (BOT-2). */
  owner: string;
  /** The agent a thread is handed to, over ACP. */
  agent_id: string;
  bot_user: string;
  bot_name: string;
  enabled: boolean;
  /** Its chats start in the agent's `auto` mode (BOT-12). */
  auto: boolean;
  /** The voice its replies are written in (BOT-13); empty is the agent's own. */
  personality: string;
  /** The model its chats run on, as the agent names it (BOT-17); empty is the agent's own. */
  model: string;
}

/** A model an agent offered when one of its conversations opened (BOT-17). */
export interface ModelChoice {
  value: string;
  name: string;
}

/** The bot's connection (BOT-10), as `slack_bot::Status` reports it. */
export interface SlackBotStatus {
  state: "off" | "connecting" | "connected" | "failing";
  error: string | null;
}

/** A Slack thread and its chat (BOT-11). */
export interface SlackThreadView {
  room: string;
  label: string;
  link: string;
  /** The chat's pane, while it is open. */
  pane_id: string | null;
}

export interface SlackBotView {
  status: SlackBotStatus;
  /** Newest first. */
  threads: SlackThreadView[];
}

/** The Grafana site agents read logs from (OBS-1). */
export interface GrafanaConfig {
  base_url: string;
  /** The Loki datasource agents query when they name none (OBS-5). */
  datasource: string | null;
}

/** One of a Grafana site's datasources, as `/api/datasources` lists it. */
export interface GrafanaDatasource {
  uid: string;
  name: string;
  type: string;
  isDefault: boolean;
}

/** Logs kept in S3 that agents read (OBS-8, OBS-9). No AWS key: the profile is the user's own. */
export interface LogSource {
  /** Empty for a new one: the backend gives it an id. */
  id: string;
  name: string;
  profile: string;
  /** The profile's own when null. */
  region: string | null;
  bucket: string;
  /** Empty, or ending in `/`: agents read only under it (OBS-10). */
  prefix: string;
  athena: AthenaTable | null;
}

/** The Athena table over a source's files (OBS-12). */
export interface AthenaTable {
  workgroup: string;
  database: string;
  table: string;
  /** The most its queries may scan in a day, in GB; null is 10 (OBS-14). */
  daily_limit_gb: number | null;
}

export interface AwsProfile {
  name: string;
  region: string | null;
}

/** The `aws` CLI (null when the PATH has none), and the profiles to pick from. */
export interface AwsSetup {
  cli: string | null;
  profiles: AwsProfile[];
}

export interface AthenaColumn {
  name: string;
  kind: string;
}

/** A table the Glue catalog has over a bucket. */
export interface AthenaTableInfo {
  database: string;
  table: string;
  location: string;
  columns: AthenaColumn[];
  partition_keys: AthenaColumn[];
}

/** What "Find" found under a bucket's prefix (OBS-9). */
export interface S3Found {
  folders: string[];
  files: number;
  more: boolean;
  tables: AthenaTableInfo[];
  /** Null when the profile may not list workgroups. */
  workgroups: string[] | null;
}
