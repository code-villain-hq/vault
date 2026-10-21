-- Files people attach to descriptions and comments, such as screenshots.
-- The bytes live on disk under FORGE_DATA_DIR/attachments; Markdown refers
-- to them by URL. Not synced: clients fetch them when shown.
CREATE TABLE attachments (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    content_type text NOT NULL,
    size bigint NOT NULL,
    created_by uuid NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL DEFAULT now()
);
