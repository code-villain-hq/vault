-- Accounts: passwords, workspace membership with roles, sessions and invite links.

ALTER TABLE users ADD COLUMN password_hash text;

-- The workspace's name in URLs, e.g. /forge/issues.
ALTER TABLE workspaces ADD COLUMN url_key text;
UPDATE workspaces SET url_key = 'workspace-' || left(id::text, 8);
ALTER TABLE workspaces ALTER COLUMN url_key SET NOT NULL;
ALTER TABLE workspaces ADD CONSTRAINT workspaces_url_key_key UNIQUE (url_key);

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
