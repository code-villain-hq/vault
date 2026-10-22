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

-- The company's workspace is made on the server's first start, before
-- anyone is in it; its first team comes with the first person.
ALTER TABLE workspaces ADD COLUMN set_up boolean NOT NULL DEFAULT true;

DROP TABLE invites;
ALTER TABLE users DROP COLUMN password_hash;
