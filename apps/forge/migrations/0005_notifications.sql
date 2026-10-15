-- Notifications, and log entries only one person may see.

-- NULL: everyone in the workspace sees the entry. Otherwise only that user.
ALTER TABLE sync_log ADD COLUMN visible_to uuid;

CREATE TABLE notifications (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- Who it's for.
    user_id uuid NOT NULL,
    type text NOT NULL CHECK (type IN ('assigned', 'comment', 'completed')),
    issue_id uuid,
    -- Not a foreign key: a notification outlives a deleted comment.
    comment_id uuid,
    -- Who caused it.
    actor_id uuid REFERENCES users (id) ON DELETE SET NULL,
    read_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE
);
CREATE INDEX notifications_user_idx ON notifications (workspace_id, user_id);
CREATE INDEX notifications_issue_idx ON notifications (issue_id);
