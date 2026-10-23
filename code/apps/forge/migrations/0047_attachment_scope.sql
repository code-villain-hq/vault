-- Attachments follow their issue, or their team while a new issue is a draft.
ALTER TABLE attachments ADD COLUMN issue_id uuid REFERENCES issues(id) ON DELETE SET NULL;
ALTER TABLE attachments ADD COLUMN team_id uuid REFERENCES teams(id) ON DELETE SET NULL;
CREATE INDEX attachments_issue_idx ON attachments(issue_id);
-- Associate existing files once. Future pasted URLs cannot change ownership.
UPDATE attachments a SET issue_id = (
 SELECT i.id FROM issues i WHERE i.workspace_id = a.workspace_id AND (
 strpos(i.description, '/api/w/' || a.workspace_id || '/attachments/' || a.id) > 0 OR EXISTS (
 SELECT 1 FROM comments c WHERE c.issue_id = i.id AND
 strpos(c.body, '/api/w/' || a.workspace_id || '/attachments/' || a.id) > 0))
 ORDER BY i.created_at, i.id LIMIT 1
);
