-- Initial schema for Schemer (a Code Villain product).
-- Migrations are run with goose: https://github.com/pressly/goose
-- Design notes live in docs/DATA_MODEL.md. Read that first.
--
-- Invariants:
--   * `events` is append-only. Never UPDATE or DELETE rows in it.
--   * Every state change appends an event AND updates its projection table
--     in the same transaction.
--   * Event-sourced projections (work_items, changes, check_runs, releases,
--     release_outcomes, edges) can always be rebuilt by replaying `events`.
--     Rebuild truncates all six in ONE statement (foreign keys between them).
--   * Everything else (users, sessions, workspaces, members, invites, tokens,
--     installations, repositories, metrics, metric_points, webhook_deliveries,
--     jobs) is plain CRUD and is not replayed.

-- +goose Up
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ---------------------------------------------------------------------------
-- Identity
-- ---------------------------------------------------------------------------

CREATE TABLE users (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    github_user_id  bigint NOT NULL UNIQUE,
    login           text NOT NULL,
    name            text,
    avatar_url      text,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    -- SHA-256 of the random session token; the raw token only lives in the cookie.
    token_hash  bytea PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE workspaces (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    slug              text NOT NULL UNIQUE,
    name              text NOT NULL,
    issue_key         text NOT NULL CHECK (issue_key ~ '^[A-Z][A-Z0-9]{1,9}$'),
    next_item_number  integer NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE workspace_members (
    workspace_id  uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role          text NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'member')),
    joined_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

-- Invite by GitHub login. Accepted automatically when that login signs in.
CREATE TABLE workspace_invites (
    workspace_id  uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    github_login  text NOT NULL,               -- store lower-cased
    invited_by    uuid NOT NULL REFERENCES users (id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    accepted_at   timestamptz,
    PRIMARY KEY (workspace_id, github_login)
);
CREATE INDEX workspace_invites_login_idx ON workspace_invites (github_login) WHERE accepted_at IS NULL;

-- Tokens for the machine APIs (releases, metrics). Store only the hash.
CREATE TABLE api_tokens (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name          text NOT NULL,
    token_hash    bytea NOT NULL UNIQUE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz,
    revoked_at    timestamptz
);

-- ---------------------------------------------------------------------------
-- Event log: the source of truth
-- ---------------------------------------------------------------------------

CREATE TABLE events (
    seq              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id               uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    workspace_id     uuid NOT NULL REFERENCES workspaces (id),
    -- The full loop is reserved here; v1 only writes the delivery-slice types.
    node_type        text NOT NULL CHECK (node_type IN (
                         'signal', 'idea', 'decision',              -- v3/v4
                         'work_item', 'change', 'check_run', 'release', -- v1
                         'metric', 'outcome'                        -- reserved
                     )),
    node_id          uuid NOT NULL,
    type             text NOT NULL,            -- e.g. 'work_item.created', 'change.merged'
    payload          jsonb NOT NULL DEFAULT '{}'::jsonb,
    actor_type       text NOT NULL CHECK (actor_type IN ('user', 'github', 'api', 'system')),
    actor_id         text,                     -- user id, GitHub login, token id...
    -- Set for anything coming from outside (webhook delivery id, client mutation id)
    -- so retries never create duplicate events.
    idempotency_key  text,
    occurred_at      timestamptz NOT NULL,     -- when it happened in the real world
    recorded_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, idempotency_key)
);
CREATE INDEX events_node_idx ON events (workspace_id, node_type, node_id, seq);
CREATE INDEX events_workspace_seq_idx ON events (workspace_id, seq);

-- ---------------------------------------------------------------------------
-- Projections (read models, rebuilt from events)
-- ---------------------------------------------------------------------------

CREATE TABLE work_items (
    id              uuid PRIMARY KEY,
    workspace_id    uuid NOT NULL REFERENCES workspaces (id),
    number          integer NOT NULL,
    title           text NOT NULL,
    description     text NOT NULL DEFAULT '',
    status          text NOT NULL DEFAULT 'backlog' CHECK (status IN (
                        'backlog', 'todo', 'in_progress', 'in_review',
                        'done', 'released', 'cancelled'
                    )),
    -- 'manual' = a person set the current status; 'derived' = the status engine did.
    status_source   text NOT NULL DEFAULT 'manual' CHECK (status_source IN ('manual', 'derived')),
    blocked_reason  text,                      -- NULL = not blocked; e.g. 'ci_failing'
    priority        smallint NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
    assignee_id     uuid REFERENCES users (id),
    labels          text[] NOT NULL DEFAULT '{}',
    created_at      timestamptz NOT NULL,
    updated_at      timestamptz NOT NULL,
    last_event_seq  bigint NOT NULL,
    search          tsvector GENERATED ALWAYS AS (
                        setweight(to_tsvector('simple', coalesce(title, '')), 'A') ||
                        setweight(to_tsvector('simple', coalesce(description, '')), 'B')
                    ) STORED,
    UNIQUE (workspace_id, number)
);
CREATE INDEX work_items_search_idx ON work_items USING gin (search);
CREATE INDEX work_items_title_trgm_idx ON work_items USING gin (title gin_trgm_ops);
CREATE INDEX work_items_status_idx ON work_items (workspace_id, status);
CREATE INDEX work_items_assignee_idx ON work_items (workspace_id, assignee_id);
CREATE INDEX work_items_labels_idx ON work_items USING gin (labels);

-- ---------------------------------------------------------------------------
-- GitHub
-- ---------------------------------------------------------------------------

CREATE TABLE github_installations (
    installation_id  bigint PRIMARY KEY,
    workspace_id     uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    account_login    text NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    suspended_at     timestamptz
);

CREATE TABLE repositories (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    installation_id  bigint NOT NULL REFERENCES github_installations (installation_id) ON DELETE CASCADE,
    github_repo_id   bigint NOT NULL UNIQUE,
    full_name        text NOT NULL,            -- 'owner/name'
    default_branch   text NOT NULL DEFAULT 'main',
    created_at       timestamptz NOT NULL DEFAULT now()
);

-- Raw webhook inbox. Store first, process in a job: fast responses to GitHub,
-- idempotency by delivery id, and a replayable history when processing has a bug.
CREATE TABLE webhook_deliveries (
    delivery_id   text PRIMARY KEY,            -- X-GitHub-Delivery header
    source        text NOT NULL DEFAULT 'github',
    event         text NOT NULL,               -- X-GitHub-Event header
    payload       jsonb NOT NULL,
    received_at   timestamptz NOT NULL DEFAULT now(),
    processed_at  timestamptz,
    error         text
);
CREATE INDEX webhook_deliveries_unprocessed_idx ON webhook_deliveries (received_at)
    WHERE processed_at IS NULL;

-- Pull requests ("changes" in the loop, so other code hosts can fit later).
CREATE TABLE changes (
    id                uuid PRIMARY KEY,
    workspace_id      uuid NOT NULL REFERENCES workspaces (id),
    repository_id     uuid NOT NULL REFERENCES repositories (id),
    external_number   integer NOT NULL,        -- PR number
    title             text NOT NULL,
    body              text NOT NULL DEFAULT '',
    branch            text NOT NULL DEFAULT '',
    state             text NOT NULL CHECK (state IN ('draft', 'open', 'closed', 'merged')),
    author_login      text,
    head_sha          text,
    merge_commit_sha  text,
    url               text NOT NULL,
    opened_at         timestamptz,
    merged_at         timestamptz,
    closed_at         timestamptz,
    updated_at        timestamptz NOT NULL,
    last_event_seq    bigint NOT NULL,
    UNIQUE (repository_id, external_number)
);
CREATE INDEX changes_merge_sha_idx ON changes (repository_id, merge_commit_sha);
CREATE INDEX changes_head_sha_idx ON changes (repository_id, head_sha);

-- CI results. Conclusion stays free text: GitHub adds values over time.
CREATE TABLE check_runs (
    id              uuid PRIMARY KEY,
    workspace_id    uuid NOT NULL REFERENCES workspaces (id),
    repository_id   uuid NOT NULL REFERENCES repositories (id),
    external_id     bigint NOT NULL,
    name            text NOT NULL,
    head_sha        text NOT NULL,
    status          text NOT NULL,             -- queued | in_progress | completed | ...
    conclusion      text,                      -- success | failure | cancelled | timed_out | ...
    started_at      timestamptz,
    completed_at    timestamptz,
    url             text,
    last_event_seq  bigint NOT NULL,
    UNIQUE (repository_id, external_id)
);
CREATE INDEX check_runs_sha_idx ON check_runs (repository_id, head_sha);

-- ---------------------------------------------------------------------------
-- Releases and metrics
-- ---------------------------------------------------------------------------

CREATE TABLE releases (
    id                   uuid PRIMARY KEY,
    workspace_id         uuid NOT NULL REFERENCES workspaces (id),
    repository_id        uuid REFERENCES repositories (id),
    environment          text NOT NULL DEFAULT 'production',
    -- GitHub deployment id (or a caller-supplied id for API releases), so later
    -- status updates find the row an earlier status created.
    external_id          text,
    version              text,
    commit_sha           text NOT NULL,
    previous_commit_sha  text,                 -- filled by the inclusion job
    status               text NOT NULL CHECK (status IN ('pending', 'in_progress', 'success', 'failure')),
    source               text NOT NULL CHECK (source IN ('github_deployment', 'api')),
    expected_metric_id   uuid,                 -- FK added below, after `metrics` exists
    deployed_at          timestamptz,
    last_event_seq       bigint NOT NULL
);
CREATE INDEX releases_env_idx ON releases (workspace_id, environment, deployed_at DESC);
CREATE UNIQUE INDEX releases_external_idx ON releases (workspace_id, source, external_id)
    WHERE external_id IS NOT NULL;

CREATE TABLE metrics (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id      uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    key               text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_]{1,63}$'),
    name              text NOT NULL,
    unit              text,                    -- '%', 'ms', 'count'...
    higher_is_better  boolean NOT NULL DEFAULT true,
    created_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, key)
);

ALTER TABLE releases
    ADD CONSTRAINT releases_expected_metric_fk
    FOREIGN KEY (expected_metric_id) REFERENCES metrics (id) ON DELETE SET NULL;

CREATE TABLE metric_points (
    metric_id  uuid NOT NULL REFERENCES metrics (id) ON DELETE CASCADE,
    ts         timestamptz NOT NULL,
    value      double precision NOT NULL,
    PRIMARY KEY (metric_id, ts)
);

-- The thin end of the loop: did the release move its metric?
CREATE TABLE release_outcomes (
    release_id    uuid NOT NULL REFERENCES releases (id) ON DELETE CASCADE,
    metric_id     uuid NOT NULL REFERENCES metrics (id) ON DELETE CASCADE,
    window_hours  integer NOT NULL DEFAULT 72 CHECK (window_hours > 0),
    before_value  double precision,
    after_value   double precision,
    before_count  integer,
    after_count   integer,
    computed_at   timestamptz,
    PRIMARY KEY (release_id, metric_id)
);

-- ---------------------------------------------------------------------------
-- The graph: typed edges between any two nodes
-- ---------------------------------------------------------------------------

CREATE TABLE edges (
    workspace_id          uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    from_type             text NOT NULL,
    from_id               uuid NOT NULL,
    kind                  text NOT NULL CHECK (kind IN (
                              'supports',     -- signal   -> idea      (v4)
                              'decided_in',   -- idea     -> decision  (v3)
                              'planned_as',   -- decision -> work_item (v3)
                              'implements',   -- change   -> work_item (v1)
                              'verifies',     -- check_run -> change   (v1)
                              'ships',        -- release  -> change    (v1)
                              'measures',     -- release  -> metric    (v1)
                              'feeds'         -- outcome  -> signal    (v4)
                          )),
    to_type               text NOT NULL,
    to_id                 uuid NOT NULL,
    source                text NOT NULL CHECK (source IN ('manual', 'derived')),
    created_by_event_seq  bigint NOT NULL,
    created_at            timestamptz NOT NULL,     -- from the event, so a rebuild matches
    last_event_seq        bigint NOT NULL,          -- bumped on removal, so sync sends it
    -- Edges are soft-deleted. A derived edge a person removed stays here with
    -- removed_by_user = true, so the linker never re-creates it.
    removed_at            timestamptz,
    removed_by_user       boolean NOT NULL DEFAULT false,
    PRIMARY KEY (workspace_id, from_type, from_id, kind, to_type, to_id)
);
CREATE INDEX edges_to_idx ON edges (workspace_id, to_type, to_id) WHERE removed_at IS NULL;
CREATE INDEX edges_from_idx ON edges (workspace_id, from_type, from_id) WHERE removed_at IS NULL;

-- ---------------------------------------------------------------------------
-- Background jobs (Postgres-backed queue, see docs/adr/0003-postgres-job-queue.md)
-- ---------------------------------------------------------------------------

CREATE TABLE jobs (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind          text NOT NULL,               -- e.g. 'github.process_delivery'
    payload       jsonb NOT NULL DEFAULT '{}'::jsonb,
    status        text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'done', 'dead')),
    attempts      integer NOT NULL DEFAULT 0,
    max_attempts  integer NOT NULL DEFAULT 10,
    run_at        timestamptz NOT NULL DEFAULT now(),
    locked_at     timestamptz,
    locked_by     text,
    last_error    text,
    dedupe_key    text UNIQUE,                 -- enqueue twice with the same key = one job
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_ready_idx ON jobs (run_at) WHERE status = 'queued';
CREATE INDEX jobs_stuck_idx ON jobs (locked_at) WHERE status = 'running';

-- +goose Down
DROP TABLE IF EXISTS jobs;
DROP TABLE IF EXISTS edges;
DROP TABLE IF EXISTS release_outcomes;
DROP TABLE IF EXISTS metric_points;
ALTER TABLE IF EXISTS releases DROP CONSTRAINT IF EXISTS releases_expected_metric_fk;
DROP TABLE IF EXISTS metrics;
DROP TABLE IF EXISTS releases;
DROP TABLE IF EXISTS check_runs;
DROP TABLE IF EXISTS changes;
DROP TABLE IF EXISTS webhook_deliveries;
DROP TABLE IF EXISTS repositories;
DROP TABLE IF EXISTS github_installations;
DROP TABLE IF EXISTS work_items;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS api_tokens;
DROP TABLE IF EXISTS workspace_invites;
DROP TABLE IF EXISTS workspace_members;
DROP TABLE IF EXISTS workspaces;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS users;
