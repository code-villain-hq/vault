-- Signing in with GitHub: the GitHub account behind a Forge account, once
-- it has been used. Not part of the synced user record.
ALTER TABLE users ADD COLUMN github_id bigint UNIQUE;
