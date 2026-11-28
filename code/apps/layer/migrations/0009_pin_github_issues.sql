-- Keep the workspace's original Issues source even if the legacy Code
-- connection is changed or removed before a new desktop joins.
WITH legacy AS (
    SELECT workspace_id,
           rtrim(lower(substring(
               settings->>'web_url' FROM
               '(?i)^https?://(\[[0-9a-f:.]+\]|[^:/?#@[:space:]]+)(?::[0-9]+)?(?:/[^?#]*)?$'
           )), '.') AS host
    FROM connections
    WHERE kind = 'github'
), pinned AS (
    UPDATE connections AS tracker
    SET settings = jsonb_set(tracker.settings, '{github_source}', to_jsonb(legacy.host)),
        updated_at = now()
    FROM legacy
    WHERE tracker.workspace_id = legacy.workspace_id
      AND tracker.kind = 'tracker'
      AND tracker.settings->>'uses' = 'github'
      AND tracker.settings->>'github_source' IS NULL
      AND legacy.host <> ''
    RETURNING tracker.workspace_id
)
-- The direct write has no sync log entry. Refresh existing copies too.
UPDATE workspaces SET sync_epoch = gen_random_uuid()
WHERE id IN (SELECT workspace_id FROM pinned);
