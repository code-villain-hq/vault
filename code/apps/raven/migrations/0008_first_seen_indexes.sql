-- The Technical page says when each error and each version was first
-- seen, which may be weeks before the window it shows. These find that
-- first call directly instead of reading all 30 days. Names, messages and
-- versions are indexed by their md5, so a long one never turns a batch away.
CREATE INDEX spans_error_firsts ON spans (
    app_id,
    md5(name),
    md5(coalesce(error_message, 'HTTP ' || http_status::text, 'failed')),
    started_at
) WHERE kind = 'server' AND is_error;
CREATE INDEX spans_version_firsts ON spans (app_id, md5(version), started_at)
    WHERE kind = 'server' AND version IS NOT NULL;
