-- The GitHub connection: installations of Forge's GitHub App, the
-- repositories they give access to, and links from issues to the pull
-- requests and commits that mention them.

CREATE TABLE github_installations (
    installation_id bigint PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- The GitHub organisation or user it's installed on.
    account text NOT NULL,
    created_by uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX github_installations_workspace_idx ON github_installations (workspace_id);

CREATE TABLE github_repositories (
    installation_id bigint NOT NULL REFERENCES github_installations (installation_id) ON DELETE CASCADE,
    -- owner/name
    full_name text NOT NULL,
    default_branch text NOT NULL DEFAULT 'main',
    -- When the server's copy was last brought up to date, and why it failed if it did.
    mirrored_at timestamptz,
    mirror_error text,
    PRIMARY KEY (installation_id, full_name)
);
CREATE INDEX github_repositories_name_idx ON github_repositories (full_name);

CREATE TABLE issue_links (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    issue_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('pull_request', 'commit')),
    url text NOT NULL,
    title text NOT NULL,
    -- owner/name
    repository text NOT NULL,
    number integer,
    sha text,
    state text NOT NULL CHECK (state IN ('open', 'merged', 'closed', 'pushed')),
    -- GitHub login of the author.
    author text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (issue_id, url),
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE
);
