-- Forge AI triage: when an issue arrives in triage, Forge's AI suggests its
-- labels, priority, service, project and assignee, and points out
-- duplicates, for people to accept or dismiss.

ALTER TABLE ai_runs
    ADD COLUMN kind text NOT NULL DEFAULT 'drift' CHECK (kind IN ('drift', 'triage')),
    -- The issue a triage run looks at.
    ADD COLUMN issue_id uuid,
    ADD FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE SET NULL (issue_id),
    DROP CONSTRAINT ai_runs_trigger_check,
    ADD CONSTRAINT ai_runs_trigger_check CHECK (trigger IN ('manual', 'schedule', 'push', 'new_issue'));
CREATE INDEX ai_runs_issue_idx ON ai_runs (issue_id) WHERE issue_id IS NOT NULL;

-- Triage suggestions show next to the issue in Triage instead of notifying.
ALTER TABLE suggestions
    ADD COLUMN kind text NOT NULL DEFAULT 'drift' CHECK (kind IN ('drift', 'triage'));

-- Runs and suggestions gained fields outside the sync log.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
