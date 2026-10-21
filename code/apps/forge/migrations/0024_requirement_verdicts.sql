-- Drift checks give each requirement of a behaviour spec a verdict (FOR-32).
-- A requirement's name is its identity, so there's one verdict per spec and
-- name: the latest. A renamed requirement starts without one.
ALTER TABLE specs ADD UNIQUE (id, workspace_id);

CREATE TABLE requirement_verdicts (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    spec_id uuid NOT NULL,
    requirement text NOT NULL CHECK (length(btrim(requirement)) > 0),
    verdict text NOT NULL CHECK (verdict IN ('met', 'not_met', 'unclear')),
    -- Markdown: the evidence (files and lines), or why it's unclear.
    evidence text NOT NULL DEFAULT '',
    -- The requirement as it read when checked: the verdict is stale once it reads differently.
    requirement_text text NOT NULL DEFAULT '',
    -- The spec's version that was checked.
    spec_updated_at timestamptz NOT NULL,
    -- The latest commit touching the service's code when checked.
    commit_sha text,
    run_id uuid,
    checked_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    UNIQUE (spec_id, requirement),
    FOREIGN KEY (spec_id, workspace_id) REFERENCES specs (id, workspace_id) ON DELETE CASCADE,
    FOREIGN KEY (run_id, workspace_id) REFERENCES ai_runs (id, workspace_id) ON DELETE SET NULL (run_id)
);

-- A push that changes a service's code makes the verdicts checked before it stale.
ALTER TABLE services ADD COLUMN code_changed_at timestamptz;

-- A drift finding can name the requirement it comes from.
ALTER TABLE suggestions
    ADD COLUMN spec_id uuid,
    ADD COLUMN requirement text,
    ADD FOREIGN KEY (spec_id, workspace_id) REFERENCES specs (id, workspace_id) ON DELETE SET NULL (spec_id);

UPDATE workspaces SET sync_epoch = gen_random_uuid();
