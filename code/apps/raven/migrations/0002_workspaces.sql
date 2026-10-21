-- Workspaces: a team's space with its members and roles, as in Forge and
-- Glyph, holding the apps the team watches. The sync engine's tables
-- (code/kit/sync) and the accounts (members, invites) are the same as
-- Glyph's. Only the watched apps and the members sync; calls, logs and
-- checks stay on the server.

CREATE TABLE workspaces (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    -- The workspace's name in URLs, e.g. /studio/forge.
    url_key text NOT NULL UNIQUE,
    -- Changes whenever the workspace's sync history starts over (for example
    -- after a database reset), so clients know to download a fresh snapshot.
    sync_epoch uuid NOT NULL DEFAULT gen_random_uuid(),
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

-- What was watched before workspaces goes to one workspace, "Code Villain":
-- the first person is its admin and everyone else a member, as everyone saw
-- every app. A Raven nobody has signed up to yet (a new server, or the one
-- on this Mac) starts with no apps: the first workspace gets Raven itself.
DELETE FROM apps WHERE NOT EXISTS (SELECT 1 FROM users);

INSERT INTO workspaces (id, name, url_key)
SELECT uuidv7(), 'Code Villain', 'code-villain'
WHERE EXISTS (SELECT 1 FROM users);

INSERT INTO workspace_members (workspace_id, user_id, role)
SELECT w.id, u.id,
       CASE WHEN row_number() OVER (ORDER BY u.created_at, u.id) = 1 THEN 'admin' ELSE 'member' END
FROM users u CROSS JOIN workspaces w;

-- An app belongs to a workspace and syncs. `key` is still what it calls
-- itself in OpenTelemetry (`service.name`), now unique in its workspace.
ALTER TABLE apps
    ADD COLUMN id uuid,
    ADD COLUMN workspace_id uuid REFERENCES workspaces (id) ON DELETE CASCADE,
    -- Raven itself: it records its own calls here. The server sets it.
    ADD COLUMN is_self boolean NOT NULL DEFAULT false,
    ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now();
UPDATE apps SET id = uuidv7(),
                workspace_id = (SELECT id FROM workspaces),
                is_self = (key = 'raven'),
                health_url = CASE WHEN key = 'raven' THEN NULL ELSE health_url END;

-- An app's ingest key (stored hashed), apart from the synced record so no
-- client ever gets a hash.
CREATE TABLE app_keys (
    app_id uuid PRIMARY KEY,
    key_hash bytea NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO app_keys (app_id, key_hash)
SELECT id, ingest_key_hash FROM apps WHERE ingest_key_hash IS NOT NULL;

-- What Raven heard refers to the app by its id.
ALTER TABLE health_checks ADD COLUMN app_id uuid;
ALTER TABLE spans ADD COLUMN app_id uuid;
ALTER TABLE logs ADD COLUMN app_id uuid;
UPDATE health_checks h SET app_id = a.id FROM apps a WHERE a.key = h.app;
UPDATE spans s SET app_id = a.id FROM apps a WHERE a.key = s.app;
UPDATE logs l SET app_id = a.id FROM apps a WHERE a.key = l.app;
-- Dropping `app` drops its foreign keys and indexes too.
ALTER TABLE health_checks DROP COLUMN app;
ALTER TABLE spans DROP COLUMN app;
ALTER TABLE logs DROP COLUMN app;

ALTER TABLE apps DROP CONSTRAINT apps_pkey;
ALTER TABLE apps DROP COLUMN ingest_key_hash;
ALTER TABLE apps ALTER COLUMN id SET NOT NULL, ADD PRIMARY KEY (id);
ALTER TABLE apps ALTER COLUMN workspace_id SET NOT NULL;
ALTER TABLE apps ADD CONSTRAINT apps_workspace_id_key_key UNIQUE (workspace_id, key);
-- Only one app is Raven itself.
CREATE UNIQUE INDEX apps_one_self_idx ON apps (is_self) WHERE is_self;
ALTER TABLE app_keys ADD FOREIGN KEY (app_id) REFERENCES apps (id) ON DELETE CASCADE;

ALTER TABLE health_checks ALTER COLUMN app_id SET NOT NULL,
    ADD FOREIGN KEY (app_id) REFERENCES apps (id) ON DELETE CASCADE;
ALTER TABLE spans ALTER COLUMN app_id SET NOT NULL,
    ADD FOREIGN KEY (app_id) REFERENCES apps (id) ON DELETE CASCADE;
ALTER TABLE logs ALTER COLUMN app_id SET NOT NULL,
    ADD FOREIGN KEY (app_id) REFERENCES apps (id) ON DELETE CASCADE;
CREATE INDEX health_checks_app_time ON health_checks (app_id, checked_at DESC);
CREATE INDEX spans_actions ON spans (app_id, started_at) WHERE kind = 'server';
CREATE INDEX spans_errors ON spans (app_id, started_at) WHERE is_error;
CREATE INDEX logs_app_time ON logs (app_id, at DESC);
