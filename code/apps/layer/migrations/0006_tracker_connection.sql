-- Which connection a workspace's tickets come from (TKT-15) is a connection
-- of its own kind, so it is one per workspace and only admins change it.
ALTER TABLE connections DROP CONSTRAINT connections_kind_check;
ALTER TABLE connections ADD CONSTRAINT connections_kind_check
    CHECK (kind IN ('jira', 'github', 'slack', 'grafana', 'tracker'));
