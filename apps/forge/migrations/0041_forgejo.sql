-- APP-9: Forgejo pull requests move issues, and work that needs no code
-- says so.

-- A workspace's Forgejo server: Forge asks it about the watched
-- repositories' pull requests about once a minute. The token never leaves
-- the server, except to that Forgejo.
CREATE TABLE forgejo_connections (
    workspace_id uuid PRIMARY KEY REFERENCES workspaces (id) ON DELETE CASCADE,
    server text NOT NULL,
    token text NOT NULL,
    repositories text[] NOT NULL,
    connected_at timestamptz NOT NULL DEFAULT now(),
    -- The last check that worked, and the last one that didn't, with why.
    checked_at timestamptz,
    failed_at timestamptz,
    check_error text
);

-- Which code host a link came from; for Forgejo pull requests, what their
-- checks say and whether they're drafts.
ALTER TABLE issue_links
    ADD COLUMN provider text NOT NULL DEFAULT 'github' CHECK (provider IN ('github', 'forgejo')),
    ADD COLUMN checks text CHECK (checks IN ('none', 'running', 'failed', 'passed')),
    ADD COLUMN draft boolean NOT NULL DEFAULT false;

-- The issue's work changes no code, so its review needs no pull request.
ALTER TABLE issues ADD COLUMN no_code boolean NOT NULL DEFAULT false;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
