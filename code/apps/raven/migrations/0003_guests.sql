-- Guests. A guest sees only the apps an admin adds them to.

ALTER TABLE workspace_members DROP CONSTRAINT workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('admin', 'member', 'guest'));
ALTER TABLE invites DROP CONSTRAINT invites_role_check;
ALTER TABLE invites ADD CONSTRAINT invites_role_check
    CHECK (role IN ('admin', 'member', 'guest'));

-- Goes up whenever what a guest may see changes (their role, their apps),
-- so their client starts over from a fresh snapshot.
ALTER TABLE workspace_members ADD COLUMN access_version bigint NOT NULL DEFAULT 0;

-- The apps a guest is added to. Admins and members see them all.
CREATE TABLE app_access (
    workspace_id uuid NOT NULL,
    app_id uuid NOT NULL REFERENCES apps (id) ON DELETE CASCADE,
    user_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (app_id, user_id),
    FOREIGN KEY (workspace_id, user_id)
        REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE
);
CREATE INDEX app_access_user_idx ON app_access (user_id, app_id);
