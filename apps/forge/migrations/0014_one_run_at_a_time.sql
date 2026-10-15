-- At most one AI run waiting or running per service and per issue, even
-- when two requests race (the second gets a conflict).
CREATE UNIQUE INDEX ai_runs_one_active_per_service ON ai_runs (service_id)
    WHERE status IN ('queued', 'running') AND service_id IS NOT NULL;
CREATE UNIQUE INDEX ai_runs_one_active_per_issue ON ai_runs (issue_id)
    WHERE status IN ('queued', 'running') AND issue_id IS NOT NULL;
