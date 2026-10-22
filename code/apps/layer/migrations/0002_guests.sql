-- Guests. A guest sees only the repositories an admin adds them to, and
-- their notes, and works there as a member does.

ALTER TABLE workspace_members DROP CONSTRAINT workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('admin', 'member', 'guest'));
ALTER TABLE invites DROP CONSTRAINT invites_role_check;
ALTER TABLE invites ADD CONSTRAINT invites_role_check
    CHECK (role IN ('admin', 'member', 'guest'));

-- Goes up whenever what a guest may see changes (their role, their
-- repositories), so their client starts over from a fresh snapshot.
ALTER TABLE workspace_members ADD COLUMN access_version bigint NOT NULL DEFAULT 0;

-- The repositories a guest is added to. Admins and members have them all.
CREATE TABLE repository_access (
    workspace_id uuid NOT NULL,
    repository_id uuid NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    user_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (repository_id, user_id),
    FOREIGN KEY (workspace_id, user_id)
        REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE
);
CREATE INDEX repository_access_user_idx ON repository_access (user_id, repository_id);
