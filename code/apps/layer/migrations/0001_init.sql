-- Layer's schema. The sync engine's tables (code/kit/sync: workspaces,
-- users, sync_log, sync_transactions) and the accounts (members, sessions,
-- invites) are the same as Glyph's and Raven's; the synced models after
-- them are a workspace's shared setup. Tasks, panes, runs and messages stay
-- on each Mac, and so does whatever is one person's own: tokens, the Jira
-- email, Slack's notify switches, AWS profiles, where a clone is on disk.

CREATE TABLE workspaces (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    -- The workspace's name in URLs, e.g. /studio/settings.
    url_key text NOT NULL UNIQUE,
    -- Changes whenever the workspace's sync history starts over (for example
    -- after a database reset), so clients know to download a fresh snapshot.
    -- Any migration that changes synced records directly must change it too.
    sync_epoch uuid NOT NULL DEFAULT gen_random_uuid(),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    password_hash text,
    -- The studio account (Pocket ID's `sub`) this account signs in with.
    -- Not part of the synced user record.
    oidc_sub text UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Every change to a synced record, in order. Clients download a snapshot once
-- and then follow this log. Within a workspace, sync ids are assigned in
-- commit order because writes to a workspace are serialised.
CREATE TABLE sync_log (
    sync_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    transaction_id uuid NOT NULL,
    actor_id uuid NOT NULL REFERENCES users (id),
    model text NOT NULL,
    model_id uuid NOT NULL,
    action text NOT NULL CHECK (action IN ('insert', 'update', 'delete')),
    -- The whole record after the change; NULL for deletes.
    data jsonb,
    -- For a record only one person syncs, that person; NULL for everyone.
    visible_to uuid,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sync_log_workspace_idx ON sync_log (workspace_id, sync_id);
CREATE INDEX sync_log_model_idx ON sync_log (model_id, sync_id);
CREATE INDEX sync_log_transaction_idx ON sync_log (transaction_id);

-- Transactions that have been applied, so a client that retries one (because
-- it never saw the response) doesn't apply it twice.
CREATE TABLE sync_transactions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    actor_id uuid NOT NULL REFERENCES users (id),
    -- The last sync id the transaction produced; NULL if it changed nothing.
    last_sync_id bigint,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE workspace_members (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('admin', 'member')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

-- Only a hash of each session token is stored, so a leaked database can't be
-- used to sign in.
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

-- An invite link can be used by several people until it expires or an admin
-- revokes it.
CREATE TABLE invites (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    role text NOT NULL CHECK (role IN ('admin', 'member')),
    created_by uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz
);
CREATE INDEX invites_workspace_idx ON invites (workspace_id);

-- The people in each workspace, as clients sync them. Read-only: membership
-- changes go through the account API, which logs them.
CREATE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at
FROM workspace_members m
JOIN users u ON u.id = m.user_id;

-- A repository the workspace works on, the same one on every Mac by where it
-- fetches from. Where a clone is on disk is each Mac's own.
CREATE TABLE repositories (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- `github.com/acme/billing`, however the remote is spelled.
    remote_key text NOT NULL,
    name text NOT NULL,
    default_branch text NOT NULL,
    -- `group` and `check` are SQL words.
    group_name text,
    update_by text CHECK (update_by IN ('merge', 'rebase')),
    check_command text,
    spec_folder text,
    specs_in_app boolean NOT NULL DEFAULT false,
    added_by uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT repositories_workspace_remote_key UNIQUE (workspace_id, remote_key)
);

-- Where the workspace's Jira, GitHub, Slack or Grafana is: one of each.
-- `settings` is the kind's, checked by the server; never a token.
CREATE TABLE connections (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('jira', 'github', 'slack', 'grafana')),
    settings jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT connections_workspace_kind UNIQUE (workspace_id, kind)
);

-- Logs kept in S3 that agents read. The AWS profile is each person's own.
CREATE TABLE log_sources (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    region text,
    bucket text NOT NULL,
    prefix text NOT NULL DEFAULT '',
    athena jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX log_sources_workspace_idx ON log_sources (workspace_id);

-- The models each agent CLI offered as its last conversation opened.
CREATE TABLE agent_models (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    agent_id text NOT NULL,
    models jsonb NOT NULL DEFAULT '[]',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT agent_models_workspace_agent UNIQUE (workspace_id, agent_id)
);

-- Repo notes: what agents learned about a repository, for the next task on
-- it. Kept by the repository's remote key, so a note outlives the
-- repository's place in the workspace.
CREATE TABLE notes (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    repo text NOT NULL,
    text text NOT NULL,
    paths jsonb NOT NULL DEFAULT '[]',
    source text NOT NULL DEFAULT '',
    written_commit text,
    written_at timestamptz NOT NULL DEFAULT now(),
    checked_at timestamptz NOT NULL DEFAULT now(),
    checked_commit text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX notes_workspace_repo_idx ON notes (workspace_id, repo);
