-- An issue carries its spec change (FOR-71, decision "Specs change only when
-- the work is done"): what it changes in each spec, in OpenSpec's delta
-- format, and the decisions it replaces; or why no spec changes. One per issue.
CREATE TABLE spec_changes (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    issue_id uuid NOT NULL,
    -- Per spec: {spec_id, new_title, new_service_id, text, rewrite}.
    specs jsonb NOT NULL DEFAULT '[]',
    replaces_decision_ids uuid[] NOT NULL DEFAULT '{}',
    no_change_reason text,
    created_by uuid NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id),
    UNIQUE (issue_id),
    CHECK (no_change_reason IS NULL OR specs = '[]'::jsonb),
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE
);
