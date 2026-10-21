-- Raven's schema: accounts (as in Forge and Glyph, without workspaces), the
-- apps Raven watches, and what it hears from them.

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    password_hash text,
    -- The studio account (Pocket ID's `sub`) this account signs in with.
    oidc_sub text UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user ON sessions (user_id);

-- The apps Raven watches. `key` is what they call themselves in
-- OpenTelemetry (`service.name`).
CREATE TABLE apps (
    key text PRIMARY KEY,
    name text NOT NULL,
    -- Raven calls this every half minute; it's up when it answers 2xx.
    health_url text,
    -- The app sends its data with this key (stored hashed); none, none taken.
    ingest_key_hash bytea UNIQUE,
    position int NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO apps (key, name, health_url, position) VALUES
    ('forge', 'Forge', 'http://127.0.0.1:7380/api/health', 1),
    ('glyph', 'Glyph', 'http://127.0.0.1:7480/api/health', 2),
    ('raven', 'Raven', NULL, 3);

-- Level 1: each health check.
CREATE TABLE health_checks (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    app text NOT NULL REFERENCES apps (key) ON DELETE CASCADE,
    checked_at timestamptz NOT NULL DEFAULT now(),
    ok boolean NOT NULL,
    status_code int,
    duration_ms double precision NOT NULL,
    -- Why it failed, in words.
    error text
);
CREATE INDEX health_checks_app_time ON health_checks (app, checked_at DESC);

-- Level 2: calls and what happened inside them, as OpenTelemetry spans.
-- A `server` span is a call to one of the app's actions; its children
-- (database queries and the like) say where the time went.
CREATE TABLE spans (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    app text NOT NULL REFERENCES apps (key) ON DELETE CASCADE,
    trace_id text NOT NULL,
    span_id text NOT NULL,
    parent_span_id text,
    -- For a server span, the action: `GET /api/specs/{id}`.
    name text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('server', 'client', 'internal', 'db', 'producer', 'consumer')),
    started_at timestamptz NOT NULL,
    duration_ms double precision NOT NULL,
    is_error boolean NOT NULL DEFAULT false,
    error_message text,
    http_status int,
    -- The app's version or commit when it made the span.
    version text,
    attributes jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX spans_actions ON spans (app, started_at) WHERE kind = 'server';
CREATE INDEX spans_errors ON spans (app, started_at) WHERE is_error;
CREATE INDEX spans_trace ON spans (trace_id);

-- Log lines, tied to the call they were written in when there was one.
CREATE TABLE logs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    app text NOT NULL REFERENCES apps (key) ON DELETE CASCADE,
    at timestamptz NOT NULL,
    level text NOT NULL,
    message text NOT NULL,
    target text,
    trace_id text,
    span_id text,
    attributes jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX logs_trace ON logs (trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX logs_app_time ON logs (app, at DESC);
