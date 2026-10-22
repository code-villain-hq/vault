-- People and groups from the Code Villain account (Pocket ID), pushed over
-- SCIM. Each app copies this into a migration of its own, as it did the sync
-- engine's tables. The app keeps workspaces and roles; these tables only say
-- who is in which group.

-- A person the account has sent. Their SCIM id is their account's id here,
-- and `external_id` is the account's own id, which signing in calls `sub`.
CREATE TABLE scim_users (
    user_id uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    external_id text NOT NULL UNIQUE,
    user_name text NOT NULL,
    -- A person turned off in the account is in no group here.
    active boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE scim_groups (
    id uuid PRIMARY KEY,
    external_id text UNIQUE,
    -- The group's friendly name in the account, as a name: "Forge admins" is
    -- "forge-admins" (group_key).
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX scim_groups_name_idx ON scim_groups (name);

CREATE TABLE scim_group_members (
    group_id uuid NOT NULL REFERENCES scim_groups (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX scim_group_members_user_idx ON scim_group_members (user_id);

-- The groups a workspace follows, from the server's settings: its members,
-- admins and guests. Empty on the desktop app's own server.
ALTER TABLE workspaces ADD COLUMN users_group text;
ALTER TABLE workspaces ADD COLUMN admins_group text;
ALTER TABLE workspaces ADD COLUMN guests_group text;
-- The group someone's role comes from; empty for an agent, or someone here
-- before the account sent its groups.
ALTER TABLE workspace_members ADD COLUMN from_group text;
