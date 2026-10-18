-- An issue's reviewer: the person asked to review it. Entering review
-- notifies them ("Ready for review" in the Notifications spec).
ALTER TABLE issues
    ADD COLUMN reviewer_id uuid,
    ADD FOREIGN KEY (workspace_id, reviewer_id) REFERENCES workspace_members (workspace_id, user_id)
        ON DELETE SET NULL (reviewer_id);

ALTER TABLE notifications
    DROP CONSTRAINT notifications_type_check,
    ADD CONSTRAINT notifications_type_check CHECK (type IN ('assigned', 'comment', 'completed', 'suggestion', 'mention', 'spec_conflict', 'review'));

UPDATE workspaces SET sync_epoch = gen_random_uuid();
