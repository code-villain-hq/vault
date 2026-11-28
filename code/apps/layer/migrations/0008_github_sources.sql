-- Older clients still see one legacy GitHub connection, never the other
-- hosts' settings in place of it.
ALTER TABLE connections DROP CONSTRAINT connections_kind_check;
ALTER TABLE connections ADD CONSTRAINT connections_kind_check
    CHECK (kind IN ('jira', 'github', 'github_sources', 'slack', 'grafana', 'tracker'));
