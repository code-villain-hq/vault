// The pages' data, as `crates/server/src/metrics.rs` writes it (snake_case).
// Times are ISO strings. The synced models and the account API come from
// `@raven/types`, generated from the Rust.

export type Level = "up" | "warn" | "down" | "unknown";

export type Status = {
  level: Level;
  reason: string;
  /** The action to look into, when calls are why. */
  action: string | null;
  last_check: string | null;
  /** The first check it missed, while it is down. */
  down_since: string | null;
};

export type AppSummary = {
  id: string;
  key: string;
  /** Whether the app has a key to send its calls with. */
  sending: boolean;
  status: Status;
};

export type Slot = {
  start: string;
  state: "ok" | "warn" | "down" | "none";
  checks: number;
  failed: number;
  errors: number;
};

export type Check = {
  checked_at: string;
  ok: boolean;
  status_code: number | null;
  duration_ms: number;
  error: string | null;
};

export type HealthPage = {
  name: string;
  health_url: string | null;
  /** Raven itself, checked at its own address. */
  is_self: boolean;
  sending: boolean;
  status: Status;
  uptime: number | null;
  typical_check_ms: number | null;
  slots: Slot[];
  recent_checks: Check[];
};

export type ActionRow = {
  name: string;
  calls: number;
  errors: number;
  p50_ms: number;
  p95_ms: number;
  normal_p95_ms: number | null;
  normal_calls: number | null;
  calls_per_min: number;
  error_rate: number;
  slower_than_usual: boolean;
};

export type ErrorGroup = {
  action: string;
  message: string;
  count: number;
  first_seen: string;
  last_seen: string;
  first_version: string | null;
  last_trace_id: string;
};

export type VersionMark = { version: string; first_seen: string };

export type TechnicalPage = {
  name: string;
  window_secs: number;
  actions: ActionRow[];
  errors: ErrorGroup[];
  versions: VersionMark[];
};

export type Point = {
  at: string;
  calls: number;
  errors: number;
  p50_ms: number;
  p95_ms: number;
};

export type CallSummary = {
  trace_id: string;
  started_at: string;
  duration_ms: number;
  http_status: number | null;
  is_error: boolean;
};

export type ActionPage = {
  app_name: string;
  name: string;
  window_secs: number;
  bucket_secs: number;
  stats: ActionRow | null;
  series: Point[];
  slowest: CallSummary[];
  errors: ErrorGroup[];
};

export type Span = {
  span_id: string;
  parent_span_id: string | null;
  name: string;
  kind: string;
  started_at: string;
  duration_ms: number;
  is_error: boolean;
  error_message: string | null;
  http_status: number | null;
  version: string | null;
  attributes: Record<string, unknown>;
};

export type LogLine = {
  at: string;
  level: string;
  message: string;
  target: string | null;
  span_id: string | null;
  attributes: Record<string, unknown>;
};

export type CallPage = {
  app_name: string;
  trace_id: string;
  spans: Span[];
  logs: LogLine[];
  logs_before: LogLine[];
};
