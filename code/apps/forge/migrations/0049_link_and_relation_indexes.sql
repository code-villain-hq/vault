-- Open pull requests are looked up by workspace and repository (and number)
-- whenever GitHub or Forgejo reports on one, and when Forgejo is checked.
CREATE INDEX issue_links_open_pull_requests_idx ON issue_links (workspace_id, repository, number)
    WHERE kind = 'pull_request' AND state = 'open';

-- An issue's relations are found from either end (deleting an issue, and the
-- cascade from its related end); the unique key covers only issue_id.
CREATE INDEX issue_relations_related_issue_idx ON issue_relations (related_issue_id);
