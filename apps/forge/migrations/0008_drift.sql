-- Drift checks: Forge's AI compares a service's code with what the tracker
-- says about it and suggests fixes for people to accept or dismiss.

CREATE TABLE ai_runs (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    service_id uuid,
    -- manual, schedule or push.
    trigger text NOT NULL CHECK (trigger IN ('manual', 'schedule', 'push')),
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    model text NOT NULL DEFAULT '',
    -- Markdown: what the run found.
    summary text NOT NULL DEFAULT '',
    error text,
    input_tokens bigint NOT NULL DEFAULT 0,
    output_tokens bigint NOT NULL DEFAULT 0,
    cost_usd double precision NOT NULL DEFAULT 0,
    suggestions integer NOT NULL DEFAULT 0,
    -- Who asked for it: a person, or Forge's AI agent for scheduled runs.
    created_by uuid NOT NULL REFERENCES users (id),
    started_at timestamptz,
    finished_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    FOREIGN KEY (service_id, workspace_id) REFERENCES services (id, workspace_id) ON DELETE SET NULL (service_id)
);
CREATE INDEX ai_runs_service_idx ON ai_runs (service_id, created_at);

-- The conversation behind a run, for anyone asking why. Not synced.
CREATE TABLE ai_run_transcripts (
    run_id uuid PRIMARY KEY REFERENCES ai_runs (id) ON DELETE CASCADE,
    messages jsonb NOT NULL
);

CREATE TABLE suggestions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    title text NOT NULL CHECK (length(btrim(title)) > 0),
    -- Markdown: what drifted, and the evidence.
    body text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'accepted', 'dismissed')),
    -- The proposed changes in words, and as sync operations to apply on accept.
    changes text[] NOT NULL DEFAULT '{}',
    operations jsonb NOT NULL DEFAULT '[]',
    service_id uuid,
    issue_id uuid,
    decision_id uuid,
    run_id uuid,
    created_by uuid NOT NULL REFERENCES users (id),
    resolved_by uuid REFERENCES users (id),
    resolved_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    FOREIGN KEY (service_id, workspace_id) REFERENCES services (id, workspace_id) ON DELETE SET NULL (service_id),
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE SET NULL (issue_id),
    FOREIGN KEY (decision_id, workspace_id) REFERENCES decisions (id, workspace_id) ON DELETE SET NULL (decision_id),
    FOREIGN KEY (run_id, workspace_id) REFERENCES ai_runs (id, workspace_id) ON DELETE SET NULL (run_id)
);
CREATE INDEX suggestions_open_idx ON suggestions (workspace_id) WHERE status = 'open';

ALTER TABLE notifications
    DROP CONSTRAINT notifications_type_check,
    ADD CONSTRAINT notifications_type_check CHECK (type IN ('assigned', 'comment', 'completed', 'suggestion')),
    ADD COLUMN suggestion_id uuid;

-- Notifications gained a field outside the sync log.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
