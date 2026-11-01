-- Almanac's accounts and workspaces, as in Forge, Glyph and Raven: people
-- come from the company's account (code/kit/scim), sign in with it, and are
-- in workspaces with roles. The sync engine's tables are code/kit/sync's.

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    -- The company account (OpenID Connect's `sub`) this person signs in with.
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

CREATE TABLE workspaces (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    -- The workspace's name in URLs, e.g. /home/pflanzen.
    url_key text NOT NULL UNIQUE,
    -- Changes whenever the workspace's sync history starts over (for example
    -- after a database reset), so clients know to download a fresh snapshot.
    sync_epoch uuid NOT NULL DEFAULT gen_random_uuid(),
    -- The groups whose people are its members, admins and guests.
    users_group text,
    admins_group text,
    guests_group text,
    -- The company's workspace is made on the server's first start, before
    -- anyone is in it.
    set_up boolean NOT NULL DEFAULT true,
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
    role text NOT NULL CHECK (role IN ('admin', 'member', 'guest')),
    -- Goes up whenever what a guest may see changes, so their client starts
    -- over from a fresh snapshot.
    access_version bigint NOT NULL DEFAULT 0,
    -- The group someone's role comes from; empty for a role set by hand.
    from_group text,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

-- The people in each workspace, as clients sync them. Read-only: membership
-- changes go through the account API, which logs them.
CREATE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at
FROM workspace_members m
JOIN users u ON u.id = m.user_id;

-- The company's people and groups, as its account sends them over SCIM.
-- These are code/kit/scim's schema.sql.
CREATE TABLE scim_users (
    user_id uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    external_id text NOT NULL UNIQUE,
    user_name text NOT NULL,
    -- A person turned off in the account is in no group here.
    active boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE scim_groups (
    id uuid PRIMARY KEY,
    external_id text UNIQUE,
    -- The group's display name in the account, such as "almanac-admins".
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX scim_groups_name_idx ON scim_groups (name);

CREATE TABLE scim_group_members (
    group_id uuid NOT NULL REFERENCES scim_groups (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX scim_group_members_user_idx ON scim_group_members (user_id);

-- Group resources arrive separately. Give a role transfer time to finish
-- before removing assignments and ending the person's sessions.
CREATE TABLE scim_departures (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    remove_after timestamptz NOT NULL DEFAULT now() + interval '30 seconds',
    PRIMARY KEY (workspace_id, user_id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members(workspace_id, user_id) ON DELETE CASCADE
);
