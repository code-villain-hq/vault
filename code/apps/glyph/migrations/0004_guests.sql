-- Guests. A guest sees the source language and the languages an admin adds
-- them to, and translates there as a member does.

ALTER TABLE workspace_members DROP CONSTRAINT workspace_members_role_check;
ALTER TABLE workspace_members ADD CONSTRAINT workspace_members_role_check
    CHECK (role IN ('admin', 'member', 'guest'));
ALTER TABLE invites DROP CONSTRAINT invites_role_check;
ALTER TABLE invites ADD CONSTRAINT invites_role_check
    CHECK (role IN ('admin', 'member', 'guest'));

-- Goes up whenever what a guest may see changes (their role, their
-- languages, the source language), so their client starts over from a
-- fresh snapshot.
ALTER TABLE workspace_members ADD COLUMN access_version bigint NOT NULL DEFAULT 0;

-- The languages a guest is added to. Admins and members have them all.
CREATE TABLE language_access (
    workspace_id uuid NOT NULL,
    language_id uuid NOT NULL REFERENCES languages (id) ON DELETE CASCADE,
    user_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (language_id, user_id),
    FOREIGN KEY (workspace_id, user_id)
        REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE
);
CREATE INDEX language_access_user_idx ON language_access (user_id, language_id);
