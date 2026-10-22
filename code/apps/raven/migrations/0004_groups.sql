-- Groups from the Code Villain account. The tables below are code/kit/scim's
-- schema.sql, as it is today.

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
    -- The group's display name in the account, such as "forge-admins".
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

-- The group a workspace is linked to: its people are members, "<name>-admins"
-- admins and "<name>-guests" guests.
ALTER TABLE workspaces ADD COLUMN group_name text;
-- The group someone's role comes from; empty for an invite or a role set by hand.
ALTER TABLE workspace_members ADD COLUMN from_group text;
