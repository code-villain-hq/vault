-- A server runs for one company: people come from its account and its
-- groups (code/kit/scim), named in the server's settings. No passwords and
-- no invites.

ALTER TABLE workspaces ADD COLUMN users_group text;
ALTER TABLE workspaces ADD COLUMN admins_group text;
ALTER TABLE workspaces ADD COLUMN guests_group text;
UPDATE workspaces SET users_group = group_name || '-users',
    admins_group = group_name || '-admins', guests_group = group_name || '-guests'
    WHERE group_name IS NOT NULL;
ALTER TABLE workspaces DROP COLUMN group_name;

DROP TABLE invites;
ALTER TABLE users DROP COLUMN password_hash;
