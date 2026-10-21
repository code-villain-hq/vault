-- Drift checks agents run outside Forge and report through MCP (APP-10).
-- They're runs like Forge AI's, with trigger 'agent': never queued, never
-- picked up by the worker, and left alone when the server restarts.

ALTER TABLE ai_runs
    DROP CONSTRAINT ai_runs_trigger_check,
    ADD CONSTRAINT ai_runs_trigger_check CHECK (trigger IN ('manual', 'schedule', 'push', 'new_issue', 'agent')),
    -- The services an agent's check covers; Forge AI's runs use service_id.
    ADD COLUMN service_ids uuid[] NOT NULL DEFAULT '{}',
    ADD COLUMN commit_from text,
    ADD COLUMN commit_to text,
    -- The checker's name and version, e.g. "vault-drift-check v2".
    ADD COLUMN checker text,
    ADD COLUMN full_audit boolean,
    -- The agent's own key for the check, so a retried start finds it.
    ADD COLUMN run_key text,
    ADD COLUMN last_report_at timestamptz,
    -- Verdicts so far, by kind, and planned requirements still without one.
    ADD COLUMN tally jsonb,
    -- Usage per part as the agent reported it; null when it didn't.
    ADD COLUMN usage jsonb;
CREATE UNIQUE INDEX ai_runs_run_key ON ai_runs (workspace_id, run_key) WHERE run_key IS NOT NULL;
CREATE INDEX ai_runs_service_ids ON ai_runs USING gin (service_ids);

-- What an agent's check found, one row per requirement: a later report on
-- the same requirement replaces it. A row without a verdict is planned and
-- not judged yet. Kept per check, unlike requirement_verdicts. Not synced:
-- a check's page asks for them.
CREATE TABLE drift_check_results (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    run_id uuid NOT NULL,
    spec_id uuid,
    -- The spec's title and version as reported, kept if the spec changes or goes.
    spec_title text NOT NULL,
    spec_updated_at timestamptz NOT NULL,
    requirement text NOT NULL,
    requirement_text text NOT NULL DEFAULT '',
    verdict text CHECK (verdict IN ('met', 'not_met', 'unclear', 'skipped', 'covered')),
    -- Markdown: the evidence, or why it's unclear, skipped or covered.
    why text NOT NULL DEFAULT '',
    -- The open issue that covers it.
    issue_id uuid,
    reported_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (run_id, spec_id, requirement),
    FOREIGN KEY (run_id, workspace_id) REFERENCES ai_runs (id, workspace_id) ON DELETE CASCADE,
    FOREIGN KEY (spec_id, workspace_id) REFERENCES specs (id, workspace_id) ON DELETE SET NULL (spec_id),
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE SET NULL (issue_id)
);

UPDATE workspaces SET sync_epoch = gen_random_uuid();
