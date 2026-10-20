-- Signing in with Code Villain (OpenID Connect): the studio account behind a
-- Forge account, once it has been used. Not part of the synced user record.
ALTER TABLE users ADD COLUMN oidc_sub text UNIQUE;
