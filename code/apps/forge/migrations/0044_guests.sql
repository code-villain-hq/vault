-- Guests and team members. A guest sees only the teams they're in; admins
-- and members are in every team unless an admin takes them out of one.

ALTER TABLE workspace_members DROP CONSTRAINT workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('admin', 'member', 'guest'));
ALTER TABLE invites DROP CONSTRAINT invites_role_check;
ALTER TABLE invites ADD CONSTRAINT invites_role_check
    CHECK (role IN ('admin', 'member', 'guest'));

-- Goes up whenever what a guest may see changes (their role, their teams),
-- so their client starts over from a fresh snapshot.
ALTER TABLE workspace_members ADD COLUMN access_version bigint NOT NULL DEFAULT 0;

CREATE TABLE team_members (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    team_id uuid NOT NULL,
    user_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (team_id, user_id),
    FOREIGN KEY (team_id, workspace_id) REFERENCES teams (id, workspace_id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members (workspace_id, user_id)
        ON DELETE CASCADE
);
CREATE INDEX team_members_user_idx ON team_members (user_id, team_id);

-- Everyone here today is in every team, so nothing changes for anyone.
INSERT INTO team_members (id, workspace_id, team_id, user_id)
SELECT gen_random_uuid(), t.workspace_id, t.id, m.user_id
FROM teams t JOIN workspace_members m ON m.workspace_id = t.workspace_id;

-- Clients gain a model outside the sync log.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
