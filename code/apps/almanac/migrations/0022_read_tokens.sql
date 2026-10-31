-- A token a member gives another app, such as Claude Code, to read their
-- workspace's topics over MCP (`/api/mcp/read`): one each, a new one in
-- place of the last. Never synced.
CREATE TABLE read_tokens (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    made_at timestamptz NOT NULL DEFAULT now(),
    used_at timestamptz,
    PRIMARY KEY (workspace_id, user_id)
);
