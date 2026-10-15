-- Agents: users that act through API tokens rather than signing in, so
-- every change an AI makes is attributed to it.

ALTER TABLE users ADD COLUMN kind text NOT NULL DEFAULT 'person' CHECK (kind IN ('person', 'agent'));

-- Only a hash of each token is stored. A token belongs to one agent in one workspace.
CREATE TABLE api_tokens (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    token_hash bytea NOT NULL UNIQUE,
    created_by uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at timestamptz,
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE
);

-- Clients learn which members are agents.
CREATE OR REPLACE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at, u.kind
FROM workspace_members m
JOIN users u ON u.id = m.user_id;

-- User records gained a field outside the sync log.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
