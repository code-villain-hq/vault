-- Knowledge: services (what exists in the system, tied to code), specs (how
-- something should work) and decisions (what was decided, and whether it
-- still holds). Issues can belong to a service.

CREATE TABLE services (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    -- Markdown: what the service is for.
    description text NOT NULL DEFAULT '',
    team_id uuid,
    -- Where its code lives: "github:owner/repo", or a path on the server.
    repository text,
    -- Paths in the repository that belong to the service, as globs.
    paths text[] NOT NULL DEFAULT '{}',
    -- Markdown: what the code actually does today. Kept up to date by the AI.
    current_state text NOT NULL DEFAULT '',
    current_state_updated_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    FOREIGN KEY (team_id, workspace_id) REFERENCES teams (id, workspace_id) ON DELETE SET NULL (team_id)
);

CREATE TABLE specs (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    title text NOT NULL CHECK (length(btrim(title)) > 0),
    -- Markdown.
    body text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'deprecated')),
    service_id uuid,
    project_id uuid,
    created_by uuid NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (service_id, workspace_id) REFERENCES services (id, workspace_id) ON DELETE SET NULL (service_id),
    FOREIGN KEY (project_id, workspace_id) REFERENCES projects (id, workspace_id) ON DELETE SET NULL (project_id)
);

CREATE TABLE decisions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    title text NOT NULL CHECK (length(btrim(title)) > 0),
    -- Markdown, in the usual decision-record shape.
    context text NOT NULL DEFAULT '',
    decision text NOT NULL DEFAULT '',
    consequences text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'accepted'
        CHECK (status IN ('proposed', 'accepted', 'superseded', 'overridden')),
    service_id uuid,
    project_id uuid,
    superseded_by_id uuid,
    decided_on date,
    created_by uuid NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    CHECK (superseded_by_id <> id),
    FOREIGN KEY (service_id, workspace_id) REFERENCES services (id, workspace_id) ON DELETE SET NULL (service_id),
    FOREIGN KEY (project_id, workspace_id) REFERENCES projects (id, workspace_id) ON DELETE SET NULL (project_id),
    FOREIGN KEY (superseded_by_id, workspace_id) REFERENCES decisions (id, workspace_id)
        ON DELETE SET NULL (superseded_by_id)
);

ALTER TABLE issues
    ADD COLUMN service_id uuid,
    ADD FOREIGN KEY (service_id, workspace_id) REFERENCES services (id, workspace_id)
        ON DELETE SET NULL (service_id);
CREATE INDEX issues_service_idx ON issues (service_id);

-- Issues gained a field outside the sync log.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
