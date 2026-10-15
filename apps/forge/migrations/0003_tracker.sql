-- The tracker: teams with their own workflow states, labels, projects,
-- initiatives, relations between issues, and comments.
--
-- References between records use composite foreign keys that include
-- workspace_id, so Postgres itself guarantees a record can only point at
-- records in its own workspace.

ALTER TABLE issues ADD CONSTRAINT issues_id_workspace_key UNIQUE (id, workspace_id);

CREATE TABLE teams (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    -- Prefix of the team's issue identifiers, e.g. FOR in FOR-12.
    key text NOT NULL CHECK (key ~ '^[A-Z][A-Z0-9]{0,4}$'),
    issue_counter integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, key),
    UNIQUE (id, workspace_id)
);

CREATE TABLE workflow_states (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    team_id uuid NOT NULL,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    type text NOT NULL CHECK (type IN ('triage', 'backlog', 'unstarted', 'started', 'completed', 'canceled')),
    color text NOT NULL DEFAULT '#8a8a94',
    position double precision NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (team_id, workspace_id) REFERENCES teams (id, workspace_id) ON DELETE CASCADE,
    UNIQUE (id, team_id)
);

CREATE TABLE labels (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- NULL for labels every team can use.
    team_id uuid,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    color text NOT NULL DEFAULT '#8a8a94',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (team_id, workspace_id) REFERENCES teams (id, workspace_id) ON DELETE CASCADE
);

CREATE TABLE initiatives (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    description text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'planned' CHECK (status IN ('planned', 'active', 'completed')),
    owner_id uuid,
    target_date date,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    FOREIGN KEY (workspace_id, owner_id) REFERENCES workspace_members (workspace_id, user_id)
        ON DELETE SET NULL (owner_id)
);

CREATE TABLE projects (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    description text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'planned'
        CHECK (status IN ('planned', 'started', 'paused', 'completed', 'canceled')),
    lead_id uuid,
    initiative_id uuid,
    target_date date,
    sort_order double precision NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    FOREIGN KEY (workspace_id, lead_id) REFERENCES workspace_members (workspace_id, user_id)
        ON DELETE SET NULL (lead_id),
    FOREIGN KEY (initiative_id, workspace_id) REFERENCES initiatives (id, workspace_id)
        ON DELETE SET NULL (initiative_id)
);

-- Every existing workspace gets a team named after it, with the default
-- workflow, and its issues move into that team.
INSERT INTO teams (id, workspace_id, name, key, issue_counter)
SELECT gen_random_uuid(), w.id, w.name,
       CASE WHEN k.key ~ '^[A-Z]' THEN k.key ELSE 'TEAM' END,
       w.issue_counter
FROM workspaces w
CROSS JOIN LATERAL (SELECT upper(left(regexp_replace(w.name, '[^A-Za-z0-9]', '', 'g'), 3)) AS key) k;

INSERT INTO workflow_states (id, workspace_id, team_id, name, type, color, position)
SELECT gen_random_uuid(), t.workspace_id, t.id, s.name, s.type, s.color, s.position
FROM teams t
CROSS JOIN (VALUES
    ('Triage', 'triage', '#f2994a', 0),
    ('Backlog', 'backlog', '#8a8a94', 1),
    ('Todo', 'unstarted', '#b5b5bd', 2),
    ('In Progress', 'started', '#f2c94c', 3),
    ('In Review', 'started', '#4cb782', 4),
    ('Done', 'completed', '#5b5bd6', 5),
    ('Canceled', 'canceled', '#8a8a94', 6)
) AS s (name, type, color, position);

ALTER TABLE issues
    ADD COLUMN team_id uuid,
    ADD COLUMN state_id uuid,
    ADD COLUMN assignee_id uuid,
    ADD COLUMN parent_id uuid,
    ADD COLUMN project_id uuid,
    ADD COLUMN label_ids uuid[] NOT NULL DEFAULT '{}',
    ADD COLUMN due_date date,
    ADD COLUMN sort_order double precision NOT NULL DEFAULT 0,
    -- Set by a trigger when the issue moves into a completed or canceled state.
    ADD COLUMN completed_at timestamptz;

UPDATE issues i SET team_id = t.id
FROM teams t WHERE t.workspace_id = i.workspace_id;

UPDATE issues i SET state_id = s.id
FROM workflow_states s
WHERE s.team_id = i.team_id
  AND s.name = CASE i.status
      WHEN 'backlog' THEN 'Backlog'
      WHEN 'todo' THEN 'Todo'
      WHEN 'in_progress' THEN 'In Progress'
      WHEN 'done' THEN 'Done'
      ELSE 'Canceled'
  END;

ALTER TABLE issues
    ALTER COLUMN team_id SET NOT NULL,
    ALTER COLUMN state_id SET NOT NULL,
    DROP COLUMN status,
    DROP CONSTRAINT issues_workspace_id_number_key,
    ADD CONSTRAINT issues_team_number_key UNIQUE (team_id, number),
    ADD CONSTRAINT issues_not_own_parent CHECK (parent_id <> id),
    ADD FOREIGN KEY (team_id, workspace_id) REFERENCES teams (id, workspace_id) ON DELETE CASCADE,
    ADD FOREIGN KEY (state_id, team_id) REFERENCES workflow_states (id, team_id),
    ADD FOREIGN KEY (workspace_id, assignee_id) REFERENCES workspace_members (workspace_id, user_id)
        ON DELETE SET NULL (assignee_id),
    ADD FOREIGN KEY (parent_id, workspace_id) REFERENCES issues (id, workspace_id)
        ON DELETE SET NULL (parent_id),
    ADD FOREIGN KEY (project_id, workspace_id) REFERENCES projects (id, workspace_id)
        ON DELETE SET NULL (project_id);

ALTER TABLE workspaces DROP COLUMN issue_counter;

CREATE INDEX issues_team_idx ON issues (team_id);
CREATE INDEX issues_parent_idx ON issues (parent_id);
CREATE INDEX issues_project_idx ON issues (project_id);

CREATE FUNCTION issues_track_completion() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    state_type text;
BEGIN
    SELECT type INTO state_type FROM workflow_states WHERE id = NEW.state_id;
    IF state_type IN ('completed', 'canceled') THEN
        IF TG_OP = 'INSERT' OR OLD.state_id IS DISTINCT FROM NEW.state_id THEN
            NEW.completed_at := now();
        END IF;
    ELSE
        NEW.completed_at := NULL;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER issues_track_completion BEFORE INSERT OR UPDATE ON issues
    FOR EACH ROW EXECUTE FUNCTION issues_track_completion();

UPDATE issues i SET completed_at = i.updated_at
FROM workflow_states s
WHERE s.id = i.state_id AND s.type IN ('completed', 'canceled');

CREATE TABLE issue_relations (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    issue_id uuid NOT NULL,
    related_issue_id uuid NOT NULL,
    -- "blocks": issue blocks related_issue. "duplicate": issue duplicates related_issue.
    type text NOT NULL CHECK (type IN ('blocks', 'related', 'duplicate')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (issue_id <> related_issue_id),
    UNIQUE (issue_id, related_issue_id, type),
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE,
    FOREIGN KEY (related_issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE
);

CREATE TABLE comments (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    issue_id uuid NOT NULL,
    author_id uuid NOT NULL REFERENCES users (id),
    body text NOT NULL CHECK (length(btrim(body)) > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE
);
CREATE INDEX comments_issue_idx ON comments (issue_id);

-- The people in each workspace, as clients sync them. Read-only: membership
-- changes go through the account API, which logs them.
CREATE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at
FROM workspace_members m
JOIN users u ON u.id = m.user_id;
